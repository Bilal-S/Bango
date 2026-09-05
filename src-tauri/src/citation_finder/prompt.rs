//! LLM prompt builders for the Citation Finder.
//!
//! Pure `#[must_use]` functions, no I/O. Two prompt shapes:
//! - **Whole-block:** one claim (the whole pasted text), candidates with best
//!   passages, one classification per candidate.
//! - **Per-statement:** multiple claims, candidates with per-claim passages;
//!   LLM returns one classification per `(article, claim)` pair.

use std::collections::HashMap;

use serde::de::Error as _;
use serde::Deserialize;

use crate::citation_finder::MatchClassification;

/// The system prompt shared by both modes. `misrepresents_source`:
/// `true` = the passage is taken out of context / selectively quoted;
/// `false` = the passage faithfully represents the source.
pub const CITATION_FINDER_SYSTEM_PROMPT: &str = "\
You are a citation-matching assistant for academic literature review.
Your job is to classify citations as validating or opposing a given claim.

For each candidate:
- classification: \"validating\" if the passage supports the claim, \"opposing\" if it
  contradicts or challenges it, \"unrelated\" if irrelevant.
- relevance_explanation: a 1-2 sentence explanation of HOW the passage relates to the claim.
- misrepresents_source: true if the matched passage is taken out of context or selectively
  quoted in a way that MISREPRESENTS the source; false if it faithfully represents the
  surrounding text.

Return ONLY a JSON array. For each element, use these fields:
- article_id (string)
- claim (string, the claim text this classification applies to; empty in whole-block mode)
- classification (\"validating\" | \"opposing\")
- relevance_explanation (string, 1-2 sentences)
- misrepresents_source (boolean)
- justifying_sentences: a JSON array of 1-3 EXACT verbatim sentences from the passage that
  most directly justify the classification. Copy each sentence character-for-character from
  the passage; do NOT paraphrase, merge, truncate, or invent. Each sentence MUST appear in
  the passage verbatim (the UI highlights these as quotes). An empty array is acceptable when
  no single sentence is decisive.

Filter out \"unrelated\" candidates. Return at most 10 results. Order by relevance.";

/// Metadata for one candidate article, used by both prompt builders.
#[derive(Debug, Clone)]
pub struct CandidateMetadata {
    pub article_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
}

/// One (article, passage, section) tuple for the prompt. In whole-block mode
/// each candidate has one entry; in per-statement mode a candidate may have
/// multiple entries (one per claim it matched).
#[derive(Debug, Clone)]
pub struct CandidatePassage {
    pub article_id: String,
    pub claim: Option<String>,
    pub passage: String,
    pub section: Option<String>,
}

/// Build the user prompt for the whole-block mode.
///
/// `user_text` is the pasted prose. `passages` carries one entry per candidate
/// (each with its best-matching chunk + section). `metadata` carries the
/// title/authors/year/journal/DOI per article_id so the LLM can write
/// informed relevance explanations (`citation_finder/AGENTS.md`: "Each candidate block
/// includes article_id, metadata, matched passage, and section").
#[must_use]
pub fn build_whole_block_prompt(
    user_text: &str,
    passages: &[CandidatePassage],
    metadata: &HashMap<String, CandidateMetadata>,
) -> String {
    let candidates_section = format_candidates_section(passages, metadata);
    format!(
        "The user is writing the following text:\n\n<user_text>\n{user_text}\n</user_text>\n\n\
         Here are potential supporting citations. For each, classify whether it validates or \
         opposes the user's claim.\n\n{candidates_section}"
    )
}

/// Build the user prompt for the per-statement mode.
///
/// `claims` is the list of claims the splitter produced. `passages` carries
/// per-(article, claim) entries; the LLM returns one classification per pair.
/// `metadata` carries the title/authors/year/journal/DOI per article_id.
#[must_use]
pub fn build_per_statement_prompt(
    claims: &[String],
    passages: &[CandidatePassage],
    metadata: &HashMap<String, CandidateMetadata>,
) -> String {
    let claims_list = claims
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {c}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let candidates_section = format_candidates_section(passages, metadata);
    format!(
        "The user is writing text that contains the following distinct claims:\n\n{claims_list}\n\n\
         Here are potential supporting citations, each matched to one of the claims above. \
         For each (article, claim) pair, classify whether the article validates or opposes \
         that specific claim.\n\n{candidates_section}"
    )
}

/// Format the candidates section shared by both prompts. Each passage block
/// includes the article_id (so the LLM's JSON output can reference it), the
/// article metadata (title, authors, year, journal, DOI - when available so
/// the LLM can write informed explanations), the claim (when present), the
/// matched passage, and the section origin.
fn format_candidates_section(
    passages: &[CandidatePassage],
    metadata: &HashMap<String, CandidateMetadata>,
) -> String {
    let mut out = String::new();
    out.push_str("## Candidates\n\n");
    for (i, p) in passages.iter().enumerate() {
        out.push_str(&format!("### Candidate {}\n", i + 1));
        out.push_str(&format!("- article_id: {}\n", p.article_id));
        // Render metadata when available so the LLM can write informed
        // relevance explanations (not just classify passages blind).
        if let Some(m) = metadata.get(&p.article_id) {
            out.push_str(&format!("- title: {}\n", m.title));
            if !m.authors.is_empty() {
                out.push_str(&format!("- authors: {}\n", m.authors.join("; ")));
            }
            if let Some(year) = m.publication_year {
                out.push_str(&format!("- year: {year}\n"));
            }
            if let Some(journal) = &m.journal {
                out.push_str(&format!("- journal: {journal}\n"));
            }
            if let Some(doi) = &m.doi {
                out.push_str(&format!("- doi: {doi}\n"));
            }
        }
        if let Some(claim) = &p.claim {
            out.push_str(&format!("- claim: {claim}\n"));
        }
        if let Some(section) = &p.section {
            out.push_str(&format!("- section: {section}\n"));
        }
        out.push_str(&format!("- passage:\n\n{}\n\n", p.passage));
    }
    out
}

/// One LLM output element (deserialized from the JSON array).
///
/// **Lenient field-name contract**: the prompt requests snake_case, but LLMs
/// are unreliable about casing. Every field accepts BOTH shapes via
/// `#[serde(alias = "...")]`. This struct is `Deserialize`-only (never
/// serialized to the frontend — the IPC types `CitationMatch`/`CitationResult`
/// serialize independently with their own `camelCase`).
///
/// `classification` + `relevance_explanation` carry `#[serde(default)]`
/// (→ empty string). `parse_classification("")` returns `None` and drops the
/// entry, so a missing classification is naturally filtered. `article_id`
/// stays required, but `parse_citation_outputs` isolates per-element faults
/// so one bad element costs only that element.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CitationLlmOutput {
    #[serde(alias = "articleId")]
    pub article_id: String,
    /// Empty in whole-block mode; claim text in per-statement mode. The
    /// lookup in `search::merge_outputs` normalizes before comparing,
    /// so light LLM reformatting doesn't cause a score-lookup miss.
    #[serde(default, alias = "claimText")]
    pub claim: String,
    #[serde(default)]
    pub classification: String,
    #[serde(default, alias = "relevanceExplanation", alias = "explanation")]
    pub relevance_explanation: String,
    /// `true` = passage misrepresents the source. `fairlyParaphrased` alias
    /// preserves backward-compat with a stale prompt template cached mid-rollout.
    #[serde(default, alias = "fairlyParaphrased", alias = "misrepresentsSource")]
    pub misrepresents_source: bool,
    /// 1-3 EXACT verbatim sentences from the passage. The prompt instructs the
    /// LLM to copy them character-for-character; `ground_quotes` filters out
    /// paraphrases via a normalized-substring gate. Survivors populate
    /// `CitationMatch.highlighted_sentences` for progressive-disclosure UI.
    #[serde(default, alias = "justifyingSentences")]
    pub justifying_sentences: Vec<String>,
}

