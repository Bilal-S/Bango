//! Coverage for screening::token_estimation pure functions.
use bango_lib::screening::token_estimation::{check_context_window, estimate_tokens};

#[test]
fn estimate_tokens_chars_divided_by_four() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcdefgh"), 2);
    // Uses char count, not byte count: 4 unicode chars = 1 token.
    assert_eq!(estimate_tokens("éééé"), 1);
}

#[test]
fn check_context_window_under_threshold_returns_none() {
    let res = check_context_window(100, &[200, 300, 150], 1000);
    // worst = 300 + 100 = 400; threshold = 800 -> None
    assert!(res.is_none());
}

#[test]
fn check_context_window_over_threshold_returns_warning() {
    let res = check_context_window(100, &[200, 950, 150], 1000);
    // worst = 950 + 100 = 1050; threshold = 800 -> Some
    assert!(res.is_some());
    let msg = res.expect("warning");
    assert!(msg.contains("1050"), "message should include worst-case count: {msg}");
    assert!(msg.contains("800"), "message should include threshold: {msg}");
}

#[test]
fn check_context_window_empty_articles_uses_template_only() {
    // worst = 0 + 100 = 100; threshold = 800 -> None
    let res = check_context_window(100, &[], 1000);
    assert!(res.is_none());
}

#[test]
fn check_context_window_exact_threshold_returns_none() {
    // worst = 700 + 100 = 800; threshold = 800; not > so None
    let res = check_context_window(100, &[700], 1000);
    assert!(res.is_none());
}
