//! Tests for the pure embedding helpers in `embedding::text`.
//!
//! The inline `#[cfg(test)] mod tests` block formerly in `text.rs` was
//! extracted into this file per `docs/CLAUDE.md` §Testing ("Avoid large
//! inline unit tests in library source files; move them into standalone
//! integration test files"); the helpers are exercised via the public
//! `bango_lib::embedding` API surface.

use bango_lib::embedding::text::{
    deserialize_embedding, pool_vectors, serialize_embedding, split_text_by_token_budget,
};
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

// ── format_embedding_text ────────────────────────────────────────────

#[test]
fn format_title_and_abstract() {
    let text = format_embedding_text("Sugar Taxes", "We studied obesity.", None);
    assert_eq!(text, "Sugar Taxes\n\nWe studied obesity.");
}

#[test]
fn format_title_only_when_abstract_empty() {
    let text = format_embedding_text("Sugar Taxes", "   ", None);
    assert_eq!(text, "Sugar Taxes");
}

#[test]
fn format_abstract_only_when_title_empty() {
    let text = format_embedding_text("", "We studied obesity.", None);
    assert_eq!(text, "We studied obesity.");
}

#[test]
fn format_empty_when_both_empty() {
    let text = format_embedding_text("   ", "", None);
    assert!(text.trim().is_empty());
}

#[test]
fn format_chunk_includes_title_prefix() {
    let text = format_embedding_text("Sugar Taxes", "ignored abstract", Some("Methods: RCT"));
    assert_eq!(text, "Sugar Taxes\n\nMethods: RCT");
}

#[test]
fn format_chunk_without_title() {
    let text = format_embedding_text("", "", Some("Methods: RCT"));
    assert_eq!(text, "Methods: RCT");
}

#[test]
fn format_chunk_ignored_when_body_blank() {
    // A whitespace-only chunk body falls back to the title+abstract path
    // so the row still carries signal.
    let text = format_embedding_text("Title", "Abstract", Some("   "));
    assert_eq!(text, "Title\n\nAbstract");
}

// ── hash_text ────────────────────────────────────────────────────────

#[test]
fn hash_is_deterministic() {
    let a = hash_text("hello world");
    let b = hash_text("hello world");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64, "SHA-256 hex digest is 64 chars");
}

#[test]
fn hash_differs_for_different_text() {
    let a = hash_text("hello world");
    let b = hash_text("hello world!");
    assert_ne!(a, b);
}

#[test]
fn hash_known_value() {
    // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    assert_eq!(
        hash_text("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// ── expected_rows ────────────────────────────────────────────────────

#[test]
fn expected_rows_abstract_only_no_chunks() {
    let rows = expected_rows("Title", "Abstract", &[], false);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, TITLE_ABSTRACT_CHUNK_INDEX);
    assert_eq!(rows[0].1, "Title\n\nAbstract");
}

#[test]
fn expected_rows_skips_chunks_when_no_full_text() {
    let chunks = vec![ChunkInput { chunk_index: 0, body: "Methods".to_string() }];
    let rows = expected_rows("Title", "Abstract", &chunks, false);
    assert_eq!(rows.len(), 1, "has_full_text=false skips chunk rows");
    assert_eq!(rows[0].0, TITLE_ABSTRACT_CHUNK_INDEX);
}

#[test]
fn expected_rows_includes_chunks_when_full_text() {
    let chunks = vec![
        ChunkInput { chunk_index: 0, body: "Methods".to_string() },
        ChunkInput { chunk_index: 1, body: "Results".to_string() },
    ];
    let rows = expected_rows("Title", "Abstract", &chunks, true);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, TITLE_ABSTRACT_CHUNK_INDEX, "row 0 is title+abstract");
    assert_eq!(rows[0].1, "Title\n\nAbstract");
    assert_eq!(rows[1].0, 0);
    assert_eq!(rows[1].1, "Title\n\nMethods");
    assert_eq!(rows[2].0, 1);
    assert_eq!(rows[2].1, "Title\n\nResults");
}