/// Parse the LLM's classification string into the typed enum. Returns `None`
/// for anything other than "validating"/"opposing" (covers "unrelated" which
/// the prompt tells the LLM to filter out, plus any stray value).
#[must_use]
pub fn parse_classification(s: &str) -> Option<MatchClassification> {
    match s.to_ascii_lowercase().trim() {
        "validating" => Some(MatchClassification::Validating),
        "opposing" => Some(MatchClassification::Opposing),
        _ => None,
    }
}

/// Grounding gate: filters the LLM's `justifying_sentences` to only those
/// that actually appear as substrings of `source` (the matched passage).
///
/// LLMs frequently paraphrase despite being told to copy verbatim. Displaying
/// an ungrounded sentence as a "quote" would fabricate text the paper does
/// not contain, so each quote MUST pass a normalized-substring check
/// (lowercase + collapse whitespace + trim).
///
/// Returns grounded quotes in their original form, ordered by first
/// appearance in `source`. Deduplicates exact dupes. Pure `#[must_use]`.
#[must_use]
pub fn ground_quotes(quotes: &[String], source: &str) -> Vec<String> {
    let norm_source = normalize_for_grounding(source);
    // Track the source-offset of each accepted quote so we can sort the
    // survivors into source order (the LLM may emit them out of order).
    let mut accepted: Vec<(usize, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for q in quotes {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Dedup on the normalized form so "Sentence." and "sentence." don't
        // both survive.
        let norm_q = normalize_for_grounding(trimmed);
        if norm_q.is_empty() || seen.contains(&norm_q) {
            continue;
        }
        if let Some(offset) = norm_source.find(&norm_q) {
            seen.insert(norm_q);
            accepted.push((offset, trimmed.to_string()));
        }
        // else: not a substring → drop (hallucinated / paraphrased).
    }
    accepted.sort_by_key(|(offset, _)| *offset);
    accepted.into_iter().map(|(_, s)| s).collect()
}

/// Normalize text for the grounding substring check: lowercase + collapse
/// internal whitespace runs to a single space + trim. Pure.
fn normalize_for_grounding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_ws = false;
        }
    }
    out
}

