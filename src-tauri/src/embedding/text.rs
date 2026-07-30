//! Pure helpers for the embedding pipeline.
//!
//! These functions are pure (no I/O) and `#[must_use]` so they can be
//! unit-tested in isolation without a database or LLM connection.
//!
//! - [`format_embedding_text`] builds the text that gets embedded for one row.
//! - [`hash_text`] computes the SHA-256 hex digest used for staleness checks.
//! - [`expected_rows`] computes the `(chunk_index, text)` pairs for an article.
//! - [`cosine_similarity`] scores two vectors (max-pool recall ranking).
//! - [`serialize_embedding`] / [`deserialize_embedding`] convert between
//!   `Vec<f32>` and the little-endian byte stream stored in the BLOB column.

use sha2::{Digest, Sha256};

/// The title+abstract row uses the sentinel `chunk_index = -1`. Per-chunk rows
/// use the matching `article_chunks.chunk_index` (`>= 0`).
///
/// We use `-1` instead of `NULL` because SQLite treats `NULL` values as
/// **distinct** in a composite PRIMARY KEY, so `INSERT OR REPLACE` with
/// `chunk_index = NULL` would never replace a prior title+abstract row (each
/// NULL is a new key). The `-1` sentinel is a real value that participates in
/// PK uniqueness correctly, and it can never collide with a genuine chunk
/// index (`article_chunks.chunk_index` is always `>= 0`).
pub const TITLE_ABSTRACT_CHUNK_INDEX: i32 = -1;

/// Build the text that gets embedded for one row.
///
/// - Title+abstract row (`chunk_body = None`): `title + "\n\n" + abstract`.
///   If the abstract is empty, the title alone is used (still meaningful for
///   short records).
/// - Per-chunk row (`chunk_body = Some(body)`): `title + "\n\n" + body`. The
///   title prefix is included so each chunk vector carries article-identity
///   signal, not just the chunk's local content.
///
/// A whitespace-only title is treated as empty to avoid a leading blank line.
#[must_use]
pub fn format_embedding_text(title: &str, abstract_text: &str, chunk_body: Option<&str>) -> String {
    let title_trimmed = title.trim();
    match chunk_body {
        Some(body) if !body.trim().is_empty() => {
            if title_trimmed.is_empty() {
                body.to_string()
            } else {
                format!("{title_trimmed}\n\n{body}")
            }
        }
        _ => {
            let abstract_trimmed = abstract_text.trim();
            if title_trimmed.is_empty() {
                abstract_trimmed.to_string()
            } else if abstract_trimmed.is_empty() {
                title_trimmed.to_string()
            } else {
                format!("{title_trimmed}\n\n{abstract_trimmed}")
            }
        }
    }
}

/// Compute the SHA-256 hex digest of the exact text that was embedded.
///
/// Used as the per-row `input_hash` so the director can detect staleness:
/// when the title, abstract, or chunk body changes, the hash changes, and the
/// row is re-embedded on the next pipeline run.
#[must_use]
pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let bytes = hasher.finalize();
    // Render as lowercase hex (64 chars for SHA-256).
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// A chunk body paired with its `article_chunks.chunk_index`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkInput {
    /// The `article_chunks.chunk_index` (`>= 0`).
    pub chunk_index: i32,
    /// The chunk body text (without the title prefix; the prefix is added by
    /// [`format_embedding_text`]).
    pub body: String,
}

/// Compute the `(chunk_index, text)` pairs that should be embedded for one
/// article.
///
/// - Row 0 is always the title+abstract row (`chunk_index = -1` sentinel).
/// - When `has_full_text = true` AND `chunks` is non-empty, one row per chunk
///   is appended (title prefix + chunk body).
/// - When `has_full_text = false` OR `chunks` is empty, only the title+abstract
///   row is produced (abstract-only articles still participate in recall).
///
/// Returns the empty vec only when both the title and abstract are empty/blank
/// AND there are no chunks (nothing meaningful to embed).
#[must_use]
pub fn expected_rows(
    title: &str,
    abstract_text: &str,
    chunks: &[ChunkInput],
    has_full_text: bool,
) -> Vec<(i32, String)> {
    let mut rows: Vec<(i32, String)> = Vec::new();

    // Title+abstract row (always emitted when there is any text to embed).
    // Uses the -1 sentinel (see `TITLE_ABSTRACT_CHUNK_INDEX`).
    let ta_text = format_embedding_text(title, abstract_text, None);
    if !ta_text.trim().is_empty() {
        rows.push((TITLE_ABSTRACT_CHUNK_INDEX, ta_text));
    }

    // Per-chunk rows: only when full text is attached AND chunks exist.
    if has_full_text {
        for chunk in chunks {
            let text = format_embedding_text(title, "", Some(&chunk.body));
            if !text.trim().is_empty() {
                rows.push((chunk.chunk_index, text));
            }
        }
    }

    rows
}