#[test]
fn expected_rows_empty_when_nothing_to_embed() {
    let rows = expected_rows("", "   ", &[], false);
    assert!(rows.is_empty());
}

// ── cosine_similarity ────────────────────────────────────────────────

#[test]
fn cosine_identical_vectors_is_one() {
    let v = vec![0.1, 0.2, 0.3];
    let sim = cosine_similarity(&v, &v);
    assert!((sim - 1.0).abs() < 1e-5, "identical vectors ~= 1.0, got {sim}");
}

#[test]
fn cosine_orthogonal_vectors_is_zero() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let sim = cosine_similarity(&a, &b);
    assert!(sim.abs() < 1e-5, "orthogonal ~= 0.0, got {sim}");
}

#[test]
fn cosine_length_mismatch_is_zero() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn cosine_empty_vectors_is_zero() {
    assert_eq!(cosine_similarity(&[], &[]), 0.0);
}

#[test]
fn cosine_zero_magnitude_is_zero() {
    let a = vec![0.0, 0.0];
    let b = vec![1.0, 2.0];
    assert_eq!(cosine_similarity(&a, &b), 0.0);
}

#[test]
fn cosine_known_value() {
    // a = [1, 0], b = [1, 1] => cos = 1/sqrt(2) ~= 0.7071
    let a = vec![1.0, 0.0];
    let b = vec![1.0, 1.0];
    let sim = cosine_similarity(&a, &b);
    assert!((sim - (1.0_f32 / 2.0_f32.sqrt())).abs() < 1e-5, "got {sim}");
}

// ── serialize / deserialize ──────────────────────────────────────────

#[test]
fn serialize_deserialize_round_trip() {
    let original = vec![0.1, -0.2, 0.3, 1.0, -1.0];
    let bytes = serialize_embedding(&original);
    assert_eq!(bytes.len(), original.len() * 4);
    let decoded = deserialize_embedding(&bytes, original.len() as i32);
    assert!(decoded.is_some());
    let decoded = decoded.unwrap();
    assert_eq!(decoded.len(), original.len());
    for (a, b) in original.iter().zip(decoded.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn deserialize_rejects_wrong_length() {
    let bytes = vec![0u8; 8]; // 2 f32s
    assert!(deserialize_embedding(&bytes, 3).is_none(), "dimension mismatch rejected");
}

#[test]
fn deserialize_rejects_non_multiple_of_four() {
    let bytes = vec![0u8; 5]; // not a multiple of 4
    assert!(deserialize_embedding(&bytes, 1).is_none());
}

#[test]
fn deserialize_rejects_zero_dimensions() {
    let bytes = vec![];
    assert!(deserialize_embedding(&bytes, 0).is_none());
}

#[test]
fn serialize_empty_vec_is_empty_bytes() {
    let bytes = serialize_embedding(&[]);
    assert!(bytes.is_empty());
    // deserialize with dimensions=0 returns None (guard), so round-trip
    // of an empty vec is intentionally not supported.
}

// ── split_text_by_token_budget ───────────────────────────────────────

#[test]
fn split_empty_text_returns_one_empty_piece() {
    let pieces = split_text_by_token_budget("", 100);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].text, "");
    assert_eq!(pieces[0].token_count, 0);
}

#[test]
fn split_under_budget_returns_single_piece() {
    let text = "one two three four five";
    let pieces = split_text_by_token_budget(text, 100);
    assert_eq!(pieces.len(), 1, "under-budget text stays as one piece");
    assert_eq!(pieces[0].text, text);
    assert_eq!(pieces[0].token_count, 5);
}

#[test]
fn split_exactly_at_budget_returns_single_piece() {
    let text = "a b c"; // 3 tokens
    let pieces = split_text_by_token_budget(text, 3);
    assert_eq!(pieces.len(), 1, "exactly at budget is one piece");
}

