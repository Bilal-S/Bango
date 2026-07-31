//! External unit tests for `citation_finder::claim_splitter` (pure helpers).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` per `docs/CLAUDE.md`
//! §Testing ("Avoid large inline unit tests in library source files").

use bango_lib::citation_finder::claim_splitter::{build_claim_splitter_prompt, enforce_max_claims};

// ── build_claim_splitter_prompt ──────────────────────────────────────────

#[test]
fn split_prompt_contains_text() {
    let prompt = build_claim_splitter_prompt("Sugar taxes reduce obesity.");
    assert!(prompt.contains("Sugar taxes reduce obesity."));
    assert!(prompt.contains("at most 5"));
    assert!(prompt.contains("JSON array of strings"));
}

#[test]
fn split_prompt_empty_text_still_builds() {
    let prompt = build_claim_splitter_prompt("");
    assert!(prompt.contains("Text: \"\""));
}

// ── enforce_max_claims ───────────────────────────────────────────────────

#[test]
fn enforce_truncates_to_five() {
    let claims = vec![
        "one".to_string(),
        "two".to_string(),
        "three".to_string(),
        "four".to_string(),
        "five".to_string(),
        "six".to_string(),
        "seven".to_string(),
    ];
    let enforced = enforce_max_claims(claims);
    assert_eq!(enforced.len(), 5);
    assert_eq!(enforced[0], "one");
    assert_eq!(enforced[4], "five");
}

#[test]
fn enforce_passes_through_under_five() {
    let claims = vec!["a".to_string(), "b".to_string()];
    let enforced = enforce_max_claims(claims);
    assert_eq!(enforced.len(), 2);
}

#[test]
fn enforce_trims_whitespace() {
    let claims = vec!["  spaced  ".to_string(), "\ttabbed\t".to_string()];
    let enforced = enforce_max_claims(claims);
    assert_eq!(enforced, vec!["spaced", "tabbed"]);
}

#[test]
fn enforce_drops_empty_claims() {
    let claims =
        vec!["real".to_string(), "   ".to_string(), "".to_string(), "also real".to_string()];
    let enforced = enforce_max_claims(claims);
    assert_eq!(enforced, vec!["real", "also real"]);
}

#[test]
fn enforce_empty_input_returns_empty() {
    let enforced = enforce_max_claims(Vec::new());
    assert!(enforced.is_empty());
}

#[test]
fn enforce_exactly_five_passes_through() {
    let claims =
        vec!["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string(), "5".to_string()];
    let enforced = enforce_max_claims(claims);
    assert_eq!(enforced.len(), 5);
}
