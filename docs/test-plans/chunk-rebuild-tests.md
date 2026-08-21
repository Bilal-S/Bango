# Chunk Rebuild Tests (binding inventory)

Source of truth for the "Rebuild text chunks" async pipeline. Enforced by
`scripts/check-test-inventory.sh` via `npm run check:all` (`check:test-inventory`).

Implements: `.worktrees/chunk_fix.md` + `.worktrees/chunk_fix_fixes.md`
(critique round 2). Contract: `src-tauri/src/commands/AGENTS.md`.

## Rust - `src-tauri/tests/chunk_rebuild_test.rs`

| Identifier | Assertion |
|---|---|
| `src-tauri/tests/chunk_rebuild_test.rs::loop_chunks_txt_articles_and_reports_progress` | happy path writes chunks, percent reaches 100 |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_missing_file_logs_error_and_counts_failed` | failure mode: missing on-disk file -> failed++, errors entry, audit row |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_missing_file_name_logs_error_and_counts_failed` | failure mode: NULL file name -> failed++, errors entry, audit row |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_skips_translated_articles_preserving_chunks` | `is_translated=1` chunks byte-identical, skipped, in backfill scope |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_cancel_token_stops_processing` | pre-set token -> is_cancelled, zero processed |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_deletes_stale_embeddings_for_rechunked_articles` | re-chunked article's embedding rows cleared |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_caps_error_list_length` | errors list capped at 50; failed reports true total; finalize tail line |
| `src-tauri/tests/chunk_rebuild_test.rs::loop_chunk_write_rolls_back_on_embedding_delete_failure` | chunk REPLACE rolls back when embedding DELETE fails (one tx) |
| `src-tauri/tests/chunk_rebuild_test.rs::embedding_cascade_scopes_maps_ids_to_scopes` | empty -> None; non-empty -> ids, force=false |
| `src-tauri/tests/chunk_rebuild_test.rs::embedding_summary_line_maps_skip_reasons` | LlmNotConfigured / Disabled friendly lines; counts + model |
| `src-tauri/tests/chunk_rebuild_test.rs::candidates_query_returns_translated_flag` | discovery query returns id + file name + translated flag |
| `src-tauri/tests/chunk_rebuild_test.rs::summary_message_includes_translated_skip_note` | summary + "Cancelled after " prefix |
| `src-tauri/tests/chunk_rebuild_test.rs::finalize_sets_cancelled_and_keeps_skipped_coherent` | cascade-time cancel sets is_cancelled; skipped stays == translated skips |
| `src-tauri/tests/chunk_rebuild_test.rs::claim_run_slot_rejects_second_claim` | atomic claim: second claim rejected without snapshot reset |

## TypeScript - `src/__tests__/components/settings-reprocessing.test.ts`

| Identifier | Assertion |
|---|---|
| `src/__tests__/components/settings-reprocessing.test.ts::rebuildChunks_starts_async_task_and_reveals_bar` | start invoke + widget reveal |
| `src/__tests__/components/settings-reprocessing.test.ts::rebuildBar_hidden_on_fresh_mount` | bar hidden when idle on mount |
| `src/__tests__/components/settings-reprocessing.test.ts::rebuildBar_restored_on_mount_when_run_is_live` | restore-on-mount from snapshot |
| `src/__tests__/components/settings-reprocessing.test.ts::rebuildBar_renders_counts_skip_note_and_errors_from_event` | counts summary, translated skip note, per-article error list |
| `src/__tests__/components/settings-reprocessing.test.ts::rebuildBar_embedding_phase_subline_and_final_summary` | cascade sub-line + final embedding summary line |
| `src/__tests__/components/settings-reprocessing.test.ts::cancel_button_invokes_cancel_rebuild_chunks` | Cancel button wiring |
| `src/__tests__/components/settings-reprocessing.test.ts::chunk_percent_maps_to_0_90_band_and_hides_counter_during_embeddings` | phase-mapped bar: 0-90 chunks band, counter hidden during cascade |
| `src/__tests__/components/settings-reprocessing.test.ts::cascade_band_90_100_driven_by_embedding_progress` | cascade band 90-100 driven by `embedding:progress` counters |
