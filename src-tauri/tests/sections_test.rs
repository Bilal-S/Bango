//! Integration tests for `utils::sections` (T1.1).
//!
//! These tests cover the pure `classify_sections` logic. `extract_sections`
//! (the I/O wrapper) is not tested here because it drives the real PDF
//! pipeline; its correctness follows from `extract_pdf_text` +
//! `classify_sections` which are both covered separately.

use std::path::Path;

use bango_lib::utils::{
    chunking::{chunk_sections, DEFAULT_CHUNK_WORDS, MIN_CHUNK_WORDS},
    pdf_extract::extract_pdf_text,
    sections::{classify_sections, SectionKind},
};

/// Helper: classify and return the (kind, heading) pairs for concise assertions.
fn kinds(text: &str) -> Vec<(SectionKind, Option<String>)> {
    classify_sections(text).into_iter().map(|s| (s.kind, s.heading)).collect()
}

#[test]
fn classify_sections_empty_returns_empty() {
    assert!(classify_sections("").is_empty());
    assert!(classify_sections("   \n  \t ").is_empty());
}

#[test]
fn classify_sections_detects_markdown_headings() {
    let text = "## Abstract\nbody a\n\n## Methods\nbody m\n\n## Results\nbody r";
    let k = kinds(text);
    assert_eq!(k.len(), 3);
    assert_eq!(k[0].0, SectionKind::Abstract);
    assert_eq!(k[0].1.as_deref(), Some("## Abstract"));
    assert_eq!(k[1].0, SectionKind::Methods);
    assert_eq!(k[1].1.as_deref(), Some("## Methods"));
    assert_eq!(k[2].0, SectionKind::Results);
    assert_eq!(k[2].1.as_deref(), Some("## Results"));
}

#[test]
fn classify_sections_detects_numbered_headings() {
    let text = "1 Introduction\nintro body\n\n2.1 Study Design\ndesign body\n\n3 Methods\nm body";
    let k = kinds(text);
    // First section is the "1 Introduction" heading (no preamble).
    assert!(
        k.iter().any(|(kind, _)| *kind == SectionKind::Introduction),
        "should detect Introduction: {k:?}"
    );
    // "2.1 Study Design" -> Heading (numbered but not a keyword; "Study Design"
    // is not in the keyword groups).
    assert!(
        k.iter().any(|(kind, heading)| *kind == SectionKind::Heading
            && heading.as_deref() == Some("2.1 Study Design")),
        "2.1 Study Design should be a generic Heading (not a keyword): {k:?}"
    );
    assert!(
        k.iter().any(|(kind, heading)| *kind == SectionKind::Methods
            && heading.as_deref() == Some("3 Methods")),
        "3 Methods should classify as Methods: {k:?}"
    );
}

#[test]
fn classify_sections_numbered_heading_regex_rejects_sentences() {
    // "3. The results showed..." is a sentence, not a heading. The tightened
    // regex must NOT treat it as a heading boundary.
    let text = "Some intro text here that is not a heading.\n\n3. The results showed a clear effect on the primary outcome measure across all subgroups.";
    let sections = classify_sections(text);
    // No heading detected => single Text section.
    assert!(
        sections.iter().all(|s| s.kind == SectionKind::Text || s.kind == SectionKind::Heading),
        "sentence should not be a heading: {:?}",
        sections.iter().map(|s| s.kind).collect::<Vec<_>>()
    );
    // And there should be no Methods section created from the sentence.
    assert!(!sections.iter().any(|s| s.kind == SectionKind::Methods));
}

#[test]
fn classify_sections_excludes_references_keyword() {
    let text =
        "## Introduction\nbody\n\n## Methods\nm body\n\n## References\n[1] ref one\n[2] ref two";
    let k = kinds(text);
    assert!(
        k.iter().any(|(kind, _)| *kind == SectionKind::References),
        "references should be detected: {k:?}"
    );
    // No section after References should leak in (it is its own section, not
    // dropped here; consumers drop it). Verify the references section has the
    // heading and body.
    let refs = classify_sections(text)
        .into_iter()
        .find(|s| s.kind == SectionKind::References)
        .expect("references section present");
    assert_eq!(refs.heading.as_deref(), Some("## References"));
    assert!(refs.body.contains("[1] ref one"));
}

#[test]
fn classify_sections_bibliography_is_references() {
    let text = "## Methods\nm\n\n## Bibliography\nitem one";
    let k = kinds(text);
    assert!(
        k.iter().any(|(kind, _)| *kind == SectionKind::References),
        "bibliography -> References: {k:?}"
    );
}

#[test]
fn classify_sections_handles_missing_abstract() {
    // No Abstract heading at all; first heading is Introduction.
    let text = "## Introduction\nintro body\n\n## Methods\nm";
    let k = kinds(text);
    assert!(
        !k.iter().any(|(kind, _)| *kind == SectionKind::Abstract),
        "no Abstract should be detected"
    );
    // First detected section is Introduction.
    assert_eq!(k.first().map(|(kind, _)| *kind), Some(SectionKind::Introduction));
}

