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
