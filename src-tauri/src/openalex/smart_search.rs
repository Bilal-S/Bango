//! LLM-generated OpenAlex Boolean query from research aims + criteria.

use serde::{Deserialize, Serialize};

use crate::db::criteria_repo;
use crate::error::AppError;
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::utils::json_repair::prepare_llm_json;

/// Hard ceiling for the OpenAlex `search=` parameter. Queries longer than this
/// are not supported without an API key, so we cap universally regardless of
/// whether a key is configured.
pub const MAX_SEARCH_QUERY_LEN: usize = 1500;

/// The LLM's parsed response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchQuery {
    pub search_query: String,
    pub suggested_filters: SmartSearchFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchFilters {
    pub publication_year: Option<String>,
    #[serde(default)]
    pub r#type: Vec<String>,
}

/// Build (system, user) prompt pair from aims + criteria.
#[must_use]
pub fn build_smart_search_prompt(
    aims: &[ResearchAim],
    inclusion: &[Criterion],
    exclusion: &[Criterion],
) -> (String, String) {
    let system = SYSTEM_PROMPT.to_string();

    let aims_list: Vec<String> =
        aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a.text)).collect();

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
You are building an OpenAlex search query for a systematic literature review.
Identify the PICO concepts from the research aims (and criteria, if present),
then compose a single concise OpenAlex Boolean search string that leverages
OpenAlex's native stemming and synonym matching.

## Research Aims
{aims}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Instructions
- Output budget: the searchQuery MUST be 1500 characters or fewer.
- Use AND, OR, NOT (uppercase) as Boolean operators.
- Use double quotes for exact multi-word phrases (e.g., "sugar-sweetened beverage").
- Do NOT enumerate redundant synonyms, stems, or plurals - OpenAlex matches them
  automatically. List only semantically distinct concept variants.
- Group shared keywords into nested ( ... ) statements; map each PICO concept to
  an OR-group and join the groups with AND.
- Map exclusion criteria to NOT-groups, but OMIT any NOT term already filtered
  out by the positive constraints or the suggested filters.
- Wildcards: only inside a quoted multi-word phrase ("smart* phone" or
  "smart phone*"~3); never on a standalone word.
- Do NOT include the filter= syntax - only the search= query.
- Do not use em dashes anywhere in the response.

## Response Format
Return ONLY a JSON object matching this schema (no prose, no markdown fences):
{{
  "searchQuery": "(\"sugar-sweetened beverage\" OR \"soft drink\") AND (tax OR levy OR policy) AND (health OR consumption OR obesity)",
  "suggestedFilters": {{
    "publicationYear": "2010-2025",
    "type": ["article", "review"]
  }}
}}"#,
        aims = aims_list.join("\n"),
        inclusion = inclusion_block,
        exclusion = exclusion_block,
    );

    (system, user)
}

/// Cap a generated search query to `max_len` bytes without splitting a quoted
/// phrase, leaving parentheses unbalanced, or cutting mid-word. The returned
/// string is always a valid UTF-8 slice whose open/close parentheses and
/// double-quotes are balanced and which carries no trailing Boolean operator.
///
/// Strategy: a single left-to-right scan tracks quote state + paren depth over
/// `query[..limit]`. A byte position is a "natural break" candidate when, at
/// that position, every construct opened earlier has also closed (paren depth
/// 0, outside a quote) AND the byte is either a whitespace boundary or
/// immediately follows a closing `)`. The cut lands at the latest natural
/// break; if none exists within budget (e.g. the entire budget sits inside one
/// unclosed group/phrase), the cut backs up to just before that construct's
/// opening token so no unbalanced syntax is emitted. A trailing Boolean
/// operator (AND/OR/NOT) is then stripped so the result is executable.
#[must_use]
pub fn truncate_search_query(query: &str, max_len: usize) -> String {
    if query.len() <= max_len {
        return trim_trailing_operator(query).to_string();
    }
    if max_len == 0 {
        return String::new();
    }

    let bytes = query.as_bytes();
    let limit = max_len.min(bytes.len());

    let mut in_quote = false;
    let mut depth: i32 = 0;
    // Byte index of the opening `(` of the first still-open top-level group,
    // used only if the scan never returns to depth 0 within budget.
    let mut first_open_paren: Option<usize> = None;
    // Latest natural break: a position safe to cut at that lands on a word or
    // group boundary. Defaults to 0 (empty prefix is trivially valid).
    let mut best_cut: usize = 0;

    for (i, &byte) in bytes.iter().enumerate().take(limit) {
        let mut just_closed_group = false;
        match byte {
            b'"' => in_quote = !in_quote,
            b'(' if !in_quote => {
                if depth == 0 && first_open_paren.is_none() {
                    first_open_paren = Some(i);
                }
                depth += 1;
            }
            b')' if !in_quote => {
                if depth > 0 {
                    depth -= 1;
                }
                just_closed_group = true;
            }
            _ => {}
        }
        // A natural break sits at depth 0, outside a quote, and either right
        // after a complete group `)` or on a whitespace boundary (so the cut
        // never lands mid-word). Record the position just past the break byte.
        if depth == 0 && !in_quote {
            if just_closed_group {
                best_cut = i + 1;
            } else if byte.is_ascii_whitespace() {
                // Cut at the whitespace so the trailing space is dropped by
                // trim_trailing_operator; the kept prefix ends on a full word.
                best_cut = i;
            }
        }
    }

    // If the scan ended at a balanced position (depth 0, outside a quote) and
    // the byte just past the budget is a whitespace boundary, the full budget
    // window is itself a natural cut - capture the last complete word that
    // fits instead of dropping it.
    if depth == 0 && !in_quote && limit < bytes.len() && bytes[limit].is_ascii_whitespace() {
        best_cut = limit;
    }

    // Prefer the latest natural break. If the whole budget is inside an
    // unclosed group/phrase (best_cut stayed 0), back up to just before the
    // opening `(` so the prefix stays balanced; last resort is a hard cut.
    let cut = if best_cut > 0 { best_cut } else { first_open_paren.unwrap_or(limit) };

    trim_trailing_operator(&query[..cut]).to_string()
}

