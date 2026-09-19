//! Integration tests for `commands::embedding::regenerate_scope_statuses`.
//!
//! Contract: the mismatch-dialog Regenerate rebuilds ONLY the checked statuses
//! and falls back to `included` when nothing is checked. The same normalized
//! scope feeds both the delete and the re-embed runner scope, so a regenerate
//! never wipes more than it rebuilds.

use bango_lib::commands::embedding::regenerate_scope_statuses;

#[test]
fn regenerate_scope_defaults_to_included_when_empty() {
    assert_eq!(regenerate_scope_statuses(None), vec!["included".to_string()]);
    assert_eq!(regenerate_scope_statuses(Some("")), vec!["included".to_string()]);
    assert_eq!(regenerate_scope_statuses(Some(" , ")), vec!["included".to_string()]);
}

#[test]
fn regenerate_scope_keeps_only_the_checked_statuses() {
    assert_eq!(regenerate_scope_statuses(Some("working")), vec!["working".to_string()]);
    assert_eq!(
        regenerate_scope_statuses(Some(" working , rejected ")),
        vec!["working".to_string(), "rejected".to_string()]
    );
}
