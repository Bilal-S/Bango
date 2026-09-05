//! Token-containment passage extraction.
//!
//! Pure `#[must_use]` functions, no I/O. Reuses `utils::text_tokens::tokenize_for_match`
//! so the in-memory scorer agrees with the embedding prefilter on token boundaries.
//!
//! Used by `search.rs` to pick the best-matching chunk per candidate after
//! the embedding prefilter has narrowed the pool to ~30 articles. Containment
//! score is internal-only (drives passage selection); the user-facing "match %"
//! is the cosine score from the recall layer.
//!
//! ## Why containment, not Jaccard
//!
//! The previous Jaccard gate (`|A ∩ B| / |A ∪ B|`, threshold 0.05) penalized
//! asymmetric length ratios: a ~12-token query against a ~300-token chunk
//! scored Jaccard ≈ 0.04 (< 0.05) even for an EXACT quote, silently dropping
//! the match before the LLM saw it.
//!
//! Containment (`|query ∩ chunk| / |query|`) is length-insensitive on the
//! document side: an exact quote scores 1.0 regardless of chunk length.
//! Threshold is 0.3 ("≥30% of query tokens must appear").

use crate::utils::chunking::Chunk;

/// Minimum containment score for passage matching. Candidates below this
/// are dropped from the LLM input. Containment is `|query ∩ chunk| / |query|`
/// (query coverage); 0.3 = ≥30% of query tokens must appear in the chunk.
pub const MIN_PASSAGE_SCORE: f64 = 0.3;

/// Tokenize text for matching: lowercase, split on non-alphanumeric, drop
/// 57 English stop words. Delegates to shared `text_tokens`. `#[must_use]`.
#[must_use]
pub fn tokenize_and_stem(text: &str) -> Vec<String> {
    crate::utils::text_tokens::tokenize_for_match(text)
}

/// Jaccard similarity on token sets: `|A ∩ B| / |A ∪ B|`. Range 0.0–1.0.
/// Empty input (either side) → 0.0.
///
/// **Not the passage gate** — Jaccard penalizes asymmetric lengths. Retained
/// as a `pub` helper for tests; the passage gate uses [`containment`].
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
/// Length-insensitive on the chunk side: exact quote → 1.0 regardless of
/// chunk length. Empty query or chunk → 0.0. Pure `#[must_use]`.
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

/// Find the best-matching chunk for user tokens, by containment.
///
/// Returns `None` when `chunks` is empty OR best score < `MIN_PASSAGE_SCORE`
/// (0.3). Section label is `chunk.section.clone()` (`None` for
/// `SectionKind::Text`; the caller synthesizes `Some("Abstract")` for
/// abstract-only articles). Pure `#[must_use]`.
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
// Unit tests live in `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs`
// (extracted per `docs/CLAUDE.md` §Testing).