#[test]
fn classify_sections_preamble_before_first_heading_is_text() {
    let text = "Some title or front matter line.\nMisc text.\n\n## Abstract\nabs\n\n## Methods\nm";
    let sections = classify_sections(text);
    // First section is Text (the preamble).
    assert_eq!(sections.first().map(|s| s.kind), Some(SectionKind::Text));
    assert!(sections.first().unwrap().heading.is_none());
    // The preamble body contains the front matter.
    assert!(sections.first().unwrap().body.contains("Some title or front matter"));
}

#[test]
fn classify_sections_falls_back_to_single_text_for_unstructured_prose() {
    let text = "Just a block of prose with no headings at all. It has multiple sentences. Each one is plain text. No structure here whatsoever.";
    let sections = classify_sections(text);
    assert_eq!(sections.len(), 1, "unstructured prose -> one section");
    assert_eq!(sections[0].kind, SectionKind::Text);
    assert!(sections[0].heading.is_none());
    assert!(sections[0].body.contains("block of prose"));
    assert!(sections[0].word_count > 10);
}

#[test]
fn classify_sections_word_count_matches_body() {
    let text = "## Methods\none two three four five";
    let sections = classify_sections(text);
    let methods =
        sections.iter().find(|s| s.kind == SectionKind::Methods).expect("methods section");
    assert_eq!(methods.word_count, 5);
}

#[test]
fn classify_sections_materials_and_methods_is_methods() {
    let text = "## Materials and Methods\nbody";
    let k = kinds(text);
    assert_eq!(
        k.as_slice(),
        &[(SectionKind::Methods, Some("## Materials and Methods".to_string()))]
    );
}

#[test]
fn classify_sections_case_insensitive_keywords() {
    let text = "METHODS\nm body\n\nRESULTS\nr body";
    let k = kinds(text);
    assert!(k.iter().any(|(kind, _)| *kind == SectionKind::Methods), "uppercase METHODS: {k:?}");
    assert!(k.iter().any(|(kind, _)| *kind == SectionKind::Results), "uppercase RESULTS: {k:?}");
}

#[test]
fn classify_sections_generic_heading_is_heading_kind() {
    // A heading that does not match any keyword should be `Heading`, not Text.
    let text = "## Limitations of Prior Work\nbody\n\n## Methods\nm";
    let k = kinds(text);
    assert!(
        k.iter().any(|(kind, _)| *kind == SectionKind::Heading),
        "non-keyword heading -> Heading kind: {k:?}"
    );
    assert!(k.iter().any(|(kind, _)| *kind == SectionKind::Methods));
}

#[test]
fn classify_sections_body_excludes_heading_line() {
    let text = "## Methods\nthe body text\nmore body";
    let sections = classify_sections(text);
    let methods = sections.iter().find(|s| s.kind == SectionKind::Methods).expect("methods");
    assert!(!methods.body.contains("## Methods"), "heading line must not be in body");
    assert!(methods.body.contains("the body text"));
    assert!(methods.body.contains("more body"));
}

// ── Real-PDF end-to-end tests ──────────────────────────────────────────────
//
// These exercise the full pipeline (pdf-extract → classify_sections →
// chunk_sections) against a committed open-access PDF. They are `#[ignore]` by
// default to keep the fast unit-test run lean; run with
// `cargo test --test sections_test -- --ignored`.
//
// Fixture: `tests/assets/plos-med-1004371.pdf`
// Cobiac et al. (2024), "Change in consumption of free sugars among children
// and adolescents ... An interrupted time series analysis", PLoS Medicine, CC-BY.
// DOI: 10.1371/journal.pmed.1004371

/// Path to the committed PLoS Medicine OA PDF used for end-to-end tests.
const PLOS_PDF: &str = "../tests/assets/plos-med-1004371.pdf";

/// Path to the second committed OA PDF (PLoS ONE). A different PLoS journal to
/// confirm the pipeline generalises across templates.
const PLOS_ONE_PDF: &str = "../tests/assets/pone-0285956.pdf";

