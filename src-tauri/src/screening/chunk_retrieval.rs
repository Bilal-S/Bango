//! Per-article criteria-targeted chunk ranking via in-memory TF scoring.
//! Pure (no I/O/DB). Uses `utils::text_tokens::tokenize_for_match` for token
//! consistency with FTS5 BM25. Faster than FTS5 per-article (microseconds vs
//! index overhead) and scoped per-article.

use crate::utils::chunking::Chunk;
use crate::utils::text_tokens::tokenize_for_match;
use std::collections::HashMap;

/// Default number of chunks to return per article (Chunkr-style `top_k`).
pub const DEFAULT_TOP_K: usize = 2;

/// Hard cap: skip chunks larger than this (words) when ranking.
pub const DEFAULT_MAX_CHUNK_WORDS: usize = 600;

/// Methods-section score boost. Methods = highest-signal section for screening.
pub const METHODS_BOOST: f64 = 0.25;

/// Per-article chunk budget (words). Guarantees no single article blows the
/// screening context window.
pub const DEFAULT_CHUNK_BUDGET_PER_ARTICLE: usize = 2_400;

/// A chunk ranked against criteria, carrying its TF score.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredChunk {
    pub chunk_index: usize,
    /// Section label (e.g. `Some("Methods")`) for `[§Methods]` citation prefix.
    pub section: Option<String>,
    pub content: String,
    /// TF score: sum of criteria-token frequencies / chunk word count + methods boost.
    pub score: f64,
}

/// Rank article chunks against criteria text. TF density scoring + methods boost;
/// filters oversized chunks, enforces word budget. Empty criteria → all tie at 0.0.
#[must_use]
pub fn rank_chunks_by_criteria(
    chunks: &[Chunk],
    inclusion_criteria: &[String],
    exclusion_criteria: &[String],
    top_k: usize,
    max_chunk_words: usize,
    chunk_budget_per_article: usize,
) -> Vec<ScoredChunk> {
    if chunks.is_empty() || top_k == 0 {
        return Vec::new();
    }

    // Build the query token-frequency map from all criteria text. Use a map
    // (not a set) so repeated criteria terms count proportionally.
    let mut query_tokens: HashMap<String, usize> = HashMap::new();
    for criterion in inclusion_criteria.iter().chain(exclusion_criteria.iter()) {
        for token in tokenize_for_match(criterion) {
            *query_tokens.entry(token).or_insert(0) += 1;
        }
    }

    // Score each chunk that fits the size cap.
    let mut scored: Vec<ScoredChunk> = chunks
        .iter()
        .filter(|c| c.word_count > 0 && c.word_count <= max_chunk_words)
        .map(|c| ScoredChunk {
            chunk_index: c.chunk_index,
            section: c.section.clone(),
            content: c.text.clone(),
            score: score_chunk(c, &query_tokens),
        })
        .collect();

    // Sort: highest score first. Ties keep stable (original) order via
    // `sort_by` being stable, so equal-score chunks preserve chunk_index order.
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Take the top_k.
    let k = top_k.min(scored.len());
    let mut result: Vec<ScoredChunk> = scored.into_iter().take(k).collect();

    // Budget guard: drop lowest-ranked chunk until the summed word count fits.
    // Iterating from the end (lowest score) and popping is the documented
    // behavior (T3.7 `budget_guard_drops_lowest_chunk_when_over_budget`).
    enforce_word_budget(&mut result, chunk_budget_per_article);

    result
}

/// Score one chunk: sum of criteria-token frequencies in the chunk, divided by
/// the chunk word count (so longer chunks don't dominate purely by size), plus
/// the Methods-section boost.
fn score_chunk(chunk: &Chunk, query_tokens: &HashMap<String, usize>) -> f64 {
    let mut hits = 0usize;
    for token in crate::utils::text_tokens::tokenize(&chunk.text) {
        if let Some(&weight) = query_tokens.get(&token) {
            hits += weight;
        }
    }
    let density = hits as f64 / chunk.word_count.max(1) as f64;
    /* Methods boost only when chunk has criteria-token hits; otherwise an unmatched
    Methods would outrank an unmatched Results despite nothing matching. */
    let boost =
        if hits > 0 && chunk.section.as_deref() == Some("Methods") { METHODS_BOOST } else { 0.0 };
    density + boost
}

/// Drop the lowest-ranked chunks from the end until the summed word count is
/// within the budget. Never drops below 1 chunk (better to exceed the budget
/// slightly than send zero evidence).
fn enforce_word_budget(chunks: &mut Vec<ScoredChunk>, budget: usize) {
    while chunks.len() > 1 {
        let total: usize = chunks.iter().map(|c| c.content.split_whitespace().count()).sum();
        if total <= budget {
            return;
        }
        /* Pop lowest-ranked from end (stable-descending sort) until budget fits.
        Never drops below 1 chunk. */
        chunks.pop();
    }
}

/// Format scored chunks into `## Supporting Evidence from Full Text` block.
/// Each chunk prefixed `[§Section]`. `None` when empty. Canonical impl; both
/// `engine` and `evidence` delegate here for byte-identical output.
#[must_use]
pub fn format_chunks_as_evidence(chunks: &[ScoredChunk]) -> Option<String> {
    if chunks.is_empty() {
        return None;
    }
    let lines: Vec<String> = chunks
        .iter()
        .map(|c| {
            let label = c.section.as_deref().unwrap_or("Text");
            format!("[§{label}] {}", c.content)
        })
        .collect();
    Some(lines.join("\n"))
}
