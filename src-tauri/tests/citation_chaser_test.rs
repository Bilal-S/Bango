//! Integration test for the Citation Chaser scraper.
//!
//! These tests require:
//!   - Chromium or Google Chrome installed on the system
//!   - Network access to https://estech.shinyapps.io/citationchaser/
//!
//! Run with: `cargo test --test citation_chaser_test -- --ignored`
//!
//! ## Pure unit tests
//!
//! The pure helpers (`detect_empty_or_disconnect`, `validate_ris_nonempty`,
//! `CancelToken`, `sleep_or_cancel`, `clean_doi_filename`,
//! `ScrapeKind::*_id`) have inline `#[cfg(test)] mod tests` in
//! `src/scraping/citation_chaser.rs` and run under the default `cargo test`
//! (no browser/network needed). This file holds the live integration tests +
//! the `ScrapeError::Validation` pure check.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use bango_lib::scraping::citation_chaser::{
    clean_doi_filename, scrape_citation_chaser, CancelToken, ScrapeError, ScrapeOptions,
};

/// Helper to create a temporary output directory.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bango_scrape_test").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create temp dir");
    dir
}

/// A well-known DOI that should have both references and citations.
/// "Masic D, et al. A systematic review and evidence-based recommendations
///  for the use of platelet-rich plasma in dermatology."
const TEST_DOI: &str = "10.1016/j.jaad.2023.01.013";

/// A DOI with **zero references** in Lens.org. Validated live (see
/// `.worktrees/scrapefix2.md` §2.2): the References tab shows
/// "Your input article(s) had 0 references ..." before Search, and clicking
/// Search disconnects the Shiny session. The scrape must return `NoData`
/// promptly (well under the 120s `ELEMENT_TIMEOUT_SECS`) instead of hanging.
const ZERO_REFS_DOI: &str = "10.1504/EJIM.2025.10073474";

/// A DOI with **zero citations** in Lens.org. Validated live (see
/// `.worktrees/scrapefix2.md` §2.3): the Citations tab renders an `Error`
/// table header + "no recorded citations in the Lens.org" row, but `#cits_ris`
/// becomes enabled with a valid href that serves a **0-byte** RIS. The scrape
/// must return `NoData` (either via the body-text detector or the
/// `validate_ris_nonempty` post-download guard).
const ZERO_CITS_DOI: &str = "10.1002/csr.70574";

// ---------------------------------------------------------------------------
// Pure check (runs under default `cargo test`)
// ---------------------------------------------------------------------------

#[test]
fn test_validation_error_when_both_false() {
    let output_dir = temp_dir("validation_test");
    let options = ScrapeOptions { get_citations: false, get_references: false };
    let cancel = CancelToken::new();

    let result = scrape_citation_chaser("10.1234/anything", &output_dir, &options, &cancel);

    assert!(result.is_err(), "Should return an error");
    let err = result.expect_err("Expected error");
    assert!(
        matches!(err, ScrapeError::Validation(_)),
        "Should be a Validation error, got: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Live integration tests (#[ignore]; require Chrome + network)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_scrape_references_only() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_references_only");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("references_only");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions { get_citations: false, get_references: true };
    let cancel = CancelToken::new();

    let result =
        scrape_citation_chaser(TEST_DOI, &output_dir, &options, &cancel).expect("Scrape failed");

    assert!(result.references_ris.is_some(), "Should have a references RIS file");
    assert!(result.citations_ris.is_none(), "Should NOT have a citations RIS file");

    let ris_path = result.references_ris.expect("references_ris missing");
    assert!(ris_path.exists(), "RIS file should exist on disk");

    // Verify DOI-based filename
    let expected_name = format!("{}_references.ris", clean_doi_filename(TEST_DOI));
    assert_eq!(
        ris_path.file_name().unwrap().to_string_lossy(),
        expected_name,
        "References RIS filename should be DOI-based"
    );

    let contents = fs::read_to_string(&ris_path).expect("Failed to read RIS file");
    assert!(
        contents.contains("TY  -"),
        "RIS file should start with a TY tag, got: {}",
        &contents[..contents.len().min(200)]
    );
    eprintln!("   ✅ References RIS saved to: {}", ris_path.display());
}

#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_scrape_citations_only() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_citations_only");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("citations_only");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions { get_citations: true, get_references: false };
    let cancel = CancelToken::new();

    let result =
        scrape_citation_chaser(TEST_DOI, &output_dir, &options, &cancel).expect("Scrape failed");

    assert!(result.references_ris.is_none(), "Should NOT have a references RIS file");
    assert!(result.citations_ris.is_some(), "Should have a citations RIS file");

    let ris_path = result.citations_ris.expect("citations_ris missing");
    assert!(ris_path.exists(), "RIS file should exist on disk");

    // Verify DOI-based filename
    let expected_name = format!("{}_citations.ris", clean_doi_filename(TEST_DOI));
    assert_eq!(
        ris_path.file_name().unwrap().to_string_lossy(),
        expected_name,
        "Citations RIS filename should be DOI-based"
    );

    let contents = fs::read_to_string(&ris_path).expect("Failed to read RIS file");
    assert!(contents.contains("TY  -"), "RIS file should contain TY tags");
    eprintln!("   ✅ Citations RIS saved to: {}", ris_path.display());
}

