# Citation Finder - Test Inventory (cf2.md §9.2)

Consumed by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).
Rows use the machine-parseable `` `path::fn` `` format the script's regex
expects. Pure-helper unit tests live in external `src-tauri/tests/` files
(extracted from inline `#[cfg(test)] mod tests` blocks per `docs/CLAUDE.md`
§Testing). The `search.rs` pipeline tests stay inline because they exercise
private internals (`merge_outputs`, `pool_finalists`, `ClaimWork`, `Finalists`)
- those rows point at the `src/` file.

## Rust

| Test identifier | Assertion |
|-----------------|-----------|
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::jaccard_identical_sets_is_one` | identical token sets → 1.0 (retained pub helper; NOT the gate) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::jaccard_disjoint_sets_is_zero` | disjoint sets → 0.0 |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::jaccard_partial_overlap` | `{a,b,c}` vs `{b,c,d}` → 0.5 |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::jaccard_empty_input_is_zero` | empty input → 0.0 (no NaN) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::jaccard_diluted_by_long_chunk_exact_quote` | exact 12-token quote in 300-token chunk → Jaccard < 0.05 (pins why Jaccard is NOT the gate) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_exact_quote_in_long_chunk_is_one` | exact 12-token quote in 300-token chunk → containment 1.0 (the regression that would have caught the shipped bug) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_partial_overlap_is_query_fraction` | 4/10 query tokens present → 0.4 |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_disjoint_is_zero` | disjoint → 0.0 |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_empty_query_is_zero` | empty query → 0.0 (no NaN) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_empty_chunk_is_zero` | empty chunk → 0.0 |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::containment_is_length_insensitive_on_chunk_side` | same query vs 10-token + 1000-token chunks (both full match) → 1.0 both (defining property) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_empty_chunks_returns_none` | empty chunks → None |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_single_chunk` | single chunk passes through |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_picks_highest_scoring_chunk` | top containment wins |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_below_threshold_returns_none` | < 0.3 containment dropped |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_preserves_none_section` | `None` section preserved verbatim |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_tie_breaking_prefers_first` | ties keep first |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::find_best_passage_exact_quote_in_realistic_long_chunk` | carotenoids quote in ~200-word chunk → passes 0.3 gate (would fail old Jaccard 0.05 gate) |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::tokenize_drops_stop_words` | 57 stop words dropped |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::tokenize_handles_punctuation` | non-alphanumeric split |
| `src-tauri/tests/citation_finder/citation_finder_similarity_test.rs::tokenize_empty_input` | empty → empty vec |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::system_prompt_contains_required_fields` | mentions `misrepresents_source` + validating/opposing + JSON-array + 10-cap |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::whole_block_prompt_contains_user_text_and_candidates` | `<user_text>` tags + candidates section |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::whole_block_prompt_renders_candidate_metadata` | title/authors/year/journal/DOI appear in prompt |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::whole_block_prompt_omits_metadata_lines_when_article_absent_from_map` | graceful degradation when passage has no metadata |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::per_statement_prompt_contains_claims_list` | numbered claims + per-(article,claim) entries |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::parse_classification_valid` | validating / opposing parse |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::parse_classification_case_insensitive` | case-insensitive parse |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::parse_classification_unrelated_returns_none` | "unrelated" → None |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::parse_classification_garbage_returns_none` | stray values → None |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_deserializes_validating` | `misrepresentsSource` deserializes |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_deserializes_legacy_fairly_paraphrased_alias` | `fairlyParaphrased` alias still parses (backward compat) |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_defaults_misrepresents_to_false_when_absent` | omitted field → default false |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_exact_match_passes` | exact verbatim quote passes the grounding gate |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_case_insensitive_match_passes` | case differences tolerated |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_whitespace_collapse_match_passes` | whitespace-run differences tolerated (PDF extraction) |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_hallucinated_sentence_dropped` | non-substring (paraphrase/invention) dropped |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_mixed_grounded_and_hallucinated` | grounded survive, hallucinated dropped |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_empty_input_returns_empty` | empty quotes → empty output |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_empty_source_drops_all` | empty source → empty output |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_deduplicates_exact_dupes` | case-variant dupes deduped |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::ground_quotes_orders_by_source_position` | survivors reordered to source order |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_justifying_sentences_snake_case` | justifying_sentences snake_case deserializes |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_justifying_sentences_camel_case_alias` | justifyingSentences camelCase alias deserializes |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::llm_output_justifying_sentences_defaults_empty_when_absent` | omitted field → default empty vec |
| `src-tauri/tests/citation_finder/citation_finder_claim_split_test.rs::enforce_truncates_to_five` | >5 claims truncated |
| `src-tauri/tests/citation_finder/citation_finder_claim_split_test.rs::enforce_trims_whitespace` | per-claim trim |
| `src-tauri/tests/citation_finder/citation_finder_claim_split_test.rs::enforce_drops_empty_claims` | post-trim empty dropped |
| `src-tauri/tests/citation_finder/citation_finder_claim_split_test.rs::enforce_empty_input_returns_empty` | empty in → empty out |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::coverage_empty_corpus_is_full` | 0 articles → 100% |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::coverage_half` | 5/10 → 50% |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::coverage_zero_embedded` | 0/N → 0% |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::coverage_embedded_exceeds_total_clamps` | defensive clamp to 100% |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_anthropic_overrides_unknown_to_disabled` | Anthropic + un-probed → reports `disabled` (static override) |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_zai_overrides_unknown_to_disabled` | Z.AI + un-probed → reports `disabled` (static override) |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_openai_keeps_unknown_when_not_probed` | OpenAI + un-probed → stays `unknown` (no static override) |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_anthropic_overrides_persisted_enabled` | static override is authoritative: wins over stale persisted `enabled` |
| `src-tauri/tests/citation_finder/citation_finder_readiness_test.rs::compute_readiness_anthropic_keeps_persisted_disabled` | persisted `disabled` is a no-op for the static check |
| `src-tauri/tests/citation_finder/citation_finder_mod_test.rs::filter_valid_statuses_keeps_valid_three` | whitelist keeps the 3 valid statuses |
| `src-tauri/tests/citation_finder/citation_finder_mod_test.rs::filter_valid_statuses_drops_duplicate_status` | `duplicate` always dropped |
| `src-tauri/tests/citation_finder/citation_finder_mod_test.rs::filter_valid_statuses_empty_input_returns_empty` | empty → empty (no "all statuses" fallback) |
| `src-tauri/src/citation_finder/search.rs::normalize_claim_key_trims_and_lowercases` | claim-key normalization (case+trim) |
| `src-tauri/src/citation_finder/search.rs::normalize_claim_key_collapses_internal_whitespace` | whitespace-run collapse |
| `src-tauri/src/citation_finder/search.rs::merge_whole_block_uses_empty_claim_key` | whole-block cosine flows to confidence |
| `src-tauri/src/citation_finder/search.rs::merge_per_statement_handles_claim_whitespace_drift` | cosmetic claim drift does not lose cosine |
| `src-tauri/src/citation_finder/search.rs::merge_per_statement_handles_claim_case_drift` | case drift tolerated |
| `src-tauri/src/citation_finder/search.rs::merge_drops_hallucinated_article_id` | unknown article_id dropped |
| `src-tauri/src/citation_finder/search.rs::merge_drops_unrelated_and_garbage_classifications` | "unrelated"/garbage dropped |
| `src-tauri/src/citation_finder/search.rs::merge_truncates_to_ten` | 10-result cap |
| `src-tauri/src/citation_finder/search.rs::merge_confidence_negative_cosine_normalizes_correctly` | NEG_INFINITY seed preserves negative cosine |
| `src-tauri/src/citation_finder/search.rs::merge_confidence_missing_cosine_falls_to_neutral` | missing recall → 0.5 neutral |
| `src-tauri/src/citation_finder/search.rs::pool_finalists_dedups_article_ids_keeping_best_score` | union dedup |
| `src-tauri/src/citation_finder/search.rs::pool_finalists_truncates_to_fifteen` | 15-finalist cap (correlated cosine adds nothing) |
| `src-tauri/src/citation_finder/search.rs::pool_finalists_cosine_union_rescues_low_containment_article` | weak-containment/top-cosine article kept via the union |
| `src-tauri/src/citation_finder/search.rs::pool_finalists_caps_union_at_twenty` | union capped at 20 finalists |
| `src-tauri/src/citation_finder/search.rs::pool_finalists_filters_passages_to_finalist_set` | per-claim passages filtered to finalists (prompt hygiene) |
| `src-tauri/src/citation_finder/search.rs::cosine_best_chunk_resolves_valid_index` | provenance index → that chunk |
| `src-tauri/src/citation_finder/search.rs::cosine_best_chunk_title_abstract_row_is_none` | `-1` sentinel row is not a chunk |
| `src-tauri/src/citation_finder/search.rs::cosine_best_chunk_out_of_range_is_none` | stale out-of-range index → None |
| `src-tauri/src/citation_finder/search.rs::cosine_best_chunk_missing_provenance_is_none` | None provenance → None |
| `src-tauri/src/citation_finder/search.rs::merge_grounds_against_abstract_context` | justifying sentence quoted from the abstract context survives grounding |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_drift_tolerant_pipeline_contract` | external pin on the pub helper |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_empty_input_is_stable` | empty → "" (whole-block key) |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_does_not_strip_punctuation` | punctuation preserved (conservative) |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::sdil_claim_surfaces_with_thesis_evidence` | E2E SDIL reproduction: claim surfaces + thesis sentence grounded + abstract in prompt + funnel counts |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::paraphrased_claim_falls_back_to_cosine_chunk` | zero lexical overlap → cosine-chunk fallback keeps the article |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::unrelated_drop_is_visible_in_funnel` | `unrelated` classification counted in the funnel, not silent |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::cosine_union_keeps_semantically_strong_finalist` | 16th-by-containment article rescued into the LLM prompt |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::per_statement_mode_groups_by_claim` | per-statement grouping + funnel emission |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::empty_recall_reports_zero_funnel` | empty recall emits a zero-funnel event |
| `src-tauri/tests/citation_finder/citation_finder_prompt_test.rs::whole_block_prompt_renders_abstract_context` | chunk-backed candidate renders `- abstract` context after the passage; abstract-primary omits it |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::empty_filter_returns_all_statuses` | §7 API: empty filter = all rows |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::single_status_filter_matches_historical_behavior` | backward-compat single status |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::multi_status_filter_working_plus_included` | working+included excludes rejected/duplicate |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::pool_hits_tracks_winning_chunk_provenance` | max-pool reports the winning row's chunk_index |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::pool_hits_reports_title_abstract_row_when_it_wins` | `-1` sentinel reported when the title+abstract row wins |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::pool_hits_skips_dimension_mismatched_rows` | length-mismatched vectors skipped |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::pool_hits_orders_by_score_desc_then_id` | deterministic score-desc + id-asc ordering |
| `src-tauri/tests/embedding/embedding_director_test.rs::director_detects_model_mismatch_as_stale` | stored model differs from current → row marked stale (pins the silent zero-results fix) |
| `src-tauri/tests/embedding/embedding_director_test.rs::director_skips_fresh_rows_when_hash_matches` | hash + model both match → row skipped (AllFresh) |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::no_mismatch_when_stored_matches_current` | stored == current → None |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::no_mismatch_when_nothing_stored` | empty stored → None |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::mismatch_when_stored_differs_from_current` | stored != current → Some(stored) |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::mismatch_case_insensitive` | ASCII case differences are NOT a mismatch |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::mismatch_returns_first_offending_model_when_multiple_stored` | first non-matching model wins |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::mismatch_when_current_set_but_stored_empty` | empty stored model is a mismatch when current is known |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::no_mismatch_when_both_current_and_stored_empty` | both empty → None (nothing probed yet) |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::list_distinct_model_names_returns_unique_values` | DISTINCT model_name across rows |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::list_distinct_model_names_empty_when_table_empty` | empty table → empty vec |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::list_distinct_model_names_omits_null_and_empty` | NULL/empty model_name filtered out |
| `src-tauri/tests/embedding/embedding_model_mismatch_test.rs::delete_all_embeddings_clears_every_row` | DELETE FROM article_embeddings wipes all rows |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::cancel_during_hanging_classification_returns_cancelled` | Cancel interrupts a pending classify promptly (select! poll) with `Cancelled` |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::cancel_during_hanging_recall_returns_cancelled` | Cancel interrupts a pending recall promptly with `Cancelled` |
| `src-tauri/tests/citation_finder/citation_finder_pipeline_test.rs::cancel_after_completed_classification_returns_cancelled` | A classification that returned after the Cancel click is discarded (no done-with-results) |
| `src-tauri/tests/commands/citation_finder_guard_test.rs::guard_starts_and_clears_cancel` | A fresh run start clears the cancel token and marks `is_running` |
| `src-tauri/tests/commands/citation_finder_guard_test.rs::guard_returns_existing_snapshot_without_clearing_cancel` | A second Find while running returns the snapshot and keeps the cancel token set |

