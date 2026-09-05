//! Integration tests for the pure embedding helpers in `embedding::text`.
//!
//! The inline `#[cfg(test)] mod tests` block in `text.rs` covers the same
//! surface; this file exists per `docs/CLAUDE.md` §Testing ("Avoid large
//! inline unit tests in library source files; move them into standalone
//! integration test files") and to assert the helpers are reachable via the
//! public `bango_lib::embedding` API surface.

use bango_lib::embedding::{
    cosine_similarity, expected_rows, format_embedding_text, hash_text, ChunkInput,
    TITLE_ABSTRACT_CHUNK_INDEX,
};

#[test]
fn format_embedding_combines_title_and_abstract() {
    assert_eq!(format_embedding_text("T", "A", None), "T\n\nA");
}

#[test]
fn hash_text_is_stable_sha256_hex() {
    let h = hash_text("bango");
    assert_eq!(h.len(), 64);
    // Determinism across calls.
    assert_eq!(h, hash_text("bango"));
}

#[test]
fn cosine_similarity_identical_is_one() {
    let v = vec![0.2, 0.4, 0.6];
    assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
}

#[test]
fn expected_rows_produces_title_abstract_plus_chunks() {
    let chunks = vec![
        ChunkInput { chunk_index: 0, body: "Methods".to_string() },
        ChunkInput { chunk_index: 1, body: "Results".to_string() },
    ];
    let rows = expected_rows("Title", "Abstract", &chunks, true);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, TITLE_ABSTRACT_CHUNK_INDEX);
    assert_eq!(rows[1].0, 0);
    assert_eq!(rows[2].0, 1);
}

#[test]
fn expected_rows_abstract_only_when_no_full_text() {
    let rows = expected_rows("Title", "Abstract", &[], false);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, TITLE_ABSTRACT_CHUNK_INDEX);
}
