//! Unit + integration tests for `citation_finder::search`.
//!
//! The `normalize_claim_key` rows pin the drift-tolerant claim pairing for the
//! binding test-inventory (`docs/test-plans/citation-finder-tests.md`).
//! The `merge_outputs` / `pool_finalists` / `cosine_best_chunk` sections were
//! extracted from the inline `#[cfg(test)] mod tests` in
//! `src/citation_finder/search.rs` to keep the source file compact (helpers
//! tested externally are `pub`, per `tests/AGENTS.md`).

use std::collections::HashMap;

use bango_lib::citation_finder::prompt::{CandidateMetadata, CitationLlmOutput};
use bango_lib::citation_finder::search::{
    cosine_best_chunk, merge_outputs, normalize_claim_key, pool_finalists, ClaimWork, Finalists,
    PassageEvidence,
};
use bango_lib::citation_finder::MatchClassification;
use bango_lib::embedding::recall::EmbeddingHit;
use bango_lib::utils::chunking::Chunk;

// ── normalize_claim_key (external pin on the pub helper) ─────────────────

#[test]
fn normalize_claim_key_drift_tolerant_pipeline_contract() {
    // The pipeline contract: the claim-splitter produces a claim, the LLM
    // echoes it with cosmetic drift, and `merge_outputs` must still pair the
    // LLM output with the recall-layer cosine score. This pins the helper
    // that drives that pairing so a future refactor cannot silently break it.
    assert_eq!(normalize_claim_key("  Sugar   taxes  "), normalize_claim_key("Sugar taxes"));
    assert_eq!(normalize_claim_key("SUGAR TAXES"), normalize_claim_key("sugar taxes"));
    assert_eq!(normalize_claim_key("Sugar\ttaxes"), normalize_claim_key(" sugar  taxes "));
}

#[test]
fn normalize_claim_key_empty_input_is_stable() {
    // Empty / whitespace-only inputs collapse to the empty string, which is
    // the whole-block claim key. Whole-block + per-statement must not collide
    // (per-statement always carries a non-empty claim after enforce_max_claims
    // drops empty claims).
    assert_eq!(normalize_claim_key(""), "");
    assert_eq!(normalize_claim_key("   "), "");
    assert_eq!(normalize_claim_key("\t\n"), "");
}

#[test]
fn normalize_claim_key_does_not_strip_punctuation() {
    // Punctuation drift (trailing period, comma) is NOT normalized away: the
    // splitter and classifier both receive the claim as a JSON string, so
    // trailing-punctuation drift is a real possibility but is rarer than
    // whitespace/case drift. If we stripped punctuation here we would risk
    // false-positive pairings between distinct claims that happen to share
    // tokens. The conservative choice is whitespace + case only.
    let with_period = normalize_claim_key("Sugar taxes reduce obesity.");
    let without_period = normalize_claim_key("Sugar taxes reduce obesity");
    assert_ne!(with_period, without_period, "punctuation drift is intentionally NOT erased");
}

/// Build a `ClaimWork` for testing. Passages take `(article_id, passage, section, score)`;
/// abstract context defaults to `None` (use `claim_work_with_abstract`
/// when the grounding test needs it).
fn claim_work(
    claim: &str,
    hits: Vec<(&str, f32)>,
    passages: Vec<(&str, &str, Option<&str>, f64)>,
) -> ClaimWork {
    ClaimWork {
        text: claim.to_string(),
        hits: hits
            .into_iter()
            .map(|(id, score)| EmbeddingHit {
                article_id: id.to_string(),
                score,
                chunk_index: None,
            })
            .collect(),
        passages: passages
            .into_iter()
            .map(|(id, passage, section, score)| PassageEvidence {
                article_id: id.to_string(),
                passage: passage.to_string(),
                section: section.map(str::to_string),
                score,
                abstract_text: None,
            })
            .collect(),
    }
}

/// Like [`claim_work`] but attaches abstract context to every passage
/// entry (for grounding tests).
fn claim_work_with_abstract(
    claim: &str,
    hits: Vec<(&str, f32)>,
    passages: Vec<(&str, &str, Option<&str>, f64, &str)>,
) -> ClaimWork {
    ClaimWork {
        text: claim.to_string(),
        hits: hits
            .into_iter()
            .map(|(id, score)| EmbeddingHit {
                article_id: id.to_string(),
                score,
                chunk_index: None,
            })
            .collect(),
        passages: passages
            .into_iter()
            .map(|(id, passage, section, score, abs)| PassageEvidence {
                article_id: id.to_string(),
                passage: passage.to_string(),
                section: section.map(str::to_string),
                score,
                abstract_text: Some(abs.to_string()),
            })
            .collect(),
    }
}