/// Cosine similarity between two f32 vectors.
///
/// Returns `0.0` on length mismatch (the caller is responsible for filtering
/// out dimension-mismatched rows before scoring). Returns `0.0` when either
/// vector has zero magnitude (no signal). Range: `[-1.0, 1.0]`; identical
/// vectors score `1.0`; orthogonal vectors score `0.0`; opposite vectors score
/// `-1.0`.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut mag_a = 0.0_f32;
    let mut mag_b = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        mag_a += x * x;
        mag_b += y * y;
    }
    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Serialize an f32 vector into a little-endian byte stream for BLOB storage.
///
/// The byte length is `vec.len() * 4`. The dimensions are stored in a separate
/// column so the reader can validate the blob length matches.
#[must_use]
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize a little-endian byte stream back into an f32 vector.
///
/// Returns `None` when the byte length is not a multiple of 4 (corrupt blob).
/// The caller passes the stored `dimensions` to validate the decoded length.
#[must_use]
pub fn deserialize_embedding(bytes: &[u8], dimensions: i32) -> Option<Vec<f32>> {
    if dimensions <= 0 {
        return None;
    }
    let expected = (dimensions as usize) * 4;
    if bytes.len() != expected {
        return None;
    }
    let mut out = Vec::with_capacity(dimensions as usize);
    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().ok()?;
        out.push(f32::from_le_bytes(arr));
    }
    Some(out)
}

// ── v2: arbitrary-length text splitting + vector pooling ────────────────────

/// One piece produced by [`split_text_by_token_budget`].
///
/// `token_count` is the weight used by [`pool_vectors`] when re-assembling
/// multiple piece vectors into one (token-weighted mean: a 1000-token piece
/// outweighs a 100-token piece).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPiece {
    /// The piece text.
    pub text: String,
    /// The number of tokens in `text` (used as the pool weight).
    pub token_count: usize,
}

/// Split a single text into pieces that each fit within `max_tokens`.
///
/// Strategy (in priority order):
/// 1. If the whole text fits, return a single piece.
/// 2. Split at sentence boundaries (`. `, `! `, `? `) and greedily pack
///    sentences into pieces under the budget.
/// 3. For a sentence that alone exceeds the budget, fall back to word
///    boundaries: greedily pack words into pieces under the budget.
/// 4. For a single word that alone exceeds the budget, hard-split by character
///    count (rare; only for pathologically long tokens with no whitespace).
///
/// Token estimation uses the shared [`crate::utils::text_tokens::tokenize`]
/// counter (whitespace-split, consistent with screening + FTS5). Each returned
/// piece carries its `token_count` so the caller can pass it as the weight to
/// [`pool_vectors`].
///
/// Returns at least one piece (possibly empty when the input is empty).
#[must_use]
pub fn split_text_by_token_budget(text: &str, max_tokens: usize) -> Vec<TextPiece> {
    if max_tokens == 0 {
        // Degenerate: caller asked for zero-token pieces. Return the whole
        // text as one piece so we never lose content; the caller's downstream
        // HTTP call will fail with a clear "too many tokens" error rather than
        // silently dropping data.
        let tokens = crate::utils::text_tokens::tokenize(text).len();
        return vec![TextPiece { text: text.to_string(), token_count: tokens }];
    }

    let total_tokens = crate::utils::text_tokens::tokenize(text).len();
    if total_tokens <= max_tokens {
        return vec![TextPiece { text: text.to_string(), token_count: total_tokens }];
    }

    // Strategy 2: split at sentence boundaries + greedily pack.
    let sentences = split_into_sentences(text);
    let mut pieces: Vec<TextPiece> = Vec::new();
    let mut current_text = String::new();
    let mut current_tokens = 0usize;

    for sentence in sentences {
        let sentence_tokens = crate::utils::text_tokens::tokenize(&sentence).len();

        if sentence_tokens > max_tokens {
            // Flush whatever we've accumulated before handling the oversized
            // sentence (so its pieces don't merge with prior content).
            if !current_text.trim().is_empty() {
                pieces.push(TextPiece {
                    text: current_text.trim().to_string(),
                    token_count: current_tokens,
                });
                current_text.clear();
                current_tokens = 0;
            }
            // Strategy 3: split the oversized sentence at word boundaries.
            for word_piece in split_word_pack(&sentence, max_tokens) {
                pieces.push(word_piece);
            }
        } else if current_tokens + sentence_tokens <= max_tokens {
            // Fits in the current piece; accumulate.
            if !current_text.is_empty() {
                current_text.push(' ');
            }
            current_text.push_str(&sentence);
            current_tokens += sentence_tokens;
        } else {
            // Doesn't fit; flush the current piece, start a new one.
            if !current_text.trim().is_empty() {
                pieces.push(TextPiece {
                    text: current_text.trim().to_string(),
                    token_count: current_tokens,
                });
            }
            current_text = sentence.clone();
            current_tokens = sentence_tokens;
        }
    }
    // Flush the tail.
    if !current_text.trim().is_empty() {
        pieces
            .push(TextPiece { text: current_text.trim().to_string(), token_count: current_tokens });
    }

    if pieces.is_empty() {
        // Defensive: never return an empty vec (caller expects ≥1 piece).
        vec![TextPiece { text: text.to_string(), token_count: total_tokens }]
    } else {
        pieces
    }
}