## TypeScript (Phase B - frontend)

| Test identifier | Assertion |
|-----------------|-----------|
| `src/__tests__/composables/use-citation-finder.test.ts::formatCitation_outputs_valid_string_per_style` | all 5 styles produce a parseable citation (consolidated) |
| `src/__tests__/composables/use-citation-finder.test.ts::findCitations_dispatches_command_and_listens_for_done` | IPC + event wiring (verifies find_citations invoked, send_chat_message not) |
| `src/__tests__/composables/use-citation-finder.test.ts::getModelMismatch_dispatches_command_and_returns_payload` | mismatch IPC wiring + payload shape |
| `src/__tests__/composables/use-citation-finder.test.ts::getModelMismatch_returns_null_when_no_mismatch` | null passthrough when no mismatch |
| `src/__tests__/composables/use-citation-finder.test.ts::regenerateEmbeddings_dispatches_scoped_command` | scoped regenerate IPC wiring |
| `src/__tests__/composables/use-citation-finder.test.ts::regenerateEmbeddings_passes_null_for_all_statuses` | null filter = all statuses |
| `src/__tests__/components/citation-result-card.test.ts::renders_metadata_passage_badge_confidence` | card layout contract: author + year + title rendered; journal/DOI hidden |
| `src/__tests__/components/citation-result-card.test.ts::sectionOrigin_null_omits_badge` | null section → no § badge |
| `src/__tests__/components/citation-result-card.test.ts::truncates_long_title_at_word_boundary_with_tooltip` | >65-char title → word-boundary prefix + `...` on-card, full title in the `title` attribute |
| `src/__tests__/components/citation-result-card.test.ts::hides_journal_and_doi_keeps_them_in_copy` | copy-only contract for journal + DOI |
| `src/__tests__/utils/chat-scroll.test.ts::scrolls_so_anchor_top_meets_container_top` | citation-results arrival pins the claim message to the scroll-area top (offset arithmetic) |
| `src/__tests__/utils/chat-scroll.test.ts::clamps_to_zero_when_anchor_is_above_container_top` | negative scroll targets clamp to 0 |
| `src/__tests__/utils/chat-scroll.test.ts::no_op_when_already_pinned` | pinned anchor → no scroll call |
| `src/__tests__/chat.test.ts::citation_finder_source_toggle` | 3rd source toggle works |
| `src/__tests__/chat.test.ts::sendMessage_branch_dispatches_find_citations` | citation branch does not call send_chat_message |
| `src/__tests__/chat.test.ts::clearChat_drops_citation_bubbles` | reset clears citations array |
| `src/__tests__/stores/chat.test.ts::citation_statuses_default_and_persist` | Articles-to-Search defaults + write-through + reload from localStorage |
| `src/__tests__/stores/chat.test.ts::citation_statuses_invalid_storage_falls_back_to_defaults` | Garbage/mis-shaped storage falls back to defaults (never throws) |
| `src/__tests__/stores/chat.test.ts::clears_citation_progress_when_the_search_command_rejects` | Command-reject path clears `citationProgress` like the terminal events |
| `src/__tests__/composables/use-citation-finder-chat.test.ts::status_change_persists_and_rechecks_readiness` | Selection change persists to the store and re-checks readiness under the new filter |
| `src/__tests__/composables/use-citation-finder.test.ts::rebase_uses_the_scope_universe_when_a_baseline_is_present` | Phase B runner counts are rebased onto the coverage baseline (10/25 + 1 -> 11/25, 40%) |
| `src/__tests__/composables/use-citation-finder.test.ts::rebase_never_moves_the_percent_backward_across_a_run` | The rebased percent is monotonic across a full run and reaches 90% |
| `src/__tests__/composables/use-citation-finder.test.ts::rebase_clamps_to_the_scope_total` | Cumulative done clamps at the scope total (no overshoot past 90%) |
| `src/__tests__/composables/use-citation-finder.test.ts::rebase_falls_back_to_run_counts_without_a_baseline` | No baseline -> run-relative counts stand alone |
| `src/__tests__/composables/use-citation-finder.test.ts::rebase_zero_total_is_safe` | Zero total -> 0% and no division by zero |
| `src/__tests__/composables/use-citation-finder.test.ts::regenerate_with_progress_streams_embedding_events_and_unlistens` | Regenerate subscribes to `embedding:progress`, forwards rebased updates, and always unlistens |
| `src/__tests__/composables/use-citation-finder-chat.test.ts::regenerate_reports_live_progress_until_completion` | The mismatch Regenerate exposes live progress while running and clears it on completion |
| `src/__tests__/components/citation-mismatch-dialog.test.ts::renders_regeneration_progress_while_running` | The dialog renders the live message + percent width while regenerating |
| `src/__tests__/components/citation-mismatch-dialog.test.ts::omits_progress_without_a_payload_and_relabels_the_button` | No payload -> no bar; the button reads "Regenerating…" |

## Notes

- `find_citations_inner` still depends on a live Tauri `State<DbState>` +
  `AppHandle` (Phases A/B need config reads + event emission), but its Phase-C
  core is extracted as `run_phase_c(&DbState, ...)`, which integration tests
  drive end-to-end with a mock `CitationLlmSender` against a seeded temp DB
  (`citation_finder_pipeline_test.rs`). The private pure helpers
  (`normalize_claim_key`, `merge_outputs`, `pool_finalists`,
  `cosine_best_chunk`) stay covered by the inline `search.rs` tests (the only
  inline block remaining - see the `src/citation_finder/search.rs::` rows
  above).
- The pure-helper tests (`similarity`, `prompt`, `claim_split`, `readiness`,
  `mod`) were extracted from inline `#[cfg(test)] mod tests` blocks into the
  external `src-tauri/tests/citation_finder_*_test.rs` files per
  `docs/CLAUDE.md` §Testing ("Avoid large inline unit tests in library source
  files").