fn meta_map(ids: &[&str]) -> HashMap<String, CandidateMetadata> {
    ids.iter()
        .map(|id| {
            (
                id.to_string(),
                CandidateMetadata {
                    article_id: id.to_string(),
                    title: format!("Title {id}"),
                    authors: vec!["Author".to_string()],
                    publication_year: Some(2024),
                    journal: Some("Journal".to_string()),
                    doi: Some(format!("10.1000/{id}")),
                },
            )
        })
        .collect()
}

fn llm_out(
    article_id: &str,
    claim: &str,
    classification: &str,
    misrepresents: bool,
) -> CitationLlmOutput {
    CitationLlmOutput {
        article_id: article_id.to_string(),
        claim: claim.to_string(),
        classification: classification.to_string(),
        relevance_explanation: "explanation".to_string(),
        misrepresents_source: misrepresents,
        justifying_sentences: Vec::new(),
    }
}

// ── normalize_claim_key ──────────────────────────────────────────────

#[test]
fn normalize_claim_key_trims_and_lowercases() {
    assert_eq!(normalize_claim_key("  Sugar Taxes  "), "sugar taxes");
}

#[test]
fn normalize_claim_key_collapses_internal_whitespace() {
    assert_eq!(normalize_claim_key("Sugar   taxes\treduce\nobesity"), "sugar taxes reduce obesity");
}

#[test]
fn normalize_claim_key_empty_stays_empty() {
    assert_eq!(normalize_claim_key(""), "");
    assert_eq!(normalize_claim_key("   "), "");
}

#[test]
fn normalize_claim_key_preserves_punctuation() {
    // Punctuation is NOT stripped (only whitespace + case). The drift we
    // guard against is whitespace/case, not trailing-period differences
    // (those still won't match, but that's a rarer drift than whitespace
    // collapse).
    assert_eq!(normalize_claim_key("Sugar taxes."), "sugar taxes.");
}

// ── merge_outputs: whole-block ───────────────────────────────────────

#[test]
fn merge_whole_block_uses_empty_claim_key() {
    // Whole-block: claim_filter = None → empty claim key. The cosine from
    // the recall hit should flow through to confidence.
    let work =
        claim_work("ignored", vec![("a1", 0.8)], vec![("a1", "passage", Some("Results"), 0.5)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![llm_out("a1", "", "validating", false)];

    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].article_id, "a1");
    assert_eq!(matches[0].classification, MatchClassification::Validating);
    // cosine 0.8 → (0.8 + 1) / 2 = 0.9
    assert!((matches[0].confidence - 0.9).abs() < 1e-5, "got {}", matches[0].confidence);
    assert_eq!(matches[0].section_origin.as_deref(), Some("Results"));
    assert!(!matches[0].misrepresents_source);
}

// ── merge_outputs: per-statement claim-key drift ─────────────────────

#[test]
fn merge_per_statement_handles_claim_whitespace_drift() {
    // The splitter produced "Sugar taxes reduce obesity." but the LLM
    // echoed "Sugar   taxes reduce obesity." (extra spaces). Without
    // normalize_claim_key the cosine lookup would miss and confidence
    // would silently fall to 0.5. With normalization the real cosine
    // (0.6 → 0.8 confidence) flows through.
    let splitter_claim = "Sugar taxes reduce obesity.";
    let llm_echoed_claim = "Sugar   taxes reduce obesity.";
    let work = claim_work(splitter_claim, vec![("a1", 0.6)], vec![("a1", "passage", None, 0.3)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![llm_out("a1", llm_echoed_claim, "validating", false)];

    let matches = merge_outputs(&outputs, &finalists, &metadata, Some(splitter_claim));
    assert_eq!(matches.len(), 1, "claim drift must not drop the match");
    // cosine 0.6 → (0.6 + 1) / 2 = 0.8 (NOT the 0.5 fallback).
    assert!(
        (matches[0].confidence - 0.8).abs() < 1e-5,
        "normalized key should recover the real cosine; got {}",
        matches[0].confidence
    );
}

#[test]
fn merge_per_statement_handles_claim_case_drift() {
    let splitter_claim = "Sugar taxes reduce obesity.";
    let llm_echoed_claim = "SUGAR TAXES REDUCE OBESITY.";
    let work = claim_work(splitter_claim, vec![("a1", 0.4)], vec![("a1", "p", None, 0.2)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![llm_out("a1", llm_echoed_claim, "opposing", true)];

    let matches = merge_outputs(&outputs, &finalists, &metadata, Some(splitter_claim));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].classification, MatchClassification::Opposing);
    assert!(matches[0].misrepresents_source);
    // cosine 0.4 → (0.4 + 1) / 2 = 0.7
    assert!((matches[0].confidence - 0.7).abs() < 1e-5);
}

#[test]
fn merge_per_statement_filters_to_claim_filter() {
    // Only outputs whose raw claim matches claim_filter are included.
    let claim_a = "Claim A.";
    let claim_b = "Claim B.";
    let work_a = claim_work(claim_a, vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
    let work_b = claim_work(claim_b, vec![("a2", 0.5)], vec![("a2", "p", None, 0.1)]);
    let finalists = Finalists {
        article_ids: vec!["a1".to_string(), "a2".to_string()],
        per_claim: vec![work_a, work_b],
    };
    let metadata = meta_map(&["a1", "a2"]);
    let outputs = vec![
        llm_out("a1", claim_a, "validating", false),
        llm_out("a2", claim_b, "validating", false),
    ];

    let only_a = merge_outputs(&outputs, &finalists, &metadata, Some(claim_a));
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].article_id, "a1");
}

// ── merge_outputs: drop paths ────────────────────────────────────────

#[test]
fn merge_drops_hallucinated_article_id() {
    // LLM invented an article_id not in metadata → dropped.
    let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![
        llm_out("a1", "", "validating", false),
        llm_out("ghost", "", "validating", false), // not in metadata
    ];
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].article_id, "a1");
}