#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_scrape_both_references_and_citations() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_both_references_and_citations");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("both");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions::default(); // both true
    let cancel = CancelToken::new();

    let result =
        scrape_citation_chaser(TEST_DOI, &output_dir, &options, &cancel).expect("Scrape failed");

    assert!(result.references_ris.is_some(), "Should have a references RIS file");
    assert!(result.citations_ris.is_some(), "Should have a citations RIS file");

    // The two files should be different.
    let ref_path = result.references_ris.expect("references_ris missing");
    let cit_path = result.citations_ris.expect("citations_ris missing");
    assert_ne!(ref_path, cit_path, "References and citations files should differ");

    // Verify DOI-based filenames
    let expected_ref_name = format!("{}_references.ris", clean_doi_filename(TEST_DOI));
    let expected_cit_name = format!("{}_citations.ris", clean_doi_filename(TEST_DOI));
    assert_eq!(
        ref_path.file_name().unwrap().to_string_lossy(),
        expected_ref_name,
        "References filename"
    );
    assert_eq!(
        cit_path.file_name().unwrap().to_string_lossy(),
        expected_cit_name,
        "Citations filename"
    );

    eprintln!("   ✅ References RIS: {}", ref_path.display());
    eprintln!("   ✅ Citations RIS:  {}", cit_path.display());
}

/// Zero-references DOI must return `NoData` promptly (not the 120s timeout).
///
/// Before the fix, this DOI caused `wait_for_download_enabled("refs_ris")` to
/// poll for 120s and return `ElementNotFound("... did not become enabled
/// within 120s")`. With the post-Search empty-result detector, the scrape
/// returns `NoData` within `EMPTY_RESULT_TIMEOUT_SECS = 20s` plus a small
/// margin for the navigate + tab-click steps.
#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_zero_references_returns_no_data_promptly() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_zero_references_returns_no_data_promptly");
    eprintln!("   DOI: {ZERO_REFS_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("zero_refs");
    let options = ScrapeOptions { get_citations: false, get_references: true };
    let cancel = CancelToken::new();

    let start = Instant::now();
    let result = scrape_citation_chaser(ZERO_REFS_DOI, &output_dir, &options, &cancel);
    let elapsed = start.elapsed();

    let err = result.expect_err("Zero-refs DOI should return an error, not Ok");
    assert!(matches!(err, ScrapeError::NoData(_)), "Should be NoData, got: {err:?}");

    // Generous bound: navigate (~5s) + tab click + human delay (~1.5s) +
    // empty-result poll (up to 20s). Must be well under the old 120s timeout.
    assert!(
        elapsed.as_secs() < 60,
        "Zero-refs scrape should return within 60s, took {}s",
        elapsed.as_secs()
    );
    eprintln!("   ✅ Returned NoData in {}s", elapsed.as_secs());

    // No partial RIS file should be left on disk.
    let expected_ref =
        output_dir.join(format!("{}_references.ris", clean_doi_filename(ZERO_REFS_DOI)));
    assert!(
        !expected_ref.exists(),
        "No partial references RIS should be left for a NoData outcome"
    );
}

/// Zero-citations DOI must return `NoData` (either via the body-text detector
/// or the `validate_ris_nonempty` post-download guard that catches the 0-byte
/// file), and must NOT leave a 0-byte RIS cached on disk.
#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_zero_citations_returns_no_data_no_cached_file() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_zero_citations_returns_no_data_no_cached_file");
    eprintln!("   DOI: {ZERO_CITS_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("zero_cits");
    let options = ScrapeOptions { get_citations: true, get_references: false };
    let cancel = CancelToken::new();

    let result = scrape_citation_chaser(ZERO_CITS_DOI, &output_dir, &options, &cancel);

    let err = result.expect_err("Zero-cits DOI should return an error, not Ok");
    assert!(matches!(err, ScrapeError::NoData(_)), "Should be NoData, got: {err:?}");

    // No 0-byte RIS file should be cached (the validate_ris_nonempty guard
    // deletes it; the existence-shortcut would otherwise cache it forever).
    let expected_cit =
        output_dir.join(format!("{}_citations.ris", clean_doi_filename(ZERO_CITS_DOI)));
    assert!(
        !expected_cit.exists(),
        "No 0-byte citations RIS should be cached for a NoData outcome"
    );
    eprintln!("   ✅ Returned NoData; no cached 0-byte file");
}

/// Cancellation: signalling the token mid-scrape must return `Cancelled`
/// promptly (within ~2s, one `POLL_INTERVAL_MS` tick) and leave no partial RIS.
///
/// Uses the well-populated DOI so the scrape reaches the polling phase (rather
/// than short-circuiting on NoData). The token is cancelled after a short
/// grace period to let the navigate + tab-click steps land.
#[test]
#[ignore = "live: requires Chrome + network + shinyapps.io"]
fn test_cancel_returns_cancelled_promptly() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_cancel_returns_cancelled_promptly");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("cancel");
    let options = ScrapeOptions { get_citations: false, get_references: true };
    let cancel = CancelToken::new();

    // Spawn a thread that cancels after a short grace period (enough for
    // navigate + tab click to land, so we exercise a poll loop).
    let cancel_clone = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        eprintln!("   ⚡ Cancelling scrape token...");
        cancel_clone.cancel();
    });

    let start = Instant::now();
    let result = scrape_citation_chaser(TEST_DOI, &output_dir, &options, &cancel);
    let elapsed = start.elapsed();

    let err = result.expect_err("Cancelled scrape should return an error, not Ok");
    assert!(matches!(err, ScrapeError::Cancelled), "Should be Cancelled, got: {err:?}");
    // 8s grace + up to 1s poll tick + slack.
    assert!(
        elapsed.as_secs() <= 15,
        "Cancel should take effect within ~15s, took {}s",
        elapsed.as_secs()
    );
    eprintln!("   ✅ Returned Cancelled in {}s", elapsed.as_secs());

    // No partial RIS file should be left on disk.
    let expected_ref = output_dir.join(format!("{}_references.ris", clean_doi_filename(TEST_DOI)));
    assert!(!expected_ref.exists(), "No partial references RIS should be left after cancel");
}
