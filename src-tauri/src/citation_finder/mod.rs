//! Citation Finder: paste prose → matching citations from the article library.
//! See `citation_finder/AGENTS.md` + spec §8.7 for authoritative contracts.
//!
//! Three-layer pipeline:
//! 1. **Embedding prefilter** — `embedding::recall::recall` embeds the query,
//!    max-pools cosine across chunk rows, returns top-30 article IDs.
//! 2. **Token-containment passage extraction** — `similarity::find_best_passage`
//!    picks the best chunk per candidate (pure, no I/O).
//! 3. **LLM classification** — `prompt` builds a prompt; LLM classifies each
//!    candidate as validating/opposing.
//!
//! Two modes: **Whole-block** (one embedding, one result set) and
//! **Per-statement** (LLM splits prose into ≤5 claims; each embedded +
//! matched independently; results grouped by claim).
//!
//! One-button flow: `find_citations` runs Phase A (readiness) → Phase B
//! (auto-prepare embeddings if coverage < 100%, reusing
//! `generate_embeddings_inner`) → Phase C (the search pipeline), all under
//! one cancel token.

pub mod claim_splitter;
pub mod prompt;
pub mod readiness;
pub mod search;
pub mod similarity;

use serde::{Deserialize, Serialize};

/// Canonical status strings accepted by the Citation Finder.
/// `duplicate` is deliberately excluded. Used by [`filter_valid_statuses`];
/// the backend does NOT apply a default — an empty filter returns the
/// "No articles match the selected filters." result (never "all statuses").
pub const CITATION_STATUS_WHITELIST: &[&str] = &["working", "included", "rejected"];

/// Filter status strings to the whitelist; drops typos, `duplicate`,
/// injection attempts, empties. Case-insensitive, order preserved.
/// Pure `#[must_use]`.
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

/// The user's input to [`find_citations`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CitationFinderInput {
    pub text: String,
    pub mode: CitationFinderMode,
    /// Status strings for the candidate pool (e.g. `["working", "included"]`).
    /// Filtered through [`filter_valid_statuses`] at the command boundary;
    /// an empty result after filtering → "No articles match the selected
    /// filters." empty result — the backend never applies a default.
    pub status_filter: Vec<String>,
}

/// LLM processing mode for the pasted text.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationFinderMode {
    /// Embed the entire pasted text as one query.
    WholeBlock,
    /// LLM splits text into ≤5 claims; each embedded + matched independently.
    PerStatement,
}

/// One matched citation: article metadata + best passage + LLM classification.
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
    /// `chunk.section` of the matched passage. `None` → UI omits the `§…` badge.
    pub section_origin: Option<String>,
    pub classification: MatchClassification,
    pub relevance_explanation: String,
    /// `true` when the matched passage is taken out of context in a way that
    /// **misrepresents** the source.
    pub misrepresents_source: bool,
    /// 1-3 EXACT verbatim sentences from `matched_passage` justifying the
    /// classification. Populated by `search::merge_outputs` via
    /// `prompt::ground_quotes` (normalized-substring gate: paraphrases /
    /// hallucinations are dropped). Empty when the LLM omitted the field OR
    /// none grounded — the UI falls back to the full `matched_passage`.
    pub highlighted_sentences: Vec<String>,
    /// User-facing "match %" — cosine (semantic) score from the recall layer,
    /// range 0.0–1.0. Jaccard is internal-only (drives passage selection).
    pub confidence: f64,
}

/// The LLM's classification of a passage relative to the claim.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchClassification {
    Validating,
    Opposing,
}

/// One search result group. Whole-block: single group with `claim: None`.
/// Per-statement: one group per claim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationResult {
    pub claim: Option<String>,
    pub matches: Vec<CitationMatch>,
}

/// Readiness payload from `get_citation_finder_readiness`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationFinderReadiness {
    pub total_articles: i64,
    pub embedded_count: i64,
    pub coverage_pct: f64,
    /// `embedding_status == Enabled AND dimensions > 0`. Drives toggle
    /// visibility (hidden when `false`). The frontend also reads
    /// `embedding_status` for a disabled-but-visible state on known-
    /// unsupported providers (Anthropic, Z.AI).
    pub provider_supports_embeddings: bool,
    pub statuses: Vec<String>,
    /// Raw triple-state: `"unknown"` | `"enabled"` | `"disabled"`.
    /// Permits a disabled-but-visible toggle with a precise tooltip on
    /// known-unsupported providers.
    pub embedding_status: String,
    /// Last-working embedding model name (e.g. `"text-embedding-3-small"`).
    /// `None` when probe has not run or provider is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

/// Progress emitted via `citation:progress` event and returned by
/// `find_citations`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationFinderProgress {
    /// `"preparing_embeddings"` (Phase B) | `"searching"` (Phase C).
    pub phase: String,
    /// Phase-C sub-stage: `"embedding_query"` | `"ranking"` | `"classifying"`.
    pub stage: Option<String>,
    pub done: usize,
    pub total: usize,
    /// 0–100 across both phases (prepare weighted by article count).
    pub overall_percent: usize,
    pub message: String,
    pub is_running: bool,
    pub is_cancelled: bool,
}
// Unit tests live in `src-tauri/tests/citation_finder_mod_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