#[test]
fn merge_drops_unrelated_and_garbage_classifications() {
    let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![
        llm_out("a1", "", "validating", false),
        llm_out("a1", "", "unrelated", false), // filtered by prompt, dropped here
        llm_out("a1", "", "maybe", false),     // garbage
    ];
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert_eq!(matches.len(), 1);
}

#[test]
fn merge_truncates_to_ten() {
    // 12 outputs for the same article → truncated to 10.
    let work = claim_work("text", vec![("a1", 0.5)], vec![("a1", "p", None, 0.1)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs: Vec<CitationLlmOutput> =
        (0..12).map(|_| llm_out("a1", "", "validating", false)).collect();
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert_eq!(matches.len(), 10);
}

// ── merge_outputs: cosine normalization edge cases ───────────────────

#[test]
fn merge_confidence_negative_cosine_normalizes_correctly() {
    // cosine -1.0 (opposite direction) → (-1 + 1) / 2 = 0.0
    let work = claim_work("text", vec![("a1", -1.0)], vec![("a1", "p", None, 0.1)]);
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![llm_out("a1", "", "validating", false)];
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert!((matches[0].confidence - 0.0).abs() < 1e-5, "got {}", matches[0].confidence);
}

#[test]
fn merge_confidence_missing_cosine_falls_to_neutral() {
    // Article in metadata but NOT in recall hits (cosine unset / 0.0
    // default) → (0 + 1) / 2 = 0.5 neutral.
    let work = claim_work("text", vec![], vec![]); // no hits, no passages
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![llm_out("a1", "", "validating", false)];
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert!((matches[0].confidence - 0.5).abs() < 1e-5, "got {}", matches[0].confidence);
}

// ── pool_finalists ───────────────────────────────────────────────────

#[test]
fn pool_finalists_dedups_article_ids_keeping_best_score() {
    // Same article appears in two claims with different scores; the union
    // keeps it once.
    let w1 = claim_work("c1", vec![("a1", 0.5)], vec![("a1", "p1", None, 0.3)]);
    let w2 = claim_work("c2", vec![("a1", 0.7)], vec![("a1", "p2", None, 0.6)]);
    let finalists = pool_finalists(vec![w1, w2]);
    assert_eq!(finalists.article_ids.len(), 1);
    assert_eq!(finalists.article_ids[0], "a1");
    // Both per-claim works are preserved (for per-statement grouping).
    assert_eq!(finalists.per_claim.len(), 2);
}

#[test]
fn pool_finalists_truncates_to_fifteen() {
    // 20 distinct articles, containment and cosine perfectly correlated →
    // the cosine union adds nothing (all 5 cosine-best are already in the
    // containment top-15) → still exactly 15.
    let works: Vec<ClaimWork> = (0..20)
        .map(|i| {
            claim_work(
                "c",
                vec![(format!("a{i}").leak(), 0.1 * i as f32)],
                vec![(format!("a{i}").leak(), "p", None, 0.1 * i as f64)],
            )
        })
        .collect();
    let finalists = pool_finalists(works);
    assert_eq!(finalists.article_ids.len(), 15);
}

#[test]
fn pool_finalists_cosine_union_rescues_low_containment_article() {
    // 15 articles with high containment (0.9..0.76) fill the containment
    // slots; article "rescue" has weak containment (0.32, paraphrased
    // claim) but the top cosine (0.99). Containment-only truncation (the
    // pre-union behavior) would evict it; the union must keep it.
    let mut works: Vec<ClaimWork> = (0..15)
        .map(|i| {
            claim_work(
                "c",
                vec![(format!("f{i}").leak(), 0.2)],
                vec![(format!("f{i}").leak(), "p", None, 0.9 - 0.01 * i as f64)],
            )
        })
        .collect();
    works.push(claim_work("c", vec![("rescue", 0.99)], vec![("rescue", "p", None, 0.32)]));
    let finalists = pool_finalists(works);
    assert!(
        finalists.article_ids.contains(&"rescue".to_string()),
        "cosine union must rescue the paraphrased match"
    );
    assert_eq!(finalists.article_ids.len(), 16);
}

#[test]
fn pool_finalists_caps_union_at_twenty() {
    // 18 containment survivors + 5 high-cosine non-survivors → capped at 20.
    let mut works: Vec<ClaimWork> = (0..18)
        .map(|i| {
            claim_work(
                "c",
                vec![(format!("f{i}").leak(), 0.05)],
                vec![(format!("f{i}").leak(), "p", None, 0.5)],
            )
        })
        .collect();
    for i in 0..5 {
        // No passage (gate-dropped): only present in the cosine ranking.
        works.push(claim_work("c", vec![(format!("x{i}").leak(), 0.9)], vec![]));
    }
    let finalists = pool_finalists(works);
    assert_eq!(finalists.article_ids.len(), 20);
}

#[test]
fn pool_finalists_filters_passages_to_finalist_set() {
    // Works carrying 18 passage entries (only 15 can be finalists) must
    // have their per-claim passages filtered to the finalist set, so the
    // classification prompt never carries un-promptable candidates.
    // Cosine scores correlate with containment so the union adds nothing.
    let works: Vec<ClaimWork> = (0..18)
        .map(|i| {
            claim_work(
                "c",
                vec![(format!("a{i}").leak(), 0.1 * i as f32)],
                vec![(format!("a{i}").leak(), "p", None, 0.1 * i as f64)],
            )
        })
        .collect();
    let finalists = pool_finalists(works);
    assert_eq!(finalists.article_ids.len(), 15);
    let survivors: usize = finalists.per_claim.iter().map(|w| w.passages.len()).sum();
    assert_eq!(survivors, 15, "per-claim passages must be filtered to finalists");
}

#[test]
fn pool_finalists_empty_works_yields_empty() {
    let finalists = pool_finalists(vec![]);
    assert!(finalists.article_ids.is_empty());
}

// ── cosine_best_chunk ────────────────────────────────────────────────

#[test]
fn cosine_best_chunk_resolves_valid_index() {
    let chunks = vec![
        Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 },
        Chunk {
            section: Some("Methods".to_string()),
            chunk_index: 1,
            text: "methods".to_string(),
            word_count: 1,
        },
    ];
    let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(1) };
    assert_eq!(cosine_best_chunk(&hit, &chunks).map(|c| c.text.as_str()), Some("methods"));
}

