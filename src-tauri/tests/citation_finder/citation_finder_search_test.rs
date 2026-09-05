//! Integration tests for the Citation Finder search pipeline's public surface.
//!
//! The async `find_citations_inner` entry point depends on a live Tauri
//! `State<DbState>` + `AppHandle`, so it cannot be driven directly from a
//! `#[test]` (same constraint documented in `tests/embedding_runner_test.rs`).
//! Instead, the pipeline's testable decisions live in pure helpers:
//! - `normalize_claim_key` (the drift-tolerant claim lookup key) - tested here
//!   + in `search.rs` inline.
//! - `merge_outputs`, `pool_finalists` - private, covered by the inline
//!   `#[cfg(test)] mod tests` in `search.rs`.
//!
//! This file exists so the binding test-inventory (`docs/test-plans/citation-
//! finder-tests.md`) has a machine-parseable `path::fn` row pointing at the
//! pipeline layer. The inventory's `scripts/check-test-inventory.sh` greps
//! for `fn <name>(` in the named file.

use bango_lib::citation_finder::search::normalize_claim_key;

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
