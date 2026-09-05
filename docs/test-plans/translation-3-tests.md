# Binding Test Inventory: translation-3-plan.md

This file is the machine-checked binding test inventory for `.worktrees/translation-3-plan.md`.
The `scripts/check-test-inventory.sh` script parses the `file::function` identifiers in the table below and greps the named test files to confirm each test exists.
Per the CLAUDE.md Test-First Protocol, every listed test must exist (un-ignored, passing) before the plan's final PR merges.

Row format (machine-parseable):

```
| `src-tauri/src/<file>.rs::<test_fn_name>` | human-readable assertion |
| `src-tauri/tests/<file>.rs::<test_fn_name>` | human-readable assertion |
```

## Rust inline unit tests (`translation/engine.rs`)

Pure helpers (`build_chunk_batches`, `build_chunk_batches_for_indices`, `batch_input_char_budget`, `parse_batch_translation_response`).

| Test (`file::function`) | Purpose |
| --- | --- |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_single_batch_when_small` | Small input packs into one batch with all ids in order |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_splits_when_large` | Tiny context window forces multiple batches |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_preserves_input_order` | Global + within-batch indices are ascending |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_every_chunk_exactly_once` | Every chunk index lands in exactly one batch (no skips/dups) |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_respects_floor_and_cap` | `batch_input_char_budget` clamps to [MIN, MAX] on all boundary inputs |
| `src-tauri/src/translation/engine.rs::build_chunk_batches_for_indices_uses_original_ids` | Resend-round helper keeps original chunk ids in prompt + indices |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_happy_path` | JSON map parses all expected ids |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_missing_keys` | Missing keys reported in `missing` |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_empty_values_marked_missing` | Whitespace-only values are treated as missing |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_strips_markdown_fences` | ` ```json ... ``` ` wrapping is stripped before parse |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_malformed_falls_back_to_all_missing` | Unparseable response records all expected ids as missing |
| `src-tauri/src/translation/engine.rs::parse_batch_translation_response_regex_fallback_extracts_embedded_json` | Embedded `{...}` block extracted via regex when wrapped in preamble |

## Rust integration tests (`tests/translation/auto_translate_full_text_test.rs`)

End-to-end batched full-text translation through the real `translate_full_text` engine with a mock `LlmClient`.

| Test (`file::function`) | Purpose |
| --- | --- |
| `src-tauri/tests/translation/auto_translate_full_text_test.rs::full_text_translation_produces_english_chunks_and_full_text` | Batched path produces English stitched text + re-chunked English chunks |
| `src-tauri/tests/translation/auto_translate_full_text_test.rs::parallel_chunk_dispatch_preserves_input_order` | Concurrent batch dispatch preserves chunk order in stitched output |
| `src-tauri/tests/translation/auto_translate_full_text_test.rs::batched_translation_resends_missing_chunks` | Truncated first batch triggers a resend round that recovers missing chunks |
| `src-tauri/tests/translation/auto_translate_full_text_test.rs::batched_translation_fails_after_resend_cap` | Persistently-empty response fails the job after `MAX_RESEND_ITERATIONS` |