# Wiki Ingest Freeze Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

Implements the test inventory from `.worktrees/wiki2.md` §T3.2.

| File | Test | Description |
|------|------|-------------|
| `src-tauri/tests/wiki_ingest_test.rs::cancel_token_aborts_between_preseed_phases` | `cancel_token_aborts_between_preseed_phases` | Cancel token signalled during pre-seed returns empty batches |
| `src-tauri/tests/wiki_ingest_test.rs::cancel_token_aborts_during_llm_batch` | `cancel_token_aborts_during_llm_batch` | Cancel token signalled during LLM batch aborts remaining tasks |
| `src-tauri/tests/wiki_ingest_test.rs::progress_events_fire_for_each_preseed_step` | `progress_events_fire_for_each_preseed_step` | WikiPrepProgressCb fires at each of the 7 pre-seed steps |
| `src-tauri/tests/wiki_ingest_test.rs::normalization_skipped_when_biblio_fresh` | `normalization_skipped_when_biblio_fresh` | `biblio_needs_refresh=false` skips `run_full_normalization` |
| `src-tauri/tests/wiki_ingest_test.rs::normalization_error_is_non_fatal` | `normalization_error_is_non_fatal` | Normalization error does not abort the pipeline |
| `src-tauri/tests/wiki_ingest_test.rs::wiki_ingest_emits_batch_progress_with_app_handle` | `wiki_ingest_emits_batch_progress_with_app_handle` | `run_chunked_ingest` emits progress events when `app_handle` is `Some` |
| `src/__tests__/composables/use-wiki.test.ts::cancelIngest invokes cancel_wiki_ingest` | `cancelIngest invokes cancel_wiki_ingest` | Composable method calls the IPC command |
| `src/__tests__/components/wiki-toolbar.test.ts::cancel button visible only during ingest` | `cancel button visible only during ingest` | Stop button renders only when `progress` is set |
| `src/__tests__/views/wiki-view.test.ts::autoIngestIfStale surfaces error toast` | `autoIngestIfStale surfaces error toast` | Backend error surfaces as a toast, not a silent freeze |