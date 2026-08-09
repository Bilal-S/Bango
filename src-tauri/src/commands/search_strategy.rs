//! Search Strategy Builder (spec §8.4).
//!
//! `suggest_search_strategy` reads aims + inclusion/exclusion criteria, asks
//! the LLM to produce database-specific Boolean search strings for 8 academic
//! databases, parses the JSON response, writes a system-level audit row, and
//! returns the structured result.
//!
//! Result is NOT persisted (session-scoped Pinia store, like the AI critiques).
//! Copy-only: no database API execution.
//!
//! Pure helpers (`build_search_strategy_prompt`, `parse_search_strategy_response`)
//! are extracted as `pub fn`s so `tests/search_strategy_test.rs` can exercise
//! them without `State<DbState>` (per `docs/CLAUDE.md` §Testing).

use std::sync::Arc;

use tauri::State;

use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::models::search_strategy::SearchStrategyResult;

/// One-shot Tauri command. Reads aims + criteria + LLM config in a short-lived
/// DB lock, releases the lock for the orchestrator call, then re-locks to
/// write the audit row. Returns the parsed result for the frontend Pinia
/// store.
#[tauri::command]
pub async fn suggest_search_strategy(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
) -> Result<SearchStrategyResult, AppError> {
    // 1. Short critical section: fetch config + aims + criteria.
    let (config, aims, inclusion, exclusion) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aims = criteria_repo::get_all_aims(&conn)?;
        if aims.is_empty() {
            return Err(AppError::Validation(
                "Research aims must be defined before generating a search strategy".to_string(),
            ));
        }
        let all_criteria = criteria_repo::get_all_criteria(&conn)?;
        let inclusion: Vec<Criterion> = all_criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Inclusion))
            .cloned()
            .collect();
        let exclusion: Vec<Criterion> = all_criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Exclusion))
            .cloned()
            .collect();
        (config, aims, inclusion, exclusion)
    }; // lock released

    // 2. Build the prompt pair.
    let (system_prompt, user_prompt) = build_search_strategy_prompt(&aims, &inclusion, &exclusion);

    // 3. LLM call via the orchestrator. Uses `send_json` so the response runs
    //    through `prepare_llm_json` (strips markdown code fences + escapes raw
    //    control chars) before reaching the parser. This is the canonical entry
    //    point for all JSON-returning LLM consumers; see `llm/AGENTS.md`.
    let result = orchestrator
        .send_json(&config, &system_prompt, &user_prompt, LlmRequestType::SearchStrategy)
        .await;
    if let Err(ref e) = result {
        let err_msg = e.to_string();
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("Search strategy generation failed: {err_msg}"),
        );
    }
    let (response, _tokens) = result?;

    // 4. Parse + validate.
    let parsed = parse_search_strategy_response(&response)?;

    // 5. Write a system-level success audit row.
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        audit_repo::log_system_action(
            &conn,
            audit_repo::SystemAction::SearchStrategy,
            &format!("Generated {}-database search strategy for {} aim(s)", 8, aims.len()),
        )?;
    }

    Ok(parsed)
}

// ── Pure helpers (extracted for testability) ──────────────────────────────

