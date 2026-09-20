//! Tests for `translation::engine` response parsing + batching helpers.
//!
//! Extracted from the inline `#[cfg(test)] mod tests` in
//! `src/translation/engine.rs` to keep the source file compact.

use bango_lib::translation::engine::{
    batch_input_char_budget, build_chunk_batches, build_chunk_batches_for_indices,
    parse_batch_translation_response, parse_metadata_translation, MAX_BATCH_INPUT_CHARS,
    MIN_BATCH_INPUT_CHARS,
};

#[test]
fn parse_valid_response() {
    let resp = "TITLE:\nOn the Origin of Species\n\nABSTRACT:\nThis paper discusses evolution.";
    let parsed = parse_metadata_translation(resp).expect("parses");
    assert_eq!(parsed.title, "On the Origin of Species");
    assert_eq!(parsed.abstract_text, "This paper discusses evolution.");
}

#[test]
fn parse_tolerates_whitespace_and_preamble() {
    let resp = "Here is the translation:\n\nTITLE:\n  A Title  \n\nABSTRACT:\n  An abstract.  ";
    let parsed = parse_metadata_translation(resp).expect("parses");
    assert_eq!(parsed.title, "A Title");
    assert_eq!(parsed.abstract_text, "An abstract.");
}

#[test]
fn parse_lowercase_markers() {
    let resp = "title:\nFoo\n\nabstract:\nBar";
    let parsed = parse_metadata_translation(resp).expect("parses");
    assert_eq!(parsed.title, "Foo");
    assert_eq!(parsed.abstract_text, "Bar");
}

#[test]
fn parse_returns_none_when_markers_missing() {
    assert!(parse_metadata_translation("just some text").is_none());
}

#[test]
fn parse_returns_none_when_abstract_before_title() {
    let resp = "ABSTRACT:\nfoo\n\nTITLE:\nbar";
    assert!(parse_metadata_translation(resp).is_none());
}

#[test]
fn parse_returns_none_when_title_empty() {
    // Strict: an empty title must be a parse failure, not an empty-string
    // overwrite of the working article title.
    let resp = "TITLE:\n\nABSTRACT:\nSome text";
    assert!(parse_metadata_translation(resp).is_none());
}

#[test]
fn parse_returns_none_when_abstract_empty() {
    // Strict: an empty abstract must be a parse failure, not an empty-string
    // overwrite of the working article abstract.
    let resp = "TITLE:\nSome title\n\nABSTRACT:\n";
    assert!(parse_metadata_translation(resp).is_none());
}

#[test]
fn parse_handles_unicode_preamble_before_markers() {
    // Regression: a preamble containing characters whose `to_uppercase()`
    // form has a different byte length (e.g. the `ﬁ` ligature, U+FB01,
    // which expands to `FI`) must NOT shift the marker index and break the
    // slice. The case-insensitive search runs on the original string so
    // indices stay byte-stable.
    let resp = "Voici la traduction ﬁnale:\n\nTITLE:\nA Title\n\nABSTRACT:\nAn abstract.";
    let parsed = parse_metadata_translation(resp).expect("parses despite Unicode preamble");
    assert_eq!(parsed.title, "A Title");
    assert_eq!(parsed.abstract_text, "An abstract.");
}

// ── Batched chunk translation helpers (translation-3-plan.md) ──

fn make_chunk(idx: usize, text: &str) -> bango_lib::utils::chunking::Chunk {
    bango_lib::utils::chunking::Chunk {
        section: Some("Methods".to_string()),
        chunk_index: idx,
        text: text.to_string(),
        word_count: text.split_whitespace().count(),
    }
}

#[test]
fn build_chunk_batches_single_batch_when_small() {
    // Three tiny chunks + a generous context window → one batch containing
    // all three, in order.
    let chunks = vec![make_chunk(0, "alpha"), make_chunk(1, "beta"), make_chunk(2, "gamma")];
    let batches = build_chunk_batches(&chunks, 50_000);
    assert_eq!(batches.len(), 1, "small input packs into one batch");
    assert_eq!(batches[0].chunk_indices, vec![0, 1, 2]);
    // The prompt must reference each chunk by its id.
    assert!(batches[0].prompt.contains("\"0\""));
    assert!(batches[0].prompt.contains("\"1\""));
    assert!(batches[0].prompt.contains("\"2\""));
}

#[test]
fn build_chunk_batches_splits_when_large() {
    // A tiny context window forces a split once the budget is exceeded.
    let chunks: Vec<_> =
        (0..10).map(|i| make_chunk(i, &"chunk text padding ".repeat(200))).collect();
    let batches = build_chunk_batches(&chunks, 4_000);
    assert!(
        batches.len() > 1,
        "expected multiple batches for a large input with a tiny window, got {}",
        batches.len()
    );
}

