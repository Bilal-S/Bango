//! Token-containment passage extraction (`citation_finder/AGENTS.md`).
//!
//! Pure `#[must_use]` functions, no I/O. Reuses the shared
//! `utils::text_tokens::tokenize_for_match` tokenizer so the in-memory scorer
//! agrees with the embedding prefilter on token boundaries.
//!
//! Used by `search.rs` to pick the best-matching chunk per candidate article
//! after the embedding prefilter has narrowed the pool to ~30 articles. The
//! containment score is internal-only (drives passage selection); the
//! user-facing "match %" is the cosine (semantic) score from the recall layer.
//!
//! ## Why containment, not Jaccard
//!
//! The previous implementation used Jaccard set-overlap
//! (`|A ∩ B| / |A ∪ B|`) and a 0.05 threshold. Jaccard penalizes the realistic
//! asymmetric length ratio (a ~12-token query against a ~300-token chunk): an
//! EXACT quote scores Jaccard ≈ 0.04 (12 / 300), which fell below the 0.05
//! cutoff and silently dropped the article before the LLM ever saw it.
//!
//! Containment (`|query ∩ chunk| / |query|`) is the standard IR metric for
//! "short query, long document": it is length-insensitive on the document
//! side, so an exact quote scores 1.0 regardless of chunk length. The
//! threshold is 0.3 ("≥30% of query tokens must appear"), which cleanly
//! separates substantive overlap from noise.

use crate::utils::chunking::Chunk;

/// Minimum containment score for a chunk to be considered a passage match.
/// Candidates whose best chunk scores below this are dropped from the LLM
/// input (`citation_finder/AGENTS.md`: "Token-similarity returns no passage
/// match → candidate excluded").
///
/// Containment is `|query ∩ chunk| / |query|` (query coverage), so 0.3 means
/// "at least 30% of the query's tokens must appear in the chunk." This is the
/// correct scale for containment - it is NOT comparable to the previous
/// Jaccard threshold (0.05), which measured union-overlap and was diluted by
/// long chunks.
pub const MIN_PASSAGE_SCORE: f64 = 0.3;

/// Tokenize text for matching: lowercase, split on non-alphanumeric, drop the
/// 57 English stop words. Delegates to the shared `text_tokens` helper so the
/// citation finder agrees with FTS5 + screening on token boundaries.
///
/// Pure `#[must_use]`.
#[must_use]
pub fn tokenize_and_stem(text: &str) -> Vec<String> {
    crate::utils::text_tokens::tokenize_for_match(text)
}

/// Jaccard similarity on token sets: `|A ∩ B| / |A ∪ B|`. Range 0.0–1.0.
///
/// - Identical token sets → 1.0.
/// - Disjoint token sets → 0.0.
/// - Empty input (either side) → 0.0 (avoids the `0/0` NaN).
///
/// **Not used as the passage gate** - Jaccard penalizes asymmetric lengths
/// (a short query against a long chunk scores low even for exact quotes).
/// Retained as a `pub` helper for tests + potential future tie-breaking; the
/// passage gate uses [`containment`] instead.
///
/// Pure `#[must_use]`.
#[must_use]
pub fn jaccard_similarity(tokens_a: &[String], tokens_b: &[String]) -> f64 {
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }
    use std::collections::HashSet;
    let set_a: HashSet<&String> = tokens_a.iter().collect();
    let set_b: HashSet<&String> = tokens_b.iter().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let sim = intersection as f64 / union as f64;
    sim
}

/// Containment (query coverage): `|query ∩ chunk| / |query|`. Range 0.0–1.0.
///
/// The standard IR metric for "short query, long document" passage matching.
/// Unlike Jaccard, it is **length-insensitive on the chunk side**: an exact
/// quote scores 1.0 regardless of how long the containing chunk is, because
/// the denominator is the *query* token count, not the union.
///
/// - All query tokens present in chunk → 1.0.
/// - Half the query tokens present → 0.5.
/// - Disjoint → 0.0.
/// - Empty query → 0.0 (avoids `0/0` NaN; the caller's `find_best_passage`
///   guards on a non-empty query before scoring).
///
/// Pure `#[must_use]`.
#[must_use]
pub fn containment(query_tokens: &[String], chunk_tokens: &[String]) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    if chunk_tokens.is_empty() {
        return 0.0;
    }
    use std::collections::HashSet;
    let query_set: HashSet<&String> = query_tokens.iter().collect();
    let chunk_set: HashSet<&String> = chunk_tokens.iter().collect();
    let intersection = query_set.intersection(&chunk_set).count();
    #[allow(clippy::cast_precision_loss)]
    let score = intersection as f64 / query_tokens.len() as f64;
    score
}

/// Find the best-matching chunk for a set of user tokens.
///
/// Scores each chunk by **containment** (query coverage) against `user_tokens`,
/// returns the one with the highest score plus its section label and score.
/// Returns `None` when:
/// - `chunks` is empty, OR
/// - the best score is below `MIN_PASSAGE_SCORE` (0.3) - the candidate is
///   excluded from the LLM input.
///
/// Containment (not Jaccard) is used because it is length-insensitive on the
/// chunk side: an exact quote scores 1.0 regardless of chunk length, where
/// Jaccard would score ≈ 0.04 and silently drop the match.
///
/// The section label is `chunk.section.clone()` (`Option<String>`). `None`
/// covers `SectionKind::Text`-derived chunks (no heading); the UI then omits
/// the `§…` badge. For the title+abstract fallback (article has no chunks),
/// the caller synthesizes a chunk with `section: Some("Abstract")`.
///
/// Pure `#[must_use]`.
#[must_use]
pub fn find_best_passage(
    user_tokens: &[String],
    chunks: &[Chunk],
) -> Option<(String, Option<String>, f64)> {
    if chunks.is_empty() {
        return None;
    }
    let mut best: Option<(usize, f64)> = None;
    for (idx, chunk) in chunks.iter().enumerate() {
        let chunk_tokens = tokenize_and_stem(&chunk.text);
        let score = containment(user_tokens, &chunk_tokens);
        match best {
            Some((_, best_score)) if score <= best_score => {}
            _ => best = Some((idx, score)),
        }
    }
    let (idx, score) = best?;
    if score < MIN_PASSAGE_SCORE {
        return None;
    }
    let chunk = &chunks[idx];
    Some((chunk.text.clone(), chunk.section.clone(), score))
}
// Unit tests live in `src-tauri/tests/citation_finder_similarity_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