/// Pure: build the (system, user) prompt pair from aims + criteria.
///
/// The system prompt embeds the full 8-database syntax cheatsheet so the LLM
/// produces syntactically correct strings per platform without guessing
/// field codes or operator conventions.
#[must_use]
pub fn build_search_strategy_prompt(
    aims: &[ResearchAim],
    inclusion: &[Criterion],
    exclusion: &[Criterion],
) -> (String, String) {
    let system = SYSTEM_PROMPT.to_string();

    // Numbered aims list.
    let aims_list: Vec<String> =
        aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();

    // Criteria sections (optional; empty slices produce no lines so the
    // prompt degrades gracefully to an aims-only input).
    let inclusion_list: Vec<String> = inclusion
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();
    let exclusion_list: Vec<String> = exclusion
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. [{}] {}", i + 1, c.priority.as_str(), c.text))
        .collect();

    let inclusion_block = if inclusion_list.is_empty() {
        "None defined.".to_string()
    } else {
        inclusion_list.join("\n")
    };
    let exclusion_block = if exclusion_list.is_empty() {
        "None defined.".to_string()
    } else {
        exclusion_list.join("\n")
    };

    let user = format!(
        r#"## Task
You are building a systematic-review search strategy. Extract the PICO concepts
from the research aims (and criteria, if present), find synonyms and spelling
variants for each concept, then compose one Boolean search string per database
using that database's syntax from the cheatsheet in your instructions.

## Research Aims
{aims}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Instructions
- Extract Population, Intervention, Comparison, and Outcome concepts from the
  aims. Omit an arm when it genuinely does not apply (e.g., observational
  reviews often have no Comparison).
- For each concept, list 3 to 6 synonyms, variant spellings, and related terms.
  Prefer precise, high-signal terms over a long list of marginal variants.
- Compose one Boolean string per database using that database's syntax from the
  cheatsheet. Join concepts with AND; join synonyms within a concept with OR
  inside parentheses.
- Exclusion handling - avoid self-canceling queries: an exclusion criterion
  that merely negates an inclusion criterion is REDUNDANT. The inclusion
  AND-groups already enforce scope, so do NOT translate a negating exclusion
  into a NOT clause (that produces a query that cancels its own inclusion
  terms and will not run). Only encode INDEPENDENT exclusions - removal
  reasons that would otherwise pass the inclusion filter (publication type,
  language, animal/in-vitro-only studies, duplicate publications, etc.) - as
  narrow, specific NOT clauses. Omit any exclusion that restates an inclusion.
- Keep each database string runnable: a few targeted NOT clauses, not many
  broad ones. If the exclusion list is long, encode only the highest-priority
  independent exclusions and note the rest in `notes`.
- Each database entry must include a short `notes` field explaining any
  database-specific choices (field codes, proximity operators, MeSH headings).
- Add a warning for Semantic Scholar: it does NOT support Boolean operators, so
  advise the user to search plain key terms there.
- Add warnings for any missing PICO concept or notable sensitivity/precision
  concerns.
- Do not use em dashes anywhere in the response.

## Response Format
Return ONLY a JSON object matching this schema (no prose, no markdown fences):
{{
  "picoBreakdown": {{
    "population": {{ "concept": "...", "synonyms": ["...", "..."] }},
    "intervention": {{ "concept": "...", "synonyms": ["...", "..."] }},
    "comparison": {{ "concept": "...", "synonyms": ["...", "..."] }},
    "outcome": {{ "concept": "...", "synonyms": ["...", "..."] }}
  }},
  "strategies": {{
    "pubmed": {{ "oneLine": "...", "notes": "..." }},
    "scopus": {{ "oneLine": "...", "notes": "..." }},
    "webOfScience": {{ "oneLine": "...", "notes": "..." }},
    "cochrane": {{ "oneLine": "...", "notes": "..." }},
    "ebscohost": {{ "oneLine": "...", "notes": "..." }},
    "jstor": {{ "oneLine": "...", "notes": "..." }},
    "sciencedirect": {{ "oneLine": "...", "notes": "..." }},
    "arxiv": {{ "oneLine": "...", "notes": "..." }}
  }},
  "warnings": [
    {{ "warningType": "non_boolean_database", "message": "Semantic Scholar: ..." }}
  ]
}}

Omit any PICO arm that does not apply. All 8 database fields are required."#,
        aims = aims_list.join("\n"),
        inclusion = inclusion_block,
        exclusion = exclusion_block,
    );

    (system, user)
}

/// Pure: parse + lightly validate the LLM JSON response.
///
/// The caller (`suggest_search_strategy`) routes the LLM response through
/// `orchestrator.send_json`, which runs `prepare_llm_json` (strips markdown
/// code fences + escapes raw control chars) before this fn sees it. So `raw`
/// is already clean JSON; this fn only deserializes + validates. Returns
/// `AppError` on malformed input (never panics).
pub fn parse_search_strategy_response(raw: &str) -> Result<SearchStrategyResult, AppError> {
    let parsed: SearchStrategyResult = serde_json::from_str(raw)
        .map_err(|e| AppError::Import(format!("Failed to parse search strategy response: {e}")))?;
    Ok(parsed)
}

