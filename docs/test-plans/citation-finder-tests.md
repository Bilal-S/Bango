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
| `src-tauri/src/citation_finder/search.rs::pool_finalists_truncates_to_fifteen` | 15-finalist cap |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_drift_tolerant_pipeline_contract` | external pin on the pub helper |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_empty_input_is_stable` | empty → "" (whole-block key) |
| `src-tauri/tests/citation_finder/citation_finder_search_test.rs::normalize_claim_key_does_not_strip_punctuation` | punctuation preserved (conservative) |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::empty_filter_returns_all_statuses` | §7 API: empty filter = all rows |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::single_status_filter_matches_historical_behavior` | backward-compat single status |
| `src-tauri/tests/embedding/embedding_recall_multistatus_test.rs::multi_status_filter_working_plus_included` | working+included excludes rejected/duplicate |
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

## TypeScript (Phase B - frontend)

| Test identifier | Assertion |
|-----------------|-----------|
| `src/__tests__/composables/use-citation-finder.test.ts::formatCitation_outputs_valid_string_per_style` | all 5 styles produce a parseable citation (consolidated) |
| `src/__tests__/composables/use-citation-finder.test.ts::findCitations_dispatches_command_and_listens_for_done` | IPC + event wiring (verifies find_citations invoked, send_chat_message not) |
| `src/__tests__/composables/use-citation-finder.test.ts::getModelMismatch_dispatches_command_and_returns_payload` | mismatch IPC wiring + payload shape |
| `src/__tests__/composables/use-citation-finder.test.ts::getModelMismatch_returns_null_when_no_mismatch` | null passthrough when no mismatch |
| `src/__tests__/composables/use-citation-finder.test.ts::regenerateEmbeddings_dispatches_scoped_command` | scoped regenerate IPC wiring |
| `src/__tests__/composables/use-citation-finder.test.ts::regenerateEmbeddings_passes_null_for_all_statuses` | null filter = all statuses |
| `src/__tests__/components/citation-result-card.test.ts::renders_metadata_passage_badge_confidence` | card layout contract |
| `src/__tests__/components/citation-result-card.test.ts::sectionOrigin_null_omits_badge` | null section → no § badge |
| `src/__tests__/chat.test.ts::citation_finder_source_toggle` | 3rd source toggle works |
| `src/__tests__/chat.test.ts::sendMessage_branch_dispatches_find_citations` | citation branch does not call send_chat_message |
| `src/__tests__/chat.test.ts::clearChat_drops_citation_bubbles` | reset clears citations array |

## Notes

- The async `find_citations_inner` entry point depends on a live Tauri
  `State<DbState>` + `AppHandle` and cannot be driven from a `#[test]` (same
  constraint documented in `tests/embedding/embedding_runner_test.rs`). The pipeline's
  testable decisions are extracted into pure helpers (`normalize_claim_key`,
  `merge_outputs`, `pool_finalists`). `normalize_claim_key` is `pub` and
  covered externally; `merge_outputs` + `pool_finalists` are private and
  covered by the inline `search.rs` tests (the only inline block remaining -
  see the `src/citation_finder/search.rs::` rows above).
- The pure-helper tests (`similarity`, `prompt`, `claim_split`, `readiness`,
  `mod`) were extracted from inline `#[cfg(test)] mod tests` blocks into the
  external `src-tauri/tests/citation_finder_*_test.rs` files per
  `docs/CLAUDE.md` §Testing ("Avoid large inline unit tests in library source
  files").