/// Object-wrapper keys an LLM may wrap the JSON array in despite being told
/// to return ONLY a JSON array. If `serde_json::from_str::<Vec<_>>` is used
/// directly, any of these wrappers fails the whole parse; `parse_citation_outputs`
/// unwraps them first.
const WRAPPER_KEYS: &[&str] = &["results", "citations", "data", "matches", "items", "output"];

/// Lenient parser for the Citation Finder LLM's JSON response.
///
/// Three layers of resilience:
/// 1. **Object-wrapper tolerance**: accepts a bare array OR `{…}` with one of
///    the known wrapper keys (`results`, `citations`, `data`, `matches`,
///    `items`, `output`).
/// 2. **Per-element fault isolation**: each element is deserialized
///    independently; one bad element costs only that element (previously a
///    single bad entry threw away every good result — the exact failure mode
///    of the `missing field articleId` bug).
/// 3. **Field-name aliases**: handled at the struct level — both snake_case
///    and camelCase parse.
///
/// # Errors
///
/// Returns the original `serde_json::Error` if ZERO elements parse (genuine
/// LLM failure — not masked as an empty result). A valid `[]` returns
/// `Ok(vec![])`. Pure (no I/O).
pub fn parse_citation_outputs(raw: &str) -> Result<Vec<CitationLlmOutput>, serde_json::Error> {
    // Parse once to a `Value`, then either descend into a wrapper key or use
    // the value directly. This avoids re-parsing per code path.
    let value: serde_json::Value = serde_json::from_str(raw)?;

    /* Resolve to the array `Value` (unwrap a known wrapper object if present).
    `serde::de::Error` is imported `as _` above so `serde_json::Error::custom`
    resolves via trait method dispatch. */
    let array_value = resolve_array(&value).ok_or_else(|| {
        serde_json::Error::custom(
            "expected a JSON array or an object with one of the keys: \
             results, citations, data, matches, items, output",
        )
    })?;

    let array = array_value
        .as_array()
        .ok_or_else(|| serde_json::Error::custom("resolved value is not a JSON array"))?;

    if array.is_empty() {
        return Ok(Vec::new());
    }

    /* Per-element fault isolation: deserialize each element independently,
    keep successes, skip failures. Track the first error so that if EVERY
    element fails we can surface it. */
    let mut outputs: Vec<CitationLlmOutput> = Vec::with_capacity(array.len());
    let mut first_error: Option<serde_json::Error> = None;
    for element in array {
        match CitationLlmOutput::deserialize(element) {
            Ok(out) => outputs.push(out),
            Err(e) if first_error.is_none() => first_error = Some(e),
            Err(_) => {}
        }
    }

    if outputs.is_empty() {
        /* Every element failed — surface the first error so the caller can
        report what went wrong. `first_error` is guaranteed `Some` here
        because `array` is non-empty (guarded above) and every element failed.
        `unwrap_or_else` (not `expect`) avoids the `expect_used` lint and
        keeps the impossible `None` arm total. */
        Err(first_error.unwrap_or_else(|| {
            serde_json::Error::custom("failed to parse any elements of the JSON array")
        }))
    } else {
        Ok(outputs)
    }
}

/// Resolve a parsed JSON `Value` to the underlying array, descending into a
/// known wrapper-key object if the top-level value is an object rather than
/// an array. Returns `None` if the value is neither an array nor a wrapper
/// object.
fn resolve_array(value: &serde_json::Value) -> Option<&serde_json::Value> {
    if value.is_array() {
        Some(value)
    } else if let Some(obj) = value.as_object() {
        // Find the first present wrapper key whose value is an array. A
        // wrapper object with a non-array value under a known key is treated
        // as not-a-match so the caller's "unknown wrapper" error fires.
        for key in WRAPPER_KEYS {
            if let Some(inner) = obj.get(*key) {
                if inner.is_array() {
                    return Some(inner);
                }
            }
        }
        None
    } else {
        None
    }
}

// Unit tests live in `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
