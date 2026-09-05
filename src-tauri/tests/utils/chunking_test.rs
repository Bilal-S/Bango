//! Integration tests for `utils::chunking` Tier 2 Phase 1 invariants.
//!
//! These cover the property-based invariants listed in the chunking plan §5.0
//! Mechanism D:
//! - Every emitted `Chunk.word_count <= MAX_CHUNK_WORDS` unless the source
//!   section is `Table`/`Figure` (atomic exception).
//! - `chunk_index` values are 0..n contiguous for any input.
//!
//! Plus the example-based atomic-Table/Figure tests that also live inline in
//! `chunking.rs`; the standalone copies here let the proptest module share
//! helpers without bloating the source file.

use bango_lib::utils::{
    chunking::{chunk_sections, DEFAULT_CHUNK_WORDS, MAX_CHUNK_WORDS},
    sections::{Section, SectionKind},
};
use proptest::prelude::*;

/// Build a `Section` with a body of `n` whitespace-separated words.
fn word_section(kind: SectionKind, heading: Option<&str>, n: usize) -> Section {
    let body: String = (0..n).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
    let word_count = body.split_whitespace().count();
    Section { kind, heading: heading.map(str::to_string), body, word_count }
}

// ── Atomic Table/Figure (mirrors the inline chunking.rs tests) ───────────────

#[test]
fn chunk_sections_table_is_atomic() {
    // A Table section larger than MAX_CHUNK_WORDS must emit exactly 1 chunk.
    let body = format!(
        "| col | val |\n| --- | --- |\n{}",
        (0..2000).map(|i| format!("| r{i} | v{i} |")).collect::<Vec<_>>().join("\n")
    );
    let s = Section {
        kind: SectionKind::Table,
        heading: Some("Table 1".to_string()),
        body,
        word_count: 0,
    };
    let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
    assert_eq!(chunks.len(), 1, "table must be one chunk");
    assert!(chunks[0].word_count > MAX_CHUNK_WORDS, "table chunk should exceed MAX");
}

#[test]
fn chunk_sections_figure_is_atomic() {
    let body = (0..2000).map(|i| format!("caption{i}")).collect::<Vec<_>>().join(" ");
    let s = Section {
        kind: SectionKind::Figure,
        heading: Some("Figure 1".to_string()),
        body,
        word_count: 0,
    };
    let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
    assert_eq!(chunks.len(), 1, "figure must be one chunk");
    assert!(chunks[0].word_count > MAX_CHUNK_WORDS, "figure chunk should exceed MAX");
}

#[test]
fn chunk_sections_table_carries_section_label() {
    let body = "| a | b |\n| --- | --- |\n| c | d |";
    let s = Section {
        kind: SectionKind::Table,
        heading: Some("Table 1".to_string()),
        body: body.to_string(),
        word_count: 0,
    };
    let chunks = chunk_sections(&[s], DEFAULT_CHUNK_WORDS);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].section.as_deref(), Some("Table"));
}

// ── Property-based invariants (§5.0 Mechanism D) ────────────────────────────

/// Generate a `Vec<Section>` with 0-6 sections of random kind and 0-1500 words.
/// `Table`/`Figure` are excluded from the word-count-bound property (they are
/// intentionally atomic and may exceed MAX_CHUNK_WORDS).
fn arb_sections_non_atomic() -> impl Strategy<Value = Vec<Section>> {
    let kinds = prop::sample::select(vec![
        SectionKind::Text,
        SectionKind::Heading,
        SectionKind::Methods,
        SectionKind::Results,
        SectionKind::Discussion,
        SectionKind::Introduction,
        SectionKind::Abstract,
        SectionKind::Conclusion,
        SectionKind::References,
    ]);
    prop::collection::vec((kinds, 0usize..1500), 0..6).prop_map(|entries| {
        entries.into_iter().map(|(kind, n)| word_section(kind, Some("heading"), n)).collect()
    })
}

proptest! {
    /// For any non-atomic section list, every emitted chunk respects MAX_CHUNK_WORDS.
    #[test]
    fn proptest_chunk_word_count_within_bounds(sections in arb_sections_non_atomic()) {
        let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
        for c in &chunks {
            prop_assert!(
                c.word_count <= MAX_CHUNK_WORDS,
                "chunk word_count {} exceeds MAX {}",
                c.word_count,
                MAX_CHUNK_WORDS
            );
        }
    }

    /// For any input, the emitted chunk_index values are 0..n contiguous.
    #[test]
    fn proptest_chunk_index_contiguous(sections in arb_sections_non_atomic()) {
        let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
        for (i, c) in chunks.iter().enumerate() {
            prop_assert_eq!(c.chunk_index, i, "chunk_index must be contiguous");
        }
    }
}
