//! External unit tests for `citation_finder::similarity` (pure helpers).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").

use bango_lib::citation_finder::similarity::{
    containment, find_best_passage, jaccard_similarity, tokenize_and_stem,
};
use bango_lib::utils::chunking::Chunk;

fn chunk(section: Option<&str>, text: &str) -> Chunk {
    Chunk {
        section: section.map(str::to_string),
        chunk_index: 0,
        text: text.to_string(),
        word_count: text.split_whitespace().count(),
    }
}

// ── tokenize_and_stem ────────────────────────────────────────────────────

#[test]
fn tokenize_drops_stop_words() {
    let tokens = tokenize_and_stem("the sugar tax and childhood obesity");
    assert_eq!(tokens, vec!["sugar", "tax", "childhood", "obesity"]);
}

#[test]
fn tokenize_handles_punctuation() {
    let tokens = tokenize_and_stem("Sugar taxes (UK SDIL) reduce obesity.");
    assert!(tokens.contains(&"sugar".to_string()));
    assert!(tokens.contains(&"sdil".to_string()));
    assert!(tokens.contains(&"obesity".to_string()));
}

#[test]
fn tokenize_empty_input() {
    assert!(tokenize_and_stem("").is_empty());
    assert!(tokenize_and_stem("the and is").len() == 3); // fallback to all
}

// ── jaccard_similarity (retained pub helper; NOT the passage gate) ───────

#[test]
fn jaccard_identical_sets_is_one() {
    let a = vec!["sugar".to_string(), "tax".to_string()];
    let sim = jaccard_similarity(&a, &a);
    assert!((sim - 1.0).abs() < 1e-9);
}

#[test]
fn jaccard_disjoint_sets_is_zero() {
    let a = vec!["sugar".to_string()];
    let b = vec!["exercise".to_string()];
    assert_eq!(jaccard_similarity(&a, &b), 0.0);
}

#[test]
fn jaccard_partial_overlap() {
    // {a, b, c} vs {b, c, d} → intersection 2, union 4 → 0.5
    let a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let b = vec!["b".to_string(), "c".to_string(), "d".to_string()];
    let sim = jaccard_similarity(&a, &b);
    assert!((sim - 0.5).abs() < 1e-9);
}

#[test]
fn jaccard_empty_input_is_zero() {
    assert_eq!(jaccard_similarity(&[], &[]), 0.0);
    assert_eq!(jaccard_similarity(&["x".to_string()], &[]), 0.0);
    assert_eq!(jaccard_similarity(&[], &["x".to_string()]), 0.0);
}

#[test]
fn jaccard_diluted_by_long_chunk_exact_quote() {
    // The regression that motivated switching the gate to containment: an
    // EXACT quote of 12 tokens inside a ~300-token chunk scores Jaccard
    // ~0.04, which fell below the old 0.05 cutoff and dropped the match.
    // This test pins Jaccard's failure mode so the metric is never reinstated
    // as the passage gate.
    let query: Vec<String> = (0..12).map(|i| format!("q{i}")).collect();
    let mut chunk_tokens = query.clone();
    chunk_tokens.extend((0..288).map(|i| format!("filler{i}")));
    let jaccard = jaccard_similarity(&query, &chunk_tokens);
    // 12 shared / 300 union = 0.04
    assert!(
        jaccard < 0.05,
        "Jaccard for exact quote in long chunk should be < 0.05 (got {jaccard}); \
         this is why Jaccard is NOT used as the passage gate"
    );
}

// ── containment (the passage gate metric) ────────────────────────────────

#[test]
fn containment_exact_quote_in_long_chunk_is_one() {
    // The user-reported scenario: a ~12-token query that appears verbatim
    // inside a ~300-token chunk. Containment = 12/12 = 1.0 (Jaccard would be
    // ~0.04 — see `jaccard_diluted_by_long_chunk_exact_quote`). This is the
    // test that would have caught the shipped bug.
    let query: Vec<String> = (0..12).map(|i| format!("q{i}")).collect();
    let mut chunk_tokens = query.clone();
    chunk_tokens.extend((0..288).map(|i| format!("filler{i}")));
    let score = containment(&query, &chunk_tokens);
    assert!((score - 1.0).abs() < 1e-9, "exact quote → containment 1.0, got {score}");
}

#[test]
fn containment_partial_overlap_is_query_fraction() {
    // Query has 10 tokens, 4 appear in chunk → containment 0.4.
    let query: Vec<String> = (0..10).map(|i| format!("q{i}")).collect();
    let chunk: Vec<String> = (0..4).map(|i| format!("q{i}")).collect();
    let score = containment(&query, &chunk);
    assert!((score - 0.4).abs() < 1e-9, "4/10 → 0.4, got {score}");
}

#[test]
fn containment_disjoint_is_zero() {
    let query = vec!["sugar".to_string(), "tax".to_string()];
    let chunk = vec!["exercise".to_string(), "diet".to_string()];
    assert_eq!(containment(&query, &chunk), 0.0);
}