/// System prompt: role + the full 8-database syntax cheatsheet + the
/// Semantic Scholar non-Boolean advisory. Inlined (no `.md` file) to keep the
/// feature self-contained, mirroring `commands::criteria`.
const SYSTEM_PROMPT: &str = "\
You are a systematic-review search strategist. You build database-specific \
Boolean search strings from a research question's PICO concepts. You return \
ONLY a JSON object, no prose, no markdown fences, no em dashes.\n\n\
DATABASE SYNTAX CHEATSHEET - follow these rules exactly for each database.\n\n\
1. PubMed\n\
   - Operators: AND, OR, NOT (MUST be uppercase)\n\
   - Field tags (append in square brackets): [tiab] (title+abstract),\n\
     [mh] (MeSH heading), [tw] (text word - title, abstract, MeSH),\n\
     [ti] (title), [au] (author)\n\
   - MeSH: use [mh] or [mesh] for Medical Subject Headings\n\
   - Wildcards: * (truncation)\n\
   - Example: (\"sugar tax\"[tiab] OR \"soda tax\"[tiab] OR\n\
     \"Sugar-Sweetened Beverages\"[mh]) AND (obesity[tiab] OR\n\
     overweight[tiab])\n\n\
2. Scopus\n\
   - Operators: AND, OR, NOT (uppercase)\n\
   - Field codes: TITLE, ABS, KEY, TITLE-ABS-KEY (recommended default:\n\
     searches title + abstract + author keywords + index terms),\n\
     AUTH, AFFIL\n\
   - Format: field code BEFORE the parentheses\n\
   - Example: TITLE-ABS-KEY(\"sugar tax\" OR \"soda tax\") AND\n\
     TITLE-ABS-KEY(obesity OR overweight)\n\n\
3. Web of Science\n\
   - Operators: AND, OR, NOT (uppercase)\n\
   - Field codes: TS (topic = title + abstract + author keywords +\n\
     keywords plus), TI (title), AB (abstract), AK (author keywords),\n\
     AU (author)\n\
   - Format: field code, equals sign, THEN parentheses\n\
   - Example: TS=(\"sugar tax\" OR \"soda tax\") AND TS=(obesity OR\n\
     overweight)\n\n\
4. Cochrane Library\n\
   - Operators: AND, OR, NOT\n\
   - Field labels (colon prefix, appended after term/group): :ti (title),\n\
     :ab (abstract), :kw (keyword + MeSH), :ti,ab,kw (recommended default)\n\
   - If no label given, searches all text\n\
   - Example: (\"sugar tax\" OR \"soda tax\"):ti,ab,kw AND\n\
     (obesity):ti,ab,kw\n\n\
5. EBSCOhost\n\
   - Operators: AND, OR, NOT\n\
   - Field codes (two-char, space-separated, NO colon): TI (title),\n\
     AB (abstract), SU (subject terms), AU (author), TX (full text)\n\
   - Default searches all authors, subjects, keywords, title info, and\n\
     abstracts\n\
   - Example: TI (\"sugar tax\" OR \"soda tax\") AND AB (obesity OR\n\
     overweight)\n\n\
6. JSTOR\n\
   - Operators: AND, OR, NOT, NEAR/5, NEAR/10, NEAR/25 (proximity)\n\
   - Field codes (colon prefix): ti: (title), au: (author),\n\
     ty: (type), cty: (content type)\n\
   - Supports parentheses grouping\n\
   - Example: ti:\"sugar tax\" OR ti:\"soda tax\" AND ab:obesity\n\n\
7. ScienceDirect\n\
   - Operators: AND, OR, NOT\n\
   - Field codes: TITLE, ABS, KEY, TITLE-ABS-KEY (recommended default),\n\
     AUTH, AFFIL\n\
   - Same syntax family as Scopus (both Elsevier)\n\
   - Example: TITLE-ABS-KEY(\"sugar tax\" OR \"soda tax\") AND\n\
     TITLE-ABS-KEY(obesity)\n\n\
8. arXiv\n\
   - Operators: AND, OR, ANDNOT (note: ANDNOT, NOT plain NOT)\n\
   - Field prefixes (colon): ti: (title), au: (author),\n\
     abs: (abstract), cat: (category), id: (paper ID)\n\
   - Example: (ti:\"large language model\" OR abs:\"LLM\") ANDNOT\n\
     cat:cs.CL\n\n\
NON-BOOLEAN DATABASE (advisory only - do NOT generate a query string):\n\
- Semantic Scholar: does NOT support Boolean operators or wildcards.\n\
  Add a warning telling the user to search plain key terms there.\n";
