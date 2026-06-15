//! Regression test for PDF extraction crash.
//!
//! `pdf-extract` 0.7.x panicked with `FromUtf16Error` on PDFs containing
//! Adobe Expert-encoded Type1 fonts (e.g. glyph names like 'C12').
//! The panic was unwinding through a non-unwinding WebKit callback, causing
//! the entire app to abort.
//!
//! Run with: `cargo test --test pdf_extract_test -- --ignored`

use std::path::Path;

/// The problematic PDF that triggered the `FromUtf16Error` panic in pdf-extract 0.7.
/// It contains a Type1 font (DIKLOL+AdvP4C4E51) with an Expert encoding whose
/// Unicode map only maps chars 34 and 59, causing the decoder to panic.
const PROBLEMATIC_PDF: &str = "../tests/assets/demo-vfs-2022-pid-69753.pdf";

#[test]
#[ignore] // slow; uses a 1.7 MB PDF asset
fn extract_pdf_text_does_not_panic_on_expert_encoded_fonts() {
    let path = Path::new(PROBLEMATIC_PDF);
    if !path.exists() {
        eprintln!("Skipping: test asset not found at {PROBLEMATIC_PDF}");
        return;
    }

    // The key assertion: this must not panic.
    // With pdf-extract 0.7 this caused: `unwrap() on Err value: FromUtf16Error`
    // The catch_unwind wrapper + lopdf fallback should now handle it gracefully.
    let result = bango_lib::utils::pdf_extract::extract_pdf_text(path);

    // We expect either:
    //  - Ok(text) with non-empty content (pdf-extract 0.10 may handle it), or
    //  - Ok(text) with non-empty content via lopdf fallback, or
    //  - Err(...) — acceptable if both extractors fail (but must not panic!)
    match &result {
        Ok(text) => {
            assert!(
                !text.trim().is_empty(),
                "Extracted text should not be empty — lopdf fallback should produce some content"
            );
        }
        Err(e) => {
            // Not ideal but acceptable — the important thing is no panic
            eprintln!("Warning: PDF extraction returned error (but did not panic): {e}");
        }
    }
}

// ── Unit tests extracted from inline `#[cfg(test)] mod tests` ──
// These cover the text-processing helpers (no PDF asset required).

use bango_lib::utils::pdf_extract::{
    extract_txt_text, is_page_number, normalize_line, remove_header_footer_lines, strip_abstract,
    strip_references, truncate_to_word_limit,
};

#[test]
fn test_strip_references() {
    let text = "Introduction\nSome text here.\nMore content.\nAdditional body paragraph.\nFurther discussion.\n\nReferences\n1. Smith et al.\n2. Jones et al.";
    let result = strip_references(text);
    assert!(!result.contains("References"));
    assert!(!result.contains("Smith"));
    assert!(result.contains("Introduction"));
}

#[test]
fn test_strip_abstract() {
    let text =
        "Title\n\nAbstract\nThis is the abstract text.\n\n1. Introduction\nThis is the intro.";
    let result = strip_abstract(text);
    assert!(!result.contains("abstract text"));
    assert!(result.contains("Introduction"));
    assert!(result.contains("intro"));
}

#[test]
fn test_truncate_to_word_limit() {
    let text = "one two three four five six";
    let result = truncate_to_word_limit(text, 4);
    assert_eq!(result, "one two three four");
}

#[test]
fn test_is_page_number() {
    assert!(is_page_number("3"));
    assert!(is_page_number("page 42"));
    assert!(is_page_number("- 7 -"));
    assert!(!is_page_number("Introduction"));
}

#[test]
fn test_normalize_line() {
    assert_eq!(normalize_line("  Hello   World  "), "hello world");
}

#[test]
fn test_remove_header_footer_lines() {
    let text =
        "Journal of Something\nIntroduction\nSome content\n3\nConclusion\nJournal of Something";
    let hfs = vec!["journal of something".to_string(), "__PAGE_NUMBER__".to_string()];
    let result = remove_header_footer_lines(text, &hfs);
    assert!(!result.contains("Journal of Something"));
    assert!(result.contains("Introduction"));
    assert!(!result.contains("\n3\n"));
}

#[test]
fn test_extract_txt_text() {
    let content =
        "Abstract\nThis is abstract.\n\nIntroduction\nBody text here.\n\nReferences\n[1] Author.";
    let result = extract_txt_text(content);
    assert!(result.contains("Body text"));
    assert!(!result.contains("References"));
}
