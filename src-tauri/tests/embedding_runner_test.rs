//! Integration tests for the embedding runner's pure helpers.
//!
//! The async `generate_embeddings_inner` flow depends on a live Tauri
//! `AppHandle` + `DbState` + `Arc<LlmOrchestrator>`, so it cannot be driven
//! directly from a `#[test]`. Instead, the runner's testable decisions are
//! extracted into pure `#[must_use]` helpers (`resolve_effective_dim`,
//! `vector_matches_dim`) and tested here. This is the same pattern the
//! director + recall modules use (pure helpers fully covered, async glue
//! verified via the storage/director/recall integration suites).
//!
//! These tests are the regression suite for the runner "dimensions trust gap"
//! bug: previously the runner discarded the provider-returned dimensions and
//! blindly stored the probe-time value, so a provider that returned vectors
//! of an unexpected length silently wrote a wrong `dimensions` column,
//! corrupting recall. The fix has two layers, each covered below:
//!   1. `resolve_effective_dim` — pick the correct effective dimensionality.
//!   2. `vector_matches_dim` — per-row length guard before storage.

use bango_lib::embedding::runner::{resolve_effective_dim, vector_matches_dim};

// ── resolve_effective_dim ────────────────────────────────────────────────────

#[test]
fn effective_dim_keeps_probe_when_returned_is_zero() {
    // Providers sometimes report `0` as a sentinel for "unknown dimension".
    // The probe value is known-good, so keep it.
    assert_eq!(resolve_effective_dim(1536, 0), 1536);
    assert_eq!(resolve_effective_dim(768, 0), 768);
}

#[test]
fn effective_dim_keeps_probe_when_returned_matches() {
    // Agreement: no drift, no change.
    assert_eq!(resolve_effective_dim(1536, 1536), 1536);
    assert_eq!(resolve_effective_dim(768, 768), 768);
}

#[test]
fn effective_dim_trusts_provider_on_drift() {
    // The probe is stale (model was swapped between probe and call). Trust the
    // provider's reported dimensionality so storage + recall use the real value.
    assert_eq!(resolve_effective_dim(1536, 768), 768, "drift to smaller dim trusts provider");
    assert_eq!(resolve_effective_dim(768, 1536), 1536, "drift to larger dim trusts provider");
    assert_eq!(resolve_effective_dim(1536, 3072), 3072, "drift to very large dim trusts provider");
}

#[test]
fn effective_dim_keeps_probe_when_returned_negative() {
    // A negative returned_dim is garbage; don't let it blow away the probe.
    assert_eq!(resolve_effective_dim(1536, -1), 1536);
    assert_eq!(resolve_effective_dim(1536, -768), 1536);
}

#[test]
fn effective_dim_handles_zero_probe() {
    // Edge case: the probe itself returned 0 (unknown). If the provider reports
    // a real positive dim, use it; otherwise stay at 0.
    assert_eq!(resolve_effective_dim(0, 768), 768, "provider value salvages a zero probe");
    assert_eq!(resolve_effective_dim(0, 0), 0, "both zero stays zero");
}

// ── vector_matches_dim ───────────────────────────────────────────────────────

#[test]
fn vector_matches_dim_accepts_exact_length() {
    let v = vec![0.1; 768];
    assert!(vector_matches_dim(&v, 768));
    assert!(vector_matches_dim(&[], 0));
}

#[test]
fn vector_matches_dim_rejects_truncated_vector() {
    // Bug #1 scenario: a batch endpoint silently truncated one vector. Without
    // the per-row guard this would store a wrong `dimensions` column and
    // corrupt recall; with the guard it is skipped + counted as an error.
    let truncated = vec![0.1; 512];
    assert!(!vector_matches_dim(&truncated, 768), "truncated vector rejected");
}

#[test]
fn vector_matches_dim_rejects_oversized_vector() {
    let oversized = vec![0.1; 1024];
    assert!(!vector_matches_dim(&oversized, 768), "oversized vector rejected");
}

#[test]
fn vector_matches_dim_rejects_dimension_mismatch_after_drift() {
    // Even when resolve_effective_dim picks the provider's drifted value, a
    // vector from the OLD model (1536) must be rejected against the new
    // effective dim (768) so it doesn't get stored with the wrong column.
    let old_vec = vec![0.1; 1536];
    let new_effective = resolve_effective_dim(1536, 768); // = 768
    assert_eq!(new_effective, 768);
    assert!(
        !vector_matches_dim(&old_vec, new_effective),
        "old-dim vector rejected against the new effective dim"
    );
}
