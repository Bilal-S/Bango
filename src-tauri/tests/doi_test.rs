//! Integration tests for `ris::doi::normalize_doi`.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/ris/doi.rs` to keep the source file compact.

use bango_lib::ris::doi::normalize_doi;

// ── Valid DOIs should pass through ──────────────────────────────

#[test]
fn test_valid_doi_journals() {
    assert_eq!(
        normalize_doi(Some("10.1016/j.jand.2021.06.013")),
        Some("10.1016/j.jand.2021.06.013")
    );
}

#[test]
fn test_valid_doi_with_slash() {
    assert_eq!(normalize_doi(Some("10.3945/ajcn.114.100925")), Some("10.3945/ajcn.114.100925"));
}

#[test]
fn test_valid_doi_bmj() {
    assert_eq!(normalize_doi(Some("10.1136/bmj.k2477")), Some("10.1136/bmj.k2477"));
}

#[test]
fn test_valid_doi_complex() {
    assert_eq!(normalize_doi(Some("10.1007/s11129-009-9073-0")), Some("10.1007/s11129-009-9073-0"));
}

// ── Placeholder values should return None ──────────────────────

#[test]
fn test_placeholder_na() {
    assert_eq!(normalize_doi(Some("NA")), None);
}

#[test]
fn test_placeholder_na_lowercase() {
    assert_eq!(normalize_doi(Some("na")), None);
}

#[test]
fn test_placeholder_na_mixed_case() {
    assert_eq!(normalize_doi(Some("Na")), None);
}

#[test]
fn test_placeholder_n_a() {
    assert_eq!(normalize_doi(Some("N/A")), None);
}

#[test]
fn test_placeholder_n_a_lowercase() {
    assert_eq!(normalize_doi(Some("n/a")), None);
}

#[test]
fn test_placeholder_null() {
    assert_eq!(normalize_doi(Some("null")), None);
}

#[test]
fn test_placeholder_null_uppercase() {
    assert_eq!(normalize_doi(Some("NULL")), None);
}

#[test]
fn test_placeholder_none_str() {
    assert_eq!(normalize_doi(Some("None")), None);
}

#[test]
fn test_placeholder_dash() {
    assert_eq!(normalize_doi(Some("-")), None);
}

// ── Empty / None values ────────────────────────────────────────

#[test]
fn test_none_input() {
    assert_eq!(normalize_doi(None), None);
}

#[test]
fn test_empty_string() {
    assert_eq!(normalize_doi(Some("")), None);
}

#[test]
fn test_whitespace_only() {
    assert_eq!(normalize_doi(Some("   ")), None);
}

// ── Whitespace trimming ────────────────────────────────────────

#[test]
fn test_trims_leading_trailing_whitespace() {
    assert_eq!(
        normalize_doi(Some(" 10.1016/j.jand.2021.06.013 ")),
        Some("10.1016/j.jand.2021.06.013")
    );
}

#[test]
fn test_trims_whitespace_around_placeholder() {
    assert_eq!(normalize_doi(Some(" NA ")), None);
}

// ── Values that should NOT be treated as placeholders ──────────

#[test]
fn test_non_placeholder_starting_with_na() {
    // "name" starts with "na" but is not a placeholder
    // This should NOT be normalized away - it's a real (though unlikely) value
    assert_eq!(normalize_doi(Some("name")), Some("name"));
}

// ── Edge cases ─────────────────────────────────────────────────

#[test]
fn test_doi_with_unicode() {
    // DOIs are ASCII, but we shouldn't crash on unicode
    assert_eq!(normalize_doi(Some("10.1016/üñïcödé")), Some("10.1016/üñïcödé"));
}

#[test]
fn test_doi_just_spaces_around_valid() {
    assert_eq!(normalize_doi(Some("  10.3390/nu12092535  ")), Some("10.3390/nu12092535"));
}