/// Strip a trailing uppercase Boolean operator (AND/OR/NOT) plus surrounding
/// whitespace. Only strips when the operator is preceded by `)` or whitespace
/// so legitimate words like "COMMAND" or "NOR" are never mangled.
fn trim_trailing_operator(s: &str) -> &str {
    let s = s.trim_end();
    for op in ["AND", "OR", "NOT"] {
        if let Some(prefix) = s.strip_suffix(op) {
            let before = prefix.trim_end();
            if before.ends_with(')') || prefix.ends_with(' ') {
                return before;
            }
        }
    }
    s
}

/// Parse + validate the LLM JSON response. The `searchQuery` is always capped
/// to [`MAX_SEARCH_QUERY_LEN`] so an over-long LLM output can never produce a
/// query OpenAlex rejects without an API key.
pub fn parse_smart_search_response(raw: &str) -> Result<SmartSearchQuery, AppError> {
    let prepared = prepare_llm_json(raw);
    let mut parsed: SmartSearchQuery = serde_json::from_str(&prepared)
        .map_err(|e| AppError::Import(format!("Failed to parse smart search response: {e}")))?;
    parsed.search_query = truncate_search_query(&parsed.search_query, MAX_SEARCH_QUERY_LEN);
    Ok(parsed)
}

/// Read aims + criteria from DB, grouped by type.
#[allow(clippy::type_complexity)]
pub fn read_aims_and_criteria(
    conn: &rusqlite::Connection,
) -> Result<(Vec<ResearchAim>, Vec<Criterion>, Vec<Criterion>), AppError> {
    let aims = criteria_repo::get_all_aims(conn)?;
    if aims.is_empty() {
        return Err(AppError::Validation(
            "Research aims must be defined before generating a smart search".to_string(),
        ));
    }
    let all_criteria = criteria_repo::get_all_criteria(conn)?;
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
    Ok((aims, inclusion, exclusion))
}

const SYSTEM_PROMPT: &str = "\
You are a systematic-review search strategist specializing in OpenAlex queries.\n\
You build OpenAlex Boolean search strings from a research question's PICO concepts.\n\
You return ONLY a JSON object, no prose, no markdown fences, no em dashes.\n\n\
HARD LIMIT: the searchQuery string MUST be 1500 characters or fewer. Queries\n\
longer than 1500 characters are rejected by OpenAlex without an API key.\n\n\
OPENALEX SEARCH BEHAVIOR (use it to stay concise):\n\
- OpenAlex automatically matches word stems and synonyms, so do NOT enumerate\n\
  redundant stems, plurals, or spelling variants (drop \"tax\"/\"taxes\" as two\n\
  terms; drop \"sugar-sweetened\" and \"sugar sweetened\" as separate variants).\n\
- List only semantically DISTINCT concept variants.\n\
- Group shared keywords into nested ( ... ) statements instead of flat expansion.\n\n\
OPENALEX SYNTAX RULES:\n\
- Operators: AND, OR, NOT (MUST be uppercase).\n\
- Default operator between adjacent words is AND.\n\
- Exact phrases: use double quotes (e.g., \"sugar tax\").\n\
- Wildcards: OpenAlex stemming makes single-word wildcards redundant - do NOT\n\
  use them. Only use * inside a quoted multi-word phrase for adjacency\n\
  (\"smart* phone\") or proximity (\"smart phone*\"~3).\n\
- The search covers title + abstract + fulltext.\n\
- Do NOT include filter= syntax - emit only the search query string.\n";
