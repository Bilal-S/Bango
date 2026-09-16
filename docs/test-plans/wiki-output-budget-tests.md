# Wiki Output Budget Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

Implements the test inventory from `.worktrees/wikifix-final.md` §Test impact and tiering.
Prep tier adds these as `#[ignore]` stubs; the implementation tier un-ignores each test as its behavior lands.

| File | Test | Description |
|------|------|-------------|
| `src-tauri/tests/wiki/wiki_summary_export_test.rs::article_content_renders_full_summary_blob_and_never_full_text` | `article_content_renders_full_summary_blob_and_never_full_text` | Blob present + full_text present -> rendered blob markdown, kind `ai_summary`, no full-text string |
| `src-tauri/tests/wiki/wiki_summary_export_test.rs::article_content_renders_section_summaries_and_typed_facts` | `article_content_renders_section_summaries_and_typed_facts` | v2 blob renders section summaries, key points, `study_design`, `sample_size`, `effect_size`, `confidence_interval` |
| `src-tauri/tests/wiki/wiki_summary_export_test.rs::article_content_falls_back_to_abstract_without_blob` | `article_content_falls_back_to_abstract_without_blob` | No blob -> `(abstract_text, "abstract")` even when full text exists |
| `src-tauri/tests/wiki/wiki_summary_export_test.rs::raw_export_body_carries_blob_content_not_full_text` | `raw_export_body_carries_blob_content_not_full_text` | `write_article_exports` body contains rendered blob content and never the full text |
| `src-tauri/tests/wiki/wiki_full_text_refresh_test.rs::ensure_summaries_targets_only_full_text_articles_missing_blobs` | `ensure_summaries_targets_only_full_text_articles_missing_blobs` | Ensure-summaries query returns exactly included full-text articles whose blob is NULL/empty |
| `src-tauri/tests/wiki/wiki_full_text_refresh_test.rs::ensure_summaries_failure_falls_back_to_abstract_with_audit_entry` | `ensure_summaries_failure_falls_back_to_abstract_with_audit_entry` | Generation failure is non-fatal: abstract fallback + audit entry, pipeline continues |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::batch_input_char_budget_scales_past_80k_up_to_2m_ceiling` | `batch_input_char_budget_scales_past_80k_up_to_2m_ceiling` | Large windows produce proportionally larger budgets, clamped at 2,000,000 chars |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::oversize_single_source_truncated_at_word_boundary_and_counted` | `oversize_single_source_truncated_at_word_boundary_and_counted` | A single source exceeding `usable_budget` is word-boundary truncated and the count surfaces in the report |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::truncated_response_drops_partial_trailing_page` | `truncated_response_drops_partial_trailing_page` | A response ending mid-page drops the partial trailing page |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::continuation_redispatches_only_uncovered_sources_bounded_at_two` | `continuation_redispatches_only_uncovered_sources_bounded_at_two` | On truncation, continuation re-dispatches only sources missing from parsed `source_articles`, max 2 per batch |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::batch_sizing_respects_estimated_output_budget` | `batch_sizing_respects_estimated_output_budget` | Batching respects both the input budget and 70% of the estimated effective output budget |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::build_batch_prompt_carries_soft_page_budget_and_no_word_cap` | `build_batch_prompt_carries_soft_page_budget_and_no_word_cap` | Prompt carries the soft page-count aim and contains no per-page word cap |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::build_batch_prompt_carries_existing_pages_index` | `build_batch_prompt_carries_existing_pages_index` | Prompt embeds the Existing Pages Index (slug/title/type) plus the reuse-existing-slugs directive |
| `src-tauri/tests/wiki/wiki_ingest_test.rs::ingest_report_records_volume_metrics_and_warns_on_regression` | `ingest_report_records_volume_metrics_and_warns_on_regression` | Report/log records per-type page counts + total chars and warns when LLM pages drop >20% vs the previous run |
| `src-tauri/tests/llm/llm_client_test.rs::call_meta_flags_truncation_from_finish_and_stop_reasons` | `call_meta_flags_truncation_from_finish_and_stop_reasons` | `CallMeta` surfaces truncation for `finish_reason=length`, `stop_reason=max_tokens`, and the Google reason |
| `src-tauri/tests/llm/llm_client_test.rs::anthropic_request_keeps_max_tokens_ceiling_and_backdown` | `anthropic_request_keeps_max_tokens_ceiling_and_backdown` | Anthropic path keeps the ceiling-not-target `max_tokens` plus over-cap back-down; no output cap is added to the OpenAI-compatible or Google request bodies |