#[test]
fn cosine_best_chunk_title_abstract_row_is_none() {
    // chunk_index = -1 is the title+abstract embedding row, not a chunk.
    let chunks =
        vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
    let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(-1) };
    assert!(cosine_best_chunk(&hit, &chunks).is_none());
}

#[test]
fn cosine_best_chunk_out_of_range_is_none() {
    // Stale chunk provenance after a re-chunk: index beyond the list.
    let chunks =
        vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
    let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: Some(7) };
    assert!(cosine_best_chunk(&hit, &chunks).is_none());
}

#[test]
fn cosine_best_chunk_missing_provenance_is_none() {
    let chunks =
        vec![Chunk { section: None, chunk_index: 0, text: "intro".to_string(), word_count: 1 }];
    let hit = EmbeddingHit { article_id: "a1".to_string(), score: 0.5, chunk_index: None };
    assert!(cosine_best_chunk(&hit, &chunks).is_none());
}

// ── merge_outputs: abstract-context grounding ─────────────────────────

#[test]
fn merge_grounds_against_abstract_context() {
    // The classifier quoted the paper's thesis sentence from the abstract
    // context (not present in the chunk passage). The grounding gate must
    // accept it (passage + abstract is the source), otherwise abstract
    // evidence could never surface as a highlighted sentence.
    let work = claim_work_with_abstract(
        "text",
        vec![("a1", 0.6)],
        vec![(
            "a1",
            "Methods paragraph about regression models.",
            None,
            0.5,
            "Title\n\nThe sugar tax reduced purchases of sugary drinks.",
        )],
    );
    let finalists = Finalists { article_ids: vec!["a1".to_string()], per_claim: vec![work] };
    let metadata = meta_map(&["a1"]);
    let outputs = vec![CitationLlmOutput {
        article_id: "a1".to_string(),
        claim: String::new(),
        classification: "validating".to_string(),
        relevance_explanation: "expl".to_string(),
        misrepresents_source: false,
        justifying_sentences: vec!["The sugar tax reduced purchases of sugary drinks.".to_string()],
    }];
    let matches = merge_outputs(&outputs, &finalists, &metadata, None);
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].highlighted_sentences.len(),
        1,
        "abstract-context quote must survive grounding"
    );
}
