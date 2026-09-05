//! External unit tests for `citation_finder::filter_valid_statuses` (pure).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").

use bango_lib::citation_finder::{filter_valid_statuses, CITATION_STATUS_WHITELIST};

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(std::string::ToString::to_string).collect()
}

// ── filter_valid_statuses (whitelist contract) ───────────────────────────

#[test]
fn filter_valid_statuses_keeps_valid_three() {
    let out = filter_valid_statuses(&strs(&["working", "included", "rejected"]));
    assert_eq!(out, strs(&["working", "included", "rejected"]));
}

#[test]
fn filter_valid_statuses_drops_duplicate_status() {
    // `duplicate` is deliberately NOT in the whitelist - duplicates are
    // never citation candidates.
    let out = filter_valid_statuses(&strs(&["working", "duplicate", "included"]));
    assert_eq!(out, strs(&["working", "included"]));
}

#[test]
fn filter_valid_statuses_drops_garbage_and_empty() {
    let out = filter_valid_statuses(&strs(&["working", "", "garbage", "included", "; DROP--"]));
    assert_eq!(out, strs(&["working", "included"]));
}

#[test]
fn filter_valid_statuses_case_insensitive() {
    let out = filter_valid_statuses(&strs(&["Working", "INCLUDED", "rejected"]));
    assert_eq!(out, strs(&["working", "included", "rejected"]));
}

#[test]
fn filter_valid_statuses_drops_duplicates_preserving_first_order() {
    let out = filter_valid_statuses(&strs(&["included", "working", "included", "working"]));
    assert_eq!(out, strs(&["included", "working"]));
}

#[test]
fn filter_valid_statuses_empty_input_returns_empty() {
    // KEY CONTRACT: an empty result means the search returns "No articles
    // match the selected filters." - the backend does NOT fall back to
    // "all statuses" (the standalone `recall_articles` command's empty-
    // means-all contract is a separate code path).
    let out = filter_valid_statuses(&[]);
    assert!(out.is_empty());
}

#[test]
fn filter_valid_statuses_all_invalid_returns_empty() {
    let out = filter_valid_statuses(&strs(&["foo", "bar", "duplicate"]));
    assert!(out.is_empty());
}

#[test]
fn citation_status_whitelist_excludes_duplicate() {
    // Belt-and-suspenders: the constant itself must not contain duplicate.
    assert!(!CITATION_STATUS_WHITELIST.contains(&"duplicate"));
    assert!(CITATION_STATUS_WHITELIST.contains(&"working"));
    assert!(CITATION_STATUS_WHITELIST.contains(&"included"));
    assert!(CITATION_STATUS_WHITELIST.contains(&"rejected"));
}
