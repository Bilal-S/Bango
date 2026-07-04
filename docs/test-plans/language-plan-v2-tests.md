# Binding Test Inventory: language-plan-v2

This file is the machine-checked binding test inventory for `.worktrees/language-plan-v2.md`.
The `scripts/check-test-inventory.sh` script parses the `file::function` identifiers in the tables below and greps the named test files to confirm each test exists.
Per the CLAUDE.md Test-First Protocol, every listed test must exist (un-ignored, passing) before the plan's final PR merges.

Row format (machine-parseable):

```
| `src-tauri/tests/<file>.rs::<test_fn_name>` | human-readable assertion |
| `src/__tests__/<file>.test.ts::<test_fn_name>` | human-readable assertion |
```

## Rust integration tests

| Test (`file::function`) | Purpose |
| --- | --- |
| `src-tauri/tests/translation_queue_test.rs::enqueue_import_non_english_creates_metadata_job` | Covers `TC-05` |
| `src-tauri/tests/translation_queue_test.rs::enqueue_attach_non_english_creates_full_text_job` | Covers `TC-05` |
| `src-tauri/tests/translation_queue_test.rs::metadata_job_translates_title_and_abstract_only` | Covers `TC-06` |
| `src-tauri/tests/translation_queue_test.rs::full_text_job_translates_chunks_and_rechunks_english` | Covers `TC-07` |
| `src-tauri/tests/translation_queue_test.rs::translation_job_persists_original_content_tables` | Covers `TC-10` |
| `src-tauri/tests/translation_queue_test.rs::translation_job_writes_audit_success_and_failure` | Covers `TC-13` |
| `src-tauri/tests/translation_queue_test.rs::startup_reenqueues_queued_and_running_articles` | Covers worker crash recovery |
| `src-tauri/tests/translation_queue_test.rs::translation_write_back_is_single_transaction` | Covers single-transaction atomicity |
| `src-tauri/tests/batch_import_translation_test.rs::phase_order_is_fulltext_citations_translation_summaries` | Covers `TC-08` |
| `src-tauri/tests/batch_import_translation_test.rs::summary_waits_for_required_translation` | Covers `TC-08` |
| `src-tauri/tests/batch_import_translation_test.rs::summary_runs_without_translation_when_not_required` | Covers `TC-08` |
| `src-tauri/tests/language_detection_test.rs::falls_back_to_unknown_when_metadata_missing` | Covers `TC-01` |
| `src-tauri/tests/language_detection_test.rs::metadata_language_wins_when_present` | Covers `TC-01` |
| `src-tauri/tests/language_detection_test.rs::english_abstract_skipped_by_stopword_heuristic` | Covers `TC-14` |
| `src-tauri/tests/language_detection_test.rs::latin_script_abstract_translated_by_stopword_heuristic` | Covers `TC-14` |
| `src-tauri/tests/multilingual_sections_test.rs::localized_headings_map_to_section_kind` | Covers `TC-02` |
| `src-tauri/tests/multilingual_sections_test.rs::unicode_numbered_headings_are_detected` | Covers `TC-03` |
| `src-tauri/tests/multilingual_assets_test.rs::all_manifest_assets_extract_and_chunk` | Covers `TC-04` |
| `src-tauri/tests/screening_translation_integration_test.rs::screening_uses_translated_text_when_available` | Covers `TC-11` |
| `src-tauri/tests/summary_translation_integration_test.rs::summary_uses_translated_text_when_available` | Covers `TC-12` |
| `src-tauri/tests/v001_v003_schema_parity_test.rs::translation_tables_match_between_v001_and_v003` | Covers v001/v003 parity |

## Frontend tests

| Test (`file::function`) | Purpose |
| --- | --- |
| `src/__tests__/components/detail-header-translation.test.ts::shows_translate_button_for_non_english_not_translated` | Covers `TC-09` |
| `src/__tests__/components/detail-header-translation.test.ts::hides_translate_button_when_translated` | Covers `TC-09` |
| `src/__tests__/components/article-detail-panel-translation.test.ts::click_translate_enqueues_job_and_shows_toast` | Covers `TC-09` |
| `src/__tests__/components/article-detail-panel-translation.test.ts::refreshes_article_on_translation_complete_event` | Covers `TC-09` |
| `src/__tests__/components/audit-timeline-translation.test.ts::renders_translation_actions` | Covers `TC-13` |