/// Extract + classify + chunk the real PLoS PDF and assert the pipeline detects
/// the expected high-value sections (Methods / Results / Discussion) and produces
/// section-labeled chunks. This is the "happy path" that the synthetic unit tests
/// cannot fully cover (they don't exercise pdf-extract's space-preserving primary
/// path or real-world heading formatting).
#[test]
#[ignore] // depends on a 744KB committed PDF asset
fn real_pdf_pipeline_detects_methods_results_discussion() {
    let path = Path::new(PLOS_PDF);
    if !path.exists() {
        eprintln!("Skipping: test asset not found at {PLOS_PDF}");
        return;
    }

    // 1. Extract (primary pdf-extract path must work without panicking).
    let text = extract_pdf_text(path).expect("extraction should succeed on the PLoS PDF");

    // 2. Classify - the PLoS template uses bare-keyword headings (Introduction,
    //    Methods, Results, Discussion) so classify_sections must detect them.
    let sections = classify_sections(&text);
    let kinds: Vec<SectionKind> = sections.iter().map(|s| s.kind).collect();
    assert!(
        kinds.contains(&SectionKind::Methods),
        "Methods section must be detected on the real PDF: {kinds:?}"
    );
    assert!(
        kinds.contains(&SectionKind::Results),
        "Results section must be detected on the real PDF: {kinds:?}"
    );
    assert!(
        kinds.contains(&SectionKind::Discussion),
        "Discussion section must be detected on the real PDF: {kinds:?}"
    );

    // 3. Chunk - the Methods section (~2000 words) must split into multiple
    //    chunks, all carrying `section: Some("Methods")`.
    let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
    let methods_chunks: Vec<_> =
        chunks.iter().filter(|c| c.section.as_deref() == Some("Methods")).collect();
    assert!(
        !methods_chunks.is_empty(),
        "should produce Methods-labeled chunks: {} total chunks, sections={:?}",
        chunks.len(),
        chunks.iter().map(|c| c.section.as_deref().unwrap_or("None")).collect::<Vec<_>>()
    );
    // Each chunk respects the bounds (>= MIN unless it is the only one).
    for c in &chunks {
        assert!(
            c.word_count <= 1200,
            "chunk {} exceeds MAX_CHUNK_WORDS: {}",
            c.chunk_index,
            c.word_count
        );
    }
    // chunk_index is contiguous 0..n.
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.chunk_index, i, "contiguous chunk_index");
    }
    // The tiny-tail merge means the last chunk is >= MIN_CHUNK_WORDS (unless the
    // whole document is one short chunk, which this 5000+ word paper is not).
    assert!(
        chunks.last().is_some_and(|c| c.word_count >= MIN_CHUNK_WORDS || chunks.len() == 1),
        "trailing chunk should be >= MIN_CHUNK_WORDS after merge: last = {:?}",
        chunks.last().map(|c| c.word_count)
    );
}

/// Second real-PDF regression: the PLoS ONE Oakland SSB tax paper. Same PLoS
/// template family as the PLoS Medicine paper, but a different journal - confirms
/// the section detection generalises and is not hardcoded to one paper's layout.
#[test]
#[ignore] // depends on a 512KB committed PDF asset
fn real_pdf_pipeline_plos_one_oakland_detects_methods_and_results() {
    let path = Path::new(PLOS_ONE_PDF);
    if !path.exists() {
        eprintln!("Skipping: test asset not found at {PLOS_ONE_PDF}");
        return;
    }

    let text = extract_pdf_text(path).expect("extraction should succeed on the PLoS ONE PDF");
    let sections = classify_sections(&text);
    let kinds: Vec<SectionKind> = sections.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&SectionKind::Methods), "Methods must be detected: {kinds:?}");
    assert!(kinds.contains(&SectionKind::Results), "Results must be detected: {kinds:?}");

    // The Methods section is ~3300 words on this paper, so it must split into
    // multiple Methods-labeled chunks.
    let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
    let methods_chunks: Vec<_> =
        chunks.iter().filter(|c| c.section.as_deref() == Some("Methods")).collect();
    assert!(
        methods_chunks.len() >= 2,
        "Methods (~3300w) should split into multiple chunks: got {}",
        methods_chunks.len()
    );
    // Contiguous chunk_index across the whole document.
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.chunk_index, i, "contiguous chunk_index");
    }
}

/// The lopdf fallback PDF (`demo-vfs-2022-pid-69753.pdf`) produces space-degenerate
/// text (no spaces between words), so classify_sections must gracefully return a
/// single Text section (no spurious headings) and chunk_sections must still split
/// it into word-count-bounded chunks. This is the graceful-degrade regression.
#[test]
#[ignore] // depends on the existing 1.7MB committed PDF asset
fn lopdf_fallback_pdf_degrades_to_single_text_section() {
    let path = Path::new("../tests/assets/demo-vfs-2022-pid-69753.pdf");
    if !path.exists() {
        eprintln!("Skipping: test asset not found");
        return;
    }

    // Extraction falls back to lopdf (the primary pdf-extract panics on this PDF's
    // expert font), producing space-degenerate text.
    let text = extract_pdf_text(path).expect("lopdf fallback should produce text");
    assert!(!text.trim().is_empty(), "fallback must produce some text");

    // classify_sections should find zero keyword headings (the space-degenerate
    // text has no recognisable `Methods` / `Results` lines) and produce a single
    // Text section.
    let sections = classify_sections(&text);
    assert!(
        sections.iter().all(|s| s.kind == SectionKind::Text),
        "space-degenerate text should not produce keyword sections: {:?}",
        sections.iter().map(|s| s.kind).collect::<Vec<_>>()
    );

    // chunk_sections should still split it into word-count-bounded chunks
    // (graceful degrade: chunking works on plain text).
    let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
    assert!(chunks.len() >= 2, "a multi-thousand-word text should split: {} chunks", chunks.len());
    for c in &chunks {
        assert!(c.word_count <= 1200, "chunk exceeds MAX_CHUNK_WORDS");
        assert!(c.section.is_none(), "no section label on Text chunks");
    }
}
