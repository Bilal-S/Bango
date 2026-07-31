//! External unit tests for `citation_finder::readiness` (pure helpers).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").
//!
//! `compute_readiness` itself is DB-backed (covered indirectly via
//! `embedding_recall_multistatus_test`); only the pure `coverage_percentage`
//! helper is unit-tested here.

use bango_lib::citation_finder::readiness::coverage_percentage;

// ── coverage_percentage (pure) ───────────────────────────────────────────

#[test]
fn coverage_empty_corpus_is_full() {
    assert_eq!(coverage_percentage(0, 0), 100.0);
}

#[test]
fn coverage_full() {
    assert_eq!(coverage_percentage(10, 10), 100.0);
}

#[test]
fn coverage_half() {
    let pct = coverage_percentage(10, 5);
    assert!((pct - 50.0).abs() < 1e-9);
}

#[test]
fn coverage_zero_embedded() {
    assert_eq!(coverage_percentage(10, 0), 0.0);
}

#[test]
fn coverage_embedded_exceeds_total_clamps() {
    // Defensive: embedded > total shouldn't happen but clamps to 100%.
    assert_eq!(coverage_percentage(10, 15), 100.0);
}

#[test]
fn coverage_negative_embedded_clamps_to_zero() {
    assert_eq!(coverage_percentage(10, -3), 0.0);
}
