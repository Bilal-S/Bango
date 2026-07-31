//! Pure sub-batch grouping for the v2 embedding pipeline.
//!
//! [`group_into_embedding_batches`] is the pure bin-pack helper that
//! `LlmOrchestrator::send_embedding_batch_parallel` uses to split a flat list
//! of `(input_idx, TextPiece)` pairs into per-HTTP-request sub-batches that
//! respect ALL THREE provider limits simultaneously:
//! - `max_inputs_per_batch` (e.g. OpenAI: 2048, Ollama/Google: 1)
//! - `max_tokens_per_batch` (e.g. OpenAI: ~300_000, local: smaller)
//!
//! A sub-batch is closed when adding one more piece would exceed EITHER cap.
//! A single piece whose `token_count` alone exceeds `max_tokens_per_batch`
//! (only possible when `max_tokens_per_input == max_tokens_per_batch`, e.g.
//! Ollama) forms its own sub-batch - the splitter already fragmented the text
//! at `max_tokens_per_input`, so we cannot subdivide a `TextPiece` further
//! here.

use crate::embedding::text::TextPiece;
use crate::llm::embedding::EmbeddingLimits;

/// Group a flat list of `(input_idx, TextPiece)` pairs into sub-batches that
/// each fit within the provider's per-request limits.
///
/// Rules:
/// - Greedily accumulate pieces into the current sub-batch.
/// - Close the sub-batch (and start a new one) when adding the next piece
///   would exceed `max_inputs_per_batch` OR `max_tokens_per_batch`.
/// - A single piece that alone exceeds `max_tokens_per_batch` forms its own
///   sub-batch (it cannot be subdivided at this layer).
/// - Empty input returns an empty vec (no sub-batches); the caller handles
///   the empty case before dispatching.
///
/// Pure `#[must_use]` so it is unit-testable in isolation without a live
/// orchestrator or network.
#[must_use]
pub fn group_into_embedding_batches(
    flat: Vec<(usize, TextPiece)>,
    limits: &EmbeddingLimits,
) -> Vec<Vec<(usize, TextPiece)>> {
    if flat.is_empty() {
        return Vec::new();
    }

    let mut batches: Vec<Vec<(usize, TextPiece)>> = Vec::new();
    let mut current: Vec<(usize, TextPiece)> = Vec::new();
    let mut current_tokens: usize = 0;

    for piece in flat {
        let piece_tokens = piece.1.token_count;

        // Would adding this piece exceed either cap?
        let exceeds_inputs = current.len() >= limits.max_inputs_per_batch;
        let exceeds_tokens =
            current_tokens.saturating_add(piece_tokens) > limits.max_tokens_per_batch;

        if (exceeds_inputs || exceeds_tokens) && !current.is_empty() {
            // Flush the current sub-batch before starting a new one.
            batches.push(std::mem::take(&mut current));
            current_tokens = 0;
        }

        current.push(piece);
        current_tokens = current_tokens.saturating_add(piece_tokens);
    }

    // Flush the tail.
    if !current.is_empty() {
        batches.push(current);
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `(idx, TextPiece)` pair with the given token count.
    fn piece(idx: usize, tokens: usize) -> (usize, TextPiece) {
        (idx, TextPiece { text: format!("piece-{idx}-{tokens}"), token_count: tokens })
    }

    fn limits(inputs: usize, tokens_per_input: usize, tokens_per_batch: usize) -> EmbeddingLimits {
        EmbeddingLimits {
            max_inputs_per_batch: inputs,
            max_tokens_per_input: tokens_per_input,
            max_tokens_per_batch: tokens_per_batch,
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let l = limits(2048, 8191, 300_000);
        let batches = group_into_embedding_batches(Vec::new(), &l);
        assert!(batches.is_empty());
    }

    #[test]
    fn single_piece_single_batch() {
        let l = limits(2048, 8191, 300_000);
        let batches = group_into_embedding_batches(vec![piece(0, 100)], &l);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].0, 0);
    }

    #[test]
    fn respects_max_inputs_per_batch() {
        // 5 pieces, max_inputs_per_batch = 2 => 3 batches (2 + 2 + 1).
        let l = limits(2, 8191, 300_000);
        let flat = vec![piece(0, 10), piece(1, 10), piece(2, 10), piece(3, 10), piece(4, 10)];
        let batches = group_into_embedding_batches(flat, &l);
        assert_eq!(batches.len(), 3, "2+2+1 split");
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 2);
        assert_eq!(batches[2].len(), 1);
    }

    #[test]
    fn respects_max_tokens_per_batch() {
        // 3 pieces of 100 tokens each, max_tokens_per_batch = 250 => 2 batches
        // (100+100=200 fits; adding a 3rd would make 300 > 250).
        let l = limits(2048, 8191, 250);
        let flat = vec![piece(0, 100), piece(1, 100), piece(2, 100)];
        let batches = group_into_embedding_batches(flat, &l);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2, "first batch holds 2 pieces (200 tokens)");
        assert_eq!(batches[1].len(), 1, "second batch holds the 3rd piece");
    }

    #[test]
    fn single_oversized_piece_forms_own_batch() {
        // A piece whose token_count exceeds max_tokens_per_batch forms its own
        // batch (the splitter already fragmented at max_tokens_per_input; we
        // cannot subdivide further here).
        let l = limits(2048, 8191, 500);
        let flat = vec![piece(0, 10), piece(1, 1000), piece(2, 10)];
        let batches = group_into_embedding_batches(flat, &l);
        assert_eq!(batches.len(), 3, "oversized middle piece forces 3 batches");
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[2].len(), 1);
        assert_eq!(batches[1][0].0, 1, "middle piece is alone in its batch");
    }

    #[test]
    fn preserves_input_order_and_indices() {
        // Indices must survive grouping so the caller can scatter vectors back
        // into per-input slots.
        let l = limits(2, 8191, 300_000);
        let flat = vec![
            piece(0, 10),
            piece(1, 10),
            piece(2, 10),
            piece(3, 10),
            piece(4, 10),
            piece(5, 10),
        ];
        let batches = group_into_embedding_batches(flat, &l);
        let all_indices: Vec<usize> =
            batches.iter().flat_map(|b| b.iter().map(|(idx, _)| *idx)).collect();
        assert_eq!(all_indices, vec![0, 1, 2, 3, 4, 5], "indices preserved in order");
    }

    #[test]
    fn ollama_single_input_per_batch() {
        // Ollama's max_inputs_per_batch = 1, so every piece is its own batch.
        let l = limits(1, 2048, 2048);
        let flat = vec![piece(0, 50), piece(1, 50), piece(2, 50)];
        let batches = group_into_embedding_batches(flat, &l);
        assert_eq!(batches.len(), 3, "one-input-per-batch => 3 batches");
        for b in &batches {
            assert_eq!(b.len(), 1);
        }
    }
}
