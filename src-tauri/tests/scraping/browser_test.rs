//! Integration test for `scraping::browser::detect_browser`.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/scraping/browser.rs` to keep the source file compact.

use bango_lib::scraping::browser::{detect_browser, BrowserError};

#[test]
fn test_detect_browser_returns_result() {
    // On CI or development machines, a browser should be available.
    // This test just verifies the function doesn't panic and returns a
    // proper Result.
    let result = detect_browser();
    assert!(result.is_ok() || matches!(result, Err(BrowserError::NotFound)));
}
