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

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(index: usize, section: Option<&str>, text: &str) -> Chunk {
        Chunk {
            chunk_index: index,
            section: section.map(str::to_string),
            text: text.to_string(),
            word_count: text.split_whitespace().count(),
        }
    }

    fn criteria(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|s| s.to_string()).collect()
    }

    // ── §T3.7 inventory tests (binding) ───────────────────────────────

    #[test]
    fn rank_chunks_empty_chunks_returns_empty() {
        let inc = criteria(&["children obesity"]);
        let out = rank_chunks_by_criteria(
            &[],
            &inc,
            &[],
            DEFAULT_TOP_K,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn rank_chunks_empty_criteria_returns_all_unscored() {
        // No criteria => every chunk ties at score 0.0. Returns up to top_k in
        // original order.
        let chunks = vec![
            chunk(0, Some("Methods"), "alpha beta gamma"),
            chunk(1, Some("Results"), "delta epsilon"),
        ];
        let out = rank_chunks_by_criteria(
            &chunks,
            &[],
            &[],
            5,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out.len(), 2, "all chunks returned when criteria empty");
        assert!(out.iter().all(|c| c.score == 0.0), "all tie at 0.0 with no criteria");
        assert_eq!(out[0].chunk_index, 0, "original order preserved");
        assert_eq!(out[1].chunk_index, 1);
    }

    #[test]
    fn rank_chunks_methods_section_gets_boost() {
        // Two chunks with identical criteria overlap; the Methods one ranks first.
        let chunks = vec![
            chunk(0, Some("Text"), "sugar tax study design rct"),
            chunk(1, Some("Methods"), "sugar tax study design rct"),
        ];
        let inc = criteria(&["sugar tax rct"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out[0].section.as_deref(), Some("Methods"), "Methods chunk boosted to top");
        assert_eq!(out[1].section.as_deref(), Some("Text"));
        assert!(
            (out[0].score - out[1].score - METHODS_BOOST).abs() < 1e-9,
            "boost delta == METHODS_BOOST"
        );
    }

    #[test]
    fn rank_chunks_respects_top_k() {
        let chunks: Vec<Chunk> =
            (0..5).map(|i| chunk(i, Some("Methods"), &format!("sugar tax {i}"))).collect();
        let inc = criteria(&["sugar tax"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out.len(), 2, "top_k=2 with 5 chunks -> 2 returned");
    }

    #[test]
    fn rank_chunks_filters_oversized_chunks() {
        // chunk 0 is oversized (> MAX), chunk 1 is normal and matches.
        let big = format!("sugar {}", "word ".repeat(DEFAULT_MAX_CHUNK_WORDS + 10));
        let chunks =
            vec![chunk(0, Some("Methods"), &big), chunk(1, Some("Methods"), "sugar tax levy")];
        let inc = criteria(&["sugar tax"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out.len(), 1, "oversized chunk excluded");
        assert_eq!(out[0].chunk_index, 1, "only the normal chunk remains");
    }

    #[test]
    fn rank_chunks_criteria_token_overlap_drives_ranking() {
        // chunk 0 has 3 criteria tokens; chunk 1 has 1. chunk 0 ranks first.
        let chunks = vec![
            chunk(0, Some("Text"), "sugar tax children"), // 3 matches
            chunk(1, Some("Text"), "sugar other prose words"), // 1 match
        ];
        let inc = criteria(&["sugar tax children obesity"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out[0].chunk_index, 0, "chunk with more overlap ranks first");
        assert!(out[0].score > out[1].score);
    }

    #[test]
    fn rank_chunks_handles_stop_words_in_criteria() {
        // "the RCT and children" -> tokens {rct, children} only.
        let chunks = vec![
            chunk(0, Some("Methods"), "rct children participants"),
            chunk(1, Some("Text"), "the and is prose words"),
        ];
        let inc = criteria(&["the RCT and children"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out[0].chunk_index, 0, "chunk matching {{rct, children}} ranks first");
        assert!(out[0].score > 0.0);
    }

    #[test]
    fn budget_guard_drops_lowest_chunk_when_over_budget() {
        // 3 chunks each ~300 words, top_k=3 -> sum ~900. Budget=700 forces one
        // drop (2 chunks = ~600 <= 700, so exactly one drop is enough).
        let words = "word ".repeat(300);
        let chunks = vec![
            chunk(0, Some("Methods"), &format!("sugar tax {words}")),
            chunk(1, Some("Results"), &format!("sugar tax {words}")),
            chunk(2, Some("Text"), &format!("sugar tax {words}")),
        ];
        let inc = criteria(&["sugar tax"]);
        // top_k=3, budget=700 => 3x~302=906 > 700 -> drop lowest -> 2x~302=604 <= 700.
        let out = rank_chunks_by_criteria(&chunks, &inc, &[], 3, DEFAULT_MAX_CHUNK_WORDS, 700);
        assert_eq!(out.len(), 2, "budget guard drops 1 chunk: got {}", out.len());
        // The Methods chunk (boosted, highest score) survives.
        assert!(
            out.iter().any(|c| c.section.as_deref() == Some("Methods")),
            "highest-ranked Methods chunk survives the drop"
        );
    }

    // ── Extra robustness tests ─────────────────────────────────────────

    #[test]
    fn rank_chunks_top_k_zero_returns_empty() {
        let chunks = vec![chunk(0, Some("Methods"), "sugar tax")];
        let inc = criteria(&["sugar"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &[],
            0,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn rank_chunks_budget_never_drops_below_one() {
        // A single chunk that exceeds the budget but is within the size cap:
        // keep it (1 chunk > 0 evidence). Must pass a `max_chunk_words` high
        // enough that the chunk is NOT filtered out before the budget guard.
        let big = format!("sugar {}", "word ".repeat(800));
        let chunks = vec![chunk(0, Some("Methods"), &big)];
        let inc = criteria(&["sugar"]);
        let out = rank_chunks_by_criteria(&chunks, &inc, &[], 2, 1000, 100);
        assert_eq!(out.len(), 1, "never drop below 1 chunk even if over budget");
    }

    #[test]
    fn rank_chunks_exclusion_criteria_also_contribute_tokens() {
        // Exclusion criteria tokens should also match (we rank by "criteria
        // relevance", not just inclusion relevance).
        let chunks = vec![
            chunk(0, Some("Methods"), "observational cohort adults"),
            chunk(1, Some("Text"), "unrelated prose here"),
        ];
        let inc = criteria(&[]);
        let exc = criteria(&["observational studies adults"]);
        let out = rank_chunks_by_criteria(
            &chunks,
            &inc,
            &exc,
            2,
            DEFAULT_MAX_CHUNK_WORDS,
            DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
        );
        assert_eq!(out[0].chunk_index, 0, "chunk matching exclusion tokens ranks first");
        assert!(out[0].score > 0.0);
    }
}
