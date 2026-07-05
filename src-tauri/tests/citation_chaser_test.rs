//! Integration test for the Citation Chaser scraper.
//!
//! These tests require:
//!   - Chromium or Google Chrome installed on the system
//!   - Network access to https://estech.shinyapps.io/citationchaser/
//!
//! Run with: `cargo test --test citation_chaser_test -- --ignored`

use std::fs;
use std::path::PathBuf;

use bango_lib::scraping::citation_chaser::{
    clean_doi_filename, scrape_citation_chaser, ScrapeError, ScrapeOptions,
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

// ---------------------------------------------------------------------------
// NOTE: All tests are #[ignore] because they require a browser + network.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_scrape_references_only() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_references_only");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("references_only");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions { get_citations: false, get_references: true };

    let result = scrape_citation_chaser(TEST_DOI, &output_dir, &options).expect("Scrape failed");

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
#[ignore]
fn test_scrape_citations_only() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_citations_only");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("citations_only");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions { get_citations: true, get_references: false };

    let result = scrape_citation_chaser(TEST_DOI, &output_dir, &options).expect("Scrape failed");

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
#[ignore]
fn test_scrape_both_references_and_citations() {
    eprintln!("═══════════════════════════════════════════");
    eprintln!("🧪 TEST: test_scrape_both_references_and_citations");
    eprintln!("   DOI: {TEST_DOI}");
    eprintln!("═══════════════════════════════════════════");
    let output_dir = temp_dir("both");
    eprintln!("   Output dir: {}", output_dir.display());
    let options = ScrapeOptions::default(); // both true

    let result = scrape_citation_chaser(TEST_DOI, &output_dir, &options).expect("Scrape failed");

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

#[test]
fn test_validation_error_when_both_false() {
    let output_dir = temp_dir("validation_test");
    let options = ScrapeOptions { get_citations: false, get_references: false };

    let result = scrape_citation_chaser("10.1234/anything", &output_dir, &options);

    assert!(result.is_err(), "Should return an error");
    let err = result.expect_err("Expected error");
    assert!(
        matches!(err, ScrapeError::Validation(_)),
        "Should be a Validation error, got: {err:?}"
    );
}
