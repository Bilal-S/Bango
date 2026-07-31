//! Citation Finder: paste prose, find matching citations from the article
//! library. See `citation_finder/AGENTS.md` + spec §8.7 for the authoritative
//! contracts.
//!
//! Three-layer pipeline:
//! 1. **Embedding prefilter** — `embedding::recall::recall` (reused, not
//!    reimplemented) embeds the query + max-pools cosine similarity across
//!    each candidate's chunk rows, returning the top-30 article IDs.
//! 2. **Token-Jaccard passage extraction** — `similarity::find_best_passage`
//!    picks the best-matching chunk per candidate (pure, no I/O).
//! 3. **LLM classification** — `prompt` builds a prompt; the LLM classifies
//!    each candidate as validating/opposing and writes a 1-2 sentence
//!    relevance explanation.
//!
//! Two modes:
//! - **Whole-block:** one embedding, one result set.
//! - **Per-statement:** the LLM splits the prose into ≤5 claims; each claim
//!   is embedded + matched independently; results grouped by claim.
//!
//! One-button flow: `find_citations` is the single entry point.
//! It runs Phase A (readiness) → Phase B (auto-prepare embeddings if coverage
//! <100%, reusing `generate_embeddings_inner`) → Phase C (the search pipeline
//! above), all under one cancel token.

pub mod claim_splitter;
pub mod prompt;
pub mod readiness;
pub mod search;
pub mod similarity;

use serde::{Deserialize, Serialize};

/// The canonical status strings the Citation Finder accepts. `duplicate` is
/// deliberately excluded (duplicates are never citation candidates). This is
/// the whitelist used by [`filter_valid_statuses`]; the backend does NOT apply
/// a default — if the caller supplies no valid statuses the search returns the
/// "No articles match the selected filters." empty result rather than
/// silently searching all articles.
pub const CITATION_STATUS_WHITELIST: &[&str] = &["working", "included", "rejected"];

/// Filter an arbitrary list of status strings down to the whitelist
/// `["working", "included", "rejected"]`. Drops anything else (typos, the
/// `duplicate` status, injection attempts, empty strings). Case-insensitive.
/// Order is preserved (first occurrence wins; later duplicates dropped).
///
/// Pure `#[must_use]` so the boundary behavior is unit-testable in isolation.
/// The Citation Finder applies this at the command boundary so the backend
/// never assumes a default — an empty result means the search returns no
/// matches (NOT "search all statuses").
#[must_use]
pub fn filter_valid_statuses(input: &[String]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::with_capacity(input.len());
    for s in input {
        let lower = s.to_ascii_lowercase();
        if CITATION_STATUS_WHITELIST.contains(&lower.as_str()) && seen.insert(lower.clone()) {
            out.push(lower);
        }
    }
    out
}

/// The user's input to `find_citations`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CitationFinderInput {
    pub text: String,
    pub mode: CitationFinderMode,
    /// Status strings to include in the candidate pool
    /// (e.g. `["working", "included"]`). Filtered through
    /// [`filter_valid_statuses`] at the command boundary; an empty result
    /// (no valid statuses) returns the "No articles match the selected
    /// filters." empty result — the backend never applies a default.
    pub status_filter: Vec<String>,
}

/// How the pasted text is processed.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationFinderMode {
    /// Embed the entire pasted text as one query.
    WholeBlock,
    /// LLM splits the text into ≤5 claims; each claim is embedded + matched
    /// independently; results grouped by claim.
    PerStatement,
}

/// One matched citation (one article's best passage + the LLM classification).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationMatch {
    pub article_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    /// The best-matching chunk text from the article.
    pub matched_passage: String,
    /// The section the matched passage came from (`chunk.section`).
    /// `None` for `SectionKind::Text`-derived chunks → the UI omits the `§…`
    /// badge.
    pub section_origin: Option<String>,
    pub classification: MatchClassification,
    pub relevance_explanation: String,
    /// `true` if the matched passage is taken out of context or selectively
    /// quoted in a way that **misrepresents** the source. `false` = the
    /// passage faithfully represents the surrounding text.
    pub misrepresents_source: bool,
    /// 1-3 EXACT verbatim sentences from `matched_passage` that justify the
    /// classification. Populated by `search::merge_outputs` via
    /// `prompt::ground_quotes`, which filters the LLM's `justifying_sentences`
    /// through a normalized-substring gate so only quotes that actually
    /// appear in the passage survive (paraphrases/hallucinations are
    /// dropped). Empty when the LLM omitted the field OR none of its
    /// sentences grounded — the UI falls back to the full `matched_passage`
    /// in that case (progressive disclosure: collapsed shows these
    /// snippets; expanded shows the full passage with them highlighted).
    pub highlighted_sentences: Vec<String>,
    /// The user-facing "match %" — the **cosine** (semantic) score from the
    /// recall layer, range 0.0–1.0. Jaccard is internal-only (drives passage
    /// selection); the card surfaces one number and it's the semantic one.
    pub confidence: f64,
}

/// The LLM's classification of a candidate passage relative to the claim.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchClassification {
    /// The passage supports the claim.
    Validating,
    /// The passage contradicts or challenges the claim.
    Opposing,
}

/// One search result group. In whole-block mode there's a single group with
/// `claim: None`; in per-statement mode there's one group per claim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationResult {
    /// `None` in whole-block mode; the claim text in per-statement mode.
    pub claim: Option<String>,
    pub matches: Vec<CitationMatch>,
}

/// Readiness payload returned by `get_citation_finder_readiness`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationFinderReadiness {
    pub total_articles: i64,
    pub embedded_count: i64,
    pub coverage_pct: f64,
    /// `embedding_status == Enabled AND dimensions > 0`. Drives toggle
    /// visibility (hidden when `false`, e.g. on Anthropic).
    pub provider_supports_embeddings: bool,
    pub statuses: Vec<String>,
}

/// Progress payload emitted via the `citation:progress` event and returned by
/// the `find_citations` command.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationFinderProgress {
    /// `"preparing_embeddings"` (Phase B) | `"searching"` (Phase C).
    pub phase: String,
    /// Phase-C sub-stage: `"embedding_query"` | `"ranking"` | `"classifying"`.
    /// `None` during Phase B.
    pub stage: Option<String>,
    pub done: usize,
    pub total: usize,
    /// 0–100 across BOTH phases (prepare weighted by article count).
    pub overall_percent: usize,
    pub message: String,
    pub is_running: bool,
    pub is_cancelled: bool,
}
// Unit tests live in `src-tauri/tests/citation_finder_mod_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
