# Batch Import Improvements - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

Covers the three coupled concerns in `.worktrees/import_plan.md`:
1. Parallel Phase 4 (AI summaries via `JoinSet`).
2. DOI-aware attach filename (`{clean_doi}.{ext}`, no UUID duplicate).
3. Short DB lock bursts in Phases 1 + 2 (split pipeline: lock-free PDF parse
   on `spawn_blocking`, short lock burst for DB writes only).

Plus the post-implementation audit gap closures recorded in
`.worktrees/import_gaps.md` (split-pipeline direct tests, O(n) DOI lookup).

## Concern 2 - DOI-aware filename (`commands::full_text`)

Pure helper + integration coverage of the `{clean_doi}.{ext}` naming decision.

| Test identifier | Assertion |
|---|---|
| `src-tauri/src/commands/full_text.rs::dest_filename_uses_clean_doi_when_present` | DOI present -> `{clean_doi}.{ext}` with no UUID |
| `src-tauri/src/commands/full_text.rs::dest_filename_falls_back_to_uuid_when_doi_absent` | DOI absent -> `{stem}_{article_id}.{ext}` |
| `src-tauri/src/commands/full_text.rs::dest_filename_falls_back_to_uuid_when_doi_empty` | Empty/whitespace DOI treated as absent |
| `src-tauri/src/commands/full_text.rs::place_file_no_op_when_source_equals_dest` | Same canonical path short-circuits (no self-copy truncation) |
| `src-tauri/src/commands/full_text.rs::place_file_hard_links_or_copies_to_new_dest` | New dest gets the content via hard-link or copy fallback |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase1_uses_doi_filename_when_article_has_doi` | Phase 1 end-to-end stores `{clean_doi}.txt` for a DOI article |

## Concern 3 - Split pipeline direct tests (`commands::full_text`)

The batch-import Phase 1 runner uses `attach_full_text_split` (lock-free parse
on `spawn_blocking` + short locked burst for DB writes). These isolated tests
cover the split functions directly; previously only the monolithic
`attach_full_text_inner` path was unit-tested (`figures_flag_test.rs`).

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/utils/full_text_split_test.rs::extract_sets_has_figures_or_tables_true_when_caption_present` | `extract_full_text_data` sets the figures flag when a caption is detected |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_sets_has_figures_or_tables_false_for_plain_prose` | Flag stays false for prose with no captions |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_invalid_pdf_soft_fallback_with_empty_text_and_error` | Invalid PDF -> empty `full_text` + `extraction_error` set (soft fallback) |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_unsupported_extension_is_a_hard_error` | Unsupported extension -> hard `Err` (nothing to attach) |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_missing_file_is_a_hard_error` | Missing source file -> hard `Err` |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_uses_doi_based_destination_filename_when_doi_present` | Destination filename is `{clean_doi}.txt` when a DOI is supplied |
| `src-tauri/tests/utils/full_text_split_test.rs::extract_uses_uuid_based_destination_filename_when_doi_absent` | Destination filename is `{stem}_{id}.txt` when no DOI |
| `src-tauri/tests/utils/full_text_split_test.rs::commit_writes_full_text_chunks_audit_and_flags` | `commit_full_text_to_db` updates the row, writes chunks, sets the figures flag |
| `src-tauri/tests/utils/full_text_split_test.rs::commit_extraction_failure_writes_error_audit_but_attachment_persists` | Extraction-failure path still attaches (empty `full_text`) + writes an error audit row |
| `src-tauri/tests/utils/full_text_split_test.rs::split_pipeline_attaches_txt_and_commits_via_short_lock_burst` | `attach_full_text_split` end-to-end: parse + commit composes correctly |
| `src-tauri/tests/utils/full_text_split_test.rs::split_pipeline_uses_doi_filename_when_doi_provided` | Split pipeline produces the DOI-based destination filename end-to-end |
| `src-tauri/tests/utils/full_text_split_test.rs::split_pipeline_soft_fallback_on_invalid_pdf` | Split pipeline soft-fallbacks on invalid PDF (attach persists, `extraction_failed` true) |

## Concern 3 - Phase 1 lock scope + match map (`batch_import::full_text_phase`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/src/batch_import/full_text_phase.rs::build_match_map_normalizes_dois` | Match map keys are `clean_doi_filename`-normalized |
| `src-tauri/src/batch_import/full_text_phase.rs::build_match_map_first_doi_wins_on_collision` | First article wins when two DOIs clean to the same filename |
| `src-tauri/src/batch_import/full_text_phase.rs::build_match_map_skips_empty_dois` | Empty-DOI articles are excluded from the match map |
| `src-tauri/src/batch_import/full_text_phase.rs::build_id_to_doi_map_indexes_articles_with_dois` | Secondary `article_id -> DOI` index covers articles with DOIs, excludes empty-DOI |
| `src-tauri/src/batch_import/full_text_phase.rs::build_id_to_doi_map_empty_input` | Empty input yields an empty index (O(1) lookup basis) |

## Concern 3 - Phase 2 lock scope + discovery (`batch_import::citations_phase`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/src/batch_import/citations_phase.rs::discover_skips_when_article_already_has_reference_details` | Discovery skips refs when `has_reference_details` is true |
| `src-tauri/src/batch_import/citations_phase.rs::discover_finds_references_file_when_missing` | Discovery finds `{clean_doi}_references.ris` |
| `src-tauri/src/batch_import/citations_phase.rs::discover_finds_citations_file_independently` | Citations discovery is independent of the refs skip flag |
| `src-tauri/src/batch_import/citations_phase.rs::discover_generic_ris_fallback_when_no_refs_suffix` | Generic `{clean_doi}.ris` fallback when no `_references.ris` |
| `src-tauri/src/batch_import/citations_phase.rs::discover_generic_bib_fallback` | Generic `{clean_doi}.bib` fallback |

## Phase 1 + 2 end-to-end (`batch_import_test.rs`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/batch_import/batch_import_test.rs::phase1_attaches_full_text_to_matching_articles` | Phase 1 attaches files matching by DOI |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase1_skips_articles_that_already_have_full_text` | Already-attached articles are skipped |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase1_ignores_files_with_no_matching_doi` | Files with no matching article DOI are ignored |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase1_skips_article_without_doi` | No-DOI articles are not matched |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase2_imports_references_from_ris_files` | Phase 2 imports `_references.ris` |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase2_imports_citations_from_ris_files` | Phase 2 imports `_citations.ris` |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase2_imports_references_and_citations_independently` | Refs + citations imported independently in one pass |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase2_skips_articles_that_already_have_reference_details` | Already-has-refs articles skipped |
| `src-tauri/tests/batch_import/batch_import_test.rs::full_pipeline_is_idempotent_on_second_run` | Second run finds nothing to do (flags set) |
| `src-tauri/tests/batch_import/batch_import_test.rs::pipeline_handles_multiple_articles_with_mixed_files` | Mixed attachments across multiple articles |

## Phase 3 pre-flight LLM guard (`batch_import::translations_phase`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/batch_import/batch_import_test.rs::phase3_pre_flight_skips_when_llm_not_configured_and_writes_audit` | No-LLM -> skip with canonical message + system audit row |
| `src-tauri/tests/batch_import/batch_import_test.rs::phase3_pre_flight_proceeds_when_llm_is_configured` | LLM configured -> proceeds, no skip audit row |