#[test]
fn build_chunk_batches_preserves_input_order() {
    // Chunks must appear in ascending-index order across all batches, and
    // within each batch.
    let chunks: Vec<_> = (0..6).map(|i| make_chunk(i, &"chunk ".repeat(150))).collect();
    let batches = build_chunk_batches(&chunks, 4_000);
    let mut all_indices: Vec<usize> = Vec::new();
    for batch in &batches {
        // Within-batch ascending order.
        for w in batch.chunk_indices.windows(2) {
            assert!(w[0] < w[1], "batch indices must be ascending: {:?}", batch.chunk_indices);
        }
        all_indices.extend(batch.chunk_indices.iter().copied());
    }
    assert_eq!(all_indices, vec![0, 1, 2, 3, 4, 5], "global order must be input order");
}

#[test]
fn build_chunk_batches_every_chunk_exactly_once() {
    // Every chunk index must land in exactly one batch (no skips, no dups).
    let chunks: Vec<_> =
        (0..8).map(|i| make_chunk(i, &format!("chunk {i} {}", " ".repeat(120)))).collect();
    let batches = build_chunk_batches(&chunks, 4_000);
    let mut all_indices: Vec<usize> =
        batches.iter().flat_map(|b| b.chunk_indices.iter().copied()).collect();
    all_indices.sort_unstable();
    assert_eq!(all_indices, vec![0, 1, 2, 3, 4, 5, 6, 7], "every chunk exactly once");
}

#[test]
fn build_chunk_batches_respects_floor_and_cap() {
    // Non-positive window → fallback (clamped to [MIN, MAX]).
    let budget = batch_input_char_budget(0);
    assert!(
        (MIN_BATCH_INPUT_CHARS..=MAX_BATCH_INPUT_CHARS).contains(&budget),
        "fallback budget must be clamped, got {budget}"
    );
    // Negative window → same fallback.
    let budget_neg = batch_input_char_budget(-1);
    assert_eq!(budget, budget_neg, "negative window matches zero window fallback");
    // Huge window → clamped to MAX_BATCH_INPUT_CHARS.
    let budget_huge = batch_input_char_budget(10_000_000);
    assert_eq!(budget_huge, MAX_BATCH_INPUT_CHARS, "huge window is clamped to the cap");
    // Tiny positive window → clamped to MIN_BATCH_INPUT_CHARS.
    let budget_tiny = batch_input_char_budget(1);
    assert_eq!(budget_tiny, MIN_BATCH_INPUT_CHARS, "tiny window is clamped to the floor");
}

#[test]
fn build_chunk_batches_for_indices_uses_original_ids() {
    // Resend-round helper: a subset must keep the ORIGINAL chunk ids in the
    // prompt keys + the returned `chunk_indices`.
    let chunks = vec![
        make_chunk(0, "zero"),
        make_chunk(1, "one"),
        make_chunk(2, "two"),
        make_chunk(3, "three"),
        make_chunk(4, "four"),
    ];
    let batches = build_chunk_batches_for_indices(&chunks, &[1, 3], 50_000);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].chunk_indices, vec![1, 3]);
    assert!(batches[0].prompt.contains("\"1\""));
    assert!(batches[0].prompt.contains("\"3\""));
    // The other chunk ids must NOT appear (resend only includes the missing subset).
    assert!(!batches[0].prompt.contains("\"0\""));
    assert!(!batches[0].prompt.contains("\"2\""));
}

#[test]
fn parse_batch_translation_response_happy_path() {
    let resp = r#"{"0": "Hello", "1": "World"}"#;
    let parsed = parse_batch_translation_response(resp, &[0, 1]);
    assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
    assert_eq!(parsed.translated.get(&1).map(String::as_str), Some("World"));
    assert!(parsed.missing.is_empty());
}

#[test]
fn parse_batch_translation_response_missing_keys() {
    // The model returned only chunk 0; chunk 1 is missing.
    let resp = r#"{"0": "Hello"}"#;
    let parsed = parse_batch_translation_response(resp, &[0, 1]);
    assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
    assert_eq!(parsed.missing, vec![1]);
}

#[test]
fn parse_batch_translation_response_empty_values_marked_missing() {
    // Empty-string values are treated as missing so the caller resends them.
    let resp = r#"{"0": "Hello", "1": "   "}"#;
    let parsed = parse_batch_translation_response(resp, &[0, 1]);
    assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Hello"));
    assert_eq!(parsed.missing, vec![1], "whitespace-only values are missing");
}

#[test]
fn parse_batch_translation_response_strips_markdown_fences() {
    let resp = "```json\n{\"0\": \"Fenced\"}\n```";
    let parsed = parse_batch_translation_response(resp, &[0]);
    assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Fenced"));
    assert!(parsed.missing.is_empty());
}

#[test]
fn parse_batch_translation_response_malformed_falls_back_to_all_missing() {
    // Completely unparseable response → every expected id is missing.
    let parsed = parse_batch_translation_response("not json at all", &[0, 1, 2]);
    assert!(parsed.translated.is_empty());
    assert_eq!(parsed.missing, vec![0, 1, 2]);
}

#[test]
fn parse_batch_translation_response_regex_fallback_extracts_embedded_json() {
    // Model wraps JSON in preamble + postamble; the regex fallback should
    // still extract the {...} block.
    let resp = "Here is the translation:\n{\"0\": \"Extracted\"}\nDone.";
    let parsed = parse_batch_translation_response(resp, &[0]);
    assert_eq!(parsed.translated.get(&0).map(String::as_str), Some("Extracted"));
}