#[test]
fn containment_empty_query_is_zero() {
    // Avoids 0/0 NaN.
    assert_eq!(containment(&[], &["x".to_string()]), 0.0);
}

#[test]
fn containment_empty_chunk_is_zero() {
    let query = vec!["sugar".to_string()];
    assert_eq!(containment(&query, &[]), 0.0);
}

#[test]
fn containment_is_length_insensitive_on_chunk_side() {
    // The same 5-token query against a 10-token chunk and a 1000-token chunk
    // (both containing all 5 query tokens) → containment 1.0 in both cases.
    // This is the defining property that makes containment correct for
    // asymmetric lengths where Jaccard fails.
    let query: Vec<String> = (0..5).map(|i| format!("q{i}")).collect();
    let short_chunk: Vec<String> =
        query.iter().take(5).cloned().chain((0..5).map(|i| format!("s{i}"))).collect();
    let long_chunk: Vec<String> =
        query.iter().take(5).cloned().chain((0..995).map(|i| format!("l{i}"))).collect();
    assert!((containment(&query, &short_chunk) - 1.0).abs() < 1e-9);
    assert!((containment(&query, &long_chunk) - 1.0).abs() < 1e-9);
}

// ── find_best_passage (uses containment under the hood) ──────────────────

#[test]
fn find_best_passage_empty_chunks_returns_none() {
    let user = tokenize_and_stem("sugar tax obesity");
    assert!(find_best_passage(&user, &[]).is_none());
}

#[test]
fn find_best_passage_single_chunk() {
    let user = tokenize_and_stem("sugar tax obesity");
    let chunks = vec![chunk(Some("Results"), "the sugar tax reduced obesity significantly")];
    let result = find_best_passage(&user, &chunks);
    assert!(result.is_some());
    let (passage, section, score) = result.unwrap();
    assert!(passage.contains("sugar tax"));
    assert_eq!(section.as_deref(), Some("Results"));
    assert!(score > 0.0);
}

#[test]
fn find_best_passage_picks_highest_scoring_chunk() {
    let user = tokenize_and_stem("sugar tax childhood obesity");
    let chunks = vec![
        chunk(Some("Methods"), "we used a qualitative survey of adults"), // low overlap
        chunk(Some("Results"), "the sugar tax significantly reduced childhood obesity"), // high
        chunk(Some("Discussion"), "these findings align with prior work"), // low
    ];
    let result = find_best_passage(&user, &chunks).expect("some");
    let (passage, section, _score) = result;
    assert!(passage.contains("significantly reduced childhood obesity"));
    assert_eq!(section.as_deref(), Some("Results"));
}

#[test]
fn find_best_passage_below_threshold_returns_none() {
    // User tokens share nothing with the chunk → containment 0.0 < 0.3.
    let user = tokenize_and_stem("sugar tax obesity");
    let chunks = vec![chunk(Some("Methods"), "qualitative survey methodology interview")];
    assert!(find_best_passage(&user, &chunks).is_none());
}

#[test]
fn find_best_passage_preserves_none_section() {
    // Text-derived chunks carry section: None.
    let user = tokenize_and_stem("sugar tax obesity");
    let chunks = vec![chunk(None, "the sugar tax obesity study findings")];
    let result = find_best_passage(&user, &chunks).expect("some");
    let (_passage, section, _score) = result;
    assert!(section.is_none(), "None section preserved verbatim");
}

#[test]
fn find_best_passage_tie_breaking_prefers_first() {
    // Two chunks with identical scores → the first one wins (the loop
    // uses strict `>` so ties don't replace).
    let user = tokenize_and_stem("sugar tax");
    let chunks = vec![
        chunk(Some("Methods"), "sugar tax methods"),
        chunk(Some("Results"), "sugar tax results"),
    ];
    let result = find_best_passage(&user, &chunks).expect("some");
    let (_passage, section, _score) = result;
    assert_eq!(section.as_deref(), Some("Methods"), "first chunk wins ties");
}

#[test]
fn find_best_passage_exact_quote_in_realistic_long_chunk() {
    // Regression for the user-reported bug: a sentence-length quote pasted
    // against a realistic ~200-word chunk (the size `chunk_sections`
    // produces). Under the OLD Jaccard gate this scored ~0.04 (< 0.05) and
    // was dropped. Under containment it scores 1.0 and passes.
    let quote =
        "Carotenoids provide yellow orange red colors readily undergo oxidative degradation";
    let filler = " IntroductionMethodsStudy designParticipantsWe recruited adults ".repeat(20);
    let long_chunk_text = format!("{quote} {filler}");
    let user = tokenize_and_stem(quote);
    let chunks = vec![chunk(Some("Introduction"), &long_chunk_text)];
    let result = find_best_passage(&user, &chunks);
    assert!(
        result.is_some(),
        "exact quote in a realistic-length chunk must pass the containment gate"
    );
    let (passage, _section, score) = result.unwrap();
    assert!(passage.contains("Carotenoids"), "the matching passage is returned");
    assert!(score >= 0.3, "containment score passes the 0.3 threshold (got {score})");
}