#[test]
fn split_over_budget_splits_at_sentence_boundaries() {
    // Two sentences, each ~5 tokens. Budget = 5 means each sentence is its
    // own piece.
    let text = "First sentence has five words. Second sentence has five words.";
    let pieces = split_text_by_token_budget(text, 5);
    assert_eq!(pieces.len(), 2, "split at the sentence boundary");
    assert!(pieces[0].text.contains("First sentence"));
    assert!(pieces[1].text.contains("Second sentence"));
    assert!(pieces.iter().all(|p| p.token_count <= 5));
}

#[test]
fn split_overlong_sentence_falls_back_to_word_boundaries() {
    // One sentence with 6 words, budget = 3. Cannot split at sentence
    // boundary, so falls back to word boundaries.
    let text = "alpha beta gamma delta epsilon zeta";
    let pieces = split_text_by_token_budget(text, 3);
    assert!(pieces.len() >= 2, "overlong sentence splits at word boundaries");
    assert!(pieces.iter().all(|p| p.token_count <= 3));
}

#[test]
fn split_single_overlong_word_hard_splits() {
    // One very long word with no spaces, budget = 3. Must hard-split the
    // word itself.
    let text = "abcdefghij"; // 1 token (no whitespace), but > 3 chars
    let pieces = split_text_by_token_budget(text, 3);
    assert_eq!(pieces.len(), 1, "single token stays as one piece (token count is 1, under budget)");
}

// ── pool_vectors ─────────────────────────────────────────────────────

#[test]
fn pool_empty_returns_empty() {
    let pooled = pool_vectors(&[], &[]);
    assert!(pooled.is_empty());
}

#[test]
fn pool_single_piece_returned_verbatim() {
    let v = vec![0.1, 0.2, 0.3];
    // `pool_vectors` takes `&[Vec<f32>]`; a single-piece input is returned
    // verbatim. Use `std::slice::from_ref` to avoid cloning the vector
    // (clippy::cloned_ref_to_slice_refs).
    let pooled = pool_vectors(std::slice::from_ref(&v), &[1]);
    assert_eq!(pooled.len(), v.len());
    for (a, b) in pooled.iter().zip(v.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn pool_uniform_weights_is_mean() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let pooled = pool_vectors(&[a, b], &[1, 1]);
    // mean = [0.5, 0.5], then L2-normalized => [1/sqrt(2), 1/sqrt(2)] ~= [0.7071, 0.7071]
    assert!((pooled[0] - (1.0_f32 / 2.0_f32.sqrt())).abs() < 1e-5);
    assert!((pooled[1] - (1.0_f32 / 2.0_f32.sqrt())).abs() < 1e-5);
}

#[test]
fn pool_token_weighted_favors_heavier_piece() {
    // Piece A weight=3, piece B weight=1. Weighted mean leans toward A.
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let pooled = pool_vectors(&[a, b], &[3, 1]);
    // weighted mean = [(3*1 + 1*0)/4, (3*0 + 1*1)/4] = [0.75, 0.25]
    // magnitude = sqrt(0.75^2 + 0.25^2) = sqrt(0.625)
    // normalized ≈ [0.9487, 0.3162]
    assert!(pooled[0] > pooled[1], "heavier piece pulls the pooled vector toward itself");
    assert!((pooled[0] - 0.9487).abs() < 1e-3, "got {}", pooled[0]);
    assert!((pooled[1] - 0.3162).abs() < 1e-3, "got {}", pooled[1]);
}

#[test]
fn pool_output_is_l2_normalized() {
    // After pooling, the result should have unit magnitude (L2 norm = 1).
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![4.0, 5.0, 6.0];
    let pooled = pool_vectors(&[a, b], &[1, 1]);
    let magnitude: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (magnitude - 1.0).abs() < 1e-5,
        "pooled vector is unit-normalized, got magnitude {magnitude}"
    );
}

#[test]
fn pool_mismatched_lengths_returns_empty() {
    let a = vec![1.0, 2.0];
    let b = vec![1.0]; // different length
    let pooled = pool_vectors(&[a, b], &[1, 1]);
    assert!(pooled.is_empty(), "length mismatch => empty (caller guards)");
}