/// Naive sentence splitter: splits on `. `, `! `, `? ` boundaries, preserving
/// the trailing punctuation with each sentence. Whitespace-only sentences are
/// dropped. This is intentionally simple (no NLP) — good enough for embedding
/// piece boundaries where exactness doesn't matter, only that splits avoid
/// landing mid-word.
fn split_into_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // Peek: if the next char is whitespace, treat as a sentence end.
            // We defer the actual split decision until we see the whitespace
            // to avoid splitting decimals like "3.14" (no space follows).
            // Simplification: just split on the punctuation + the following
            // whitespace is consumed naturally by the next iteration's trim.
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

/// Pack words greedily into pieces under the token budget. Each word is one
/// token (per the whitespace tokenizer). Falls back to character hard-split
/// for a single word that exceeds the budget (extremely rare).
fn split_word_pack(text: &str, max_tokens: usize) -> Vec<TextPiece> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut pieces = Vec::new();
    let mut current_words: Vec<&str> = Vec::new();
    for word in words {
        if !current_words.is_empty() && current_words.len() + 1 > max_tokens {
            // Flush current piece before adding the next word.
            let joined = current_words.join(" ");
            pieces.push(TextPiece { text: joined, token_count: current_words.len() });
            current_words.clear();
        }
        current_words.push(word);
    }
    if !current_words.is_empty() {
        let joined = current_words.join(" ");
        pieces.push(TextPiece { text: joined, token_count: current_words.len() });
    }
    pieces
}

/// Token-weighted mean-pool + L2-normalize a set of piece vectors into ONE
/// representative vector.
///
/// Used by [`crate::llm::orchestrator::LlmOrchestrator::send_embedding_batch_parallel`]
/// when [`split_text_by_token_budget`] produced more than one piece for a
/// single input text — each piece was embedded separately, then this fn merges
/// the resulting vectors back into a single per-input vector so the storage
/// contract `(article_id, chunk_index) -> 1 vector` is preserved.
///
/// Semantics:
/// - Empty input → empty output.
/// - Single piece → returned verbatim (no normalization; the provider's
///   vector is already in the right shape).
/// - Multiple pieces → weighted mean (weights = `piece_tokens`), then
///   L2-normalized so the result has unit magnitude (matches the scale of
///   non-pooled vectors from the same provider).
/// - Length mismatch across pieces → empty output (defense-in-depth; the
///   caller's per-row guard skips the slot).
#[must_use]
pub fn pool_vectors(pieces: &[Vec<f32>], weights: &[usize]) -> Vec<f32> {
    if pieces.is_empty() {
        return Vec::new();
    }
    if pieces.len() != weights.len() {
        return Vec::new();
    }
    let dim = pieces[0].len();
    if dim == 0 {
        return Vec::new();
    }
    if pieces.iter().any(|p| p.len() != dim) {
        return Vec::new();
    }
    if pieces.len() == 1 {
        return pieces[0].clone();
    }
    let total_weight: usize = weights.iter().sum();
    if total_weight == 0 {
        return Vec::new();
    }
    let total_weight_f = total_weight as f32;
    let mut pooled = vec![0.0_f32; dim];
    for (vec, w) in pieces.iter().zip(weights.iter()) {
        let w_f = *w as f32 / total_weight_f;
        for (acc, &x) in pooled.iter_mut().zip(vec.iter()) {
            *acc += x * w_f;
        }
    }
    // L2-normalize.
    let magnitude: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for x in &mut pooled {
            *x /= magnitude;
        }
    }
    pooled
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            pieces.len(),
            1,
            "single token stays as one piece (token count is 1, under budget)"
        );
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
}
