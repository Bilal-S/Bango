//! Pure, `#[must_use]` helpers for the embedding pipeline (no I/O).
//!
//! [`format_embedding_text`], [`hash_text`], [`expected_rows`], [`cosine_similarity`],
//! [`serialize_embedding`]/[`deserialize_embedding`], [`split_text_by_token_budget`],
//! [`pool_vectors`].

use sha2::{Digest, Sha256};

/// Sentinel `chunk_index = -1` for title+abstract rows. Per-chunk rows use `>= 0`.
///
/// `-1` not `NULL` because SQLite treats NULLs as distinct in composite PKs,
/// defeating `INSERT OR REPLACE`. `-1` can never collide with a real chunk index (`>= 0`).
pub const TITLE_ABSTRACT_CHUNK_INDEX: i32 = -1;

/// Build embedded text for one row.
///
/// Title+abstract (`chunk_body = None`): `title + "\n\n" + abstract`.
/// Per-chunk (`chunk_body = Some(body)`): `title + "\n\n" + body` (title prefix
/// carries article-identity signal). Whitespace-only title treated as empty.
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

/// SHA-256 hex digest of the embedded text. `input_hash` for staleness detection:
/// when source text changes → hash changes → row re-embedded next run.
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
    pub chunk_index: i32,
    /// Chunk body text (title prefix added by [`format_embedding_text`]).
    pub body: String,
}

/// Compute `(chunk_index, text)` pairs for one article.
///
/// Always: title+abstract row (`-1`). Full-text: + one row per chunk. Empty only when
/// no text and no chunks exist.
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
/// Returns `0.0` on length mismatch, zero magnitude, or empty input.
/// Range `[-1.0, 1.0]`.
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

/// Serialize f32 vector → little-endian BLOB (`vec.len() * 4` bytes).
/// Dimensions stored separately for length validation.
#[must_use]
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &v in vec {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Deserialize little-endian BLOB → f32 vector. Returns `None` on corrupt blob
/// or dimension mismatch.
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

/// One text piece from [`split_text_by_token_budget`]. `token_count` is the
/// weight for [`pool_vectors`] token-weighted mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPiece {
    /// The piece text.
    pub text: String,
    /// The number of tokens in `text` (used as the pool weight).
    pub token_count: usize,
}

/// Split text into pieces ≤ `max_tokens`. Token estimation via
/// [`crate::utils::text_tokens::tokenize`].
///
/// Priority: 1) whole text fits → one piece; 2) sentence-boundary greedy pack;
/// 3) word-boundary fallback for oversized sentences; 4) character hard-split
/// (rare, pathologically long tokens). Always returns ≥1 piece.
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

/// Naive sentence splitter on `.`, `!`, `?` boundaries. Simple (no NLP) --
/// good enough for embedding piece boundaries where exactness doesn't matter.
/// Whitespace-only sentences dropped.
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

/// Token-weighted mean-pool + L2-normalize piece vectors into one vector.
///
/// Preserves storage contract: `(article_id, chunk_index) → 1 vector`.
/// Empty input → empty. Single piece → verbatim. Multiple pieces → weighted mean
/// (weights = `piece_tokens`), then L2-normalized. Length mismatch → empty.
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
