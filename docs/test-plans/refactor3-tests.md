# Refactor v3 (engine.rs decomposition) - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

Closes the test-coverage gaps recorded in `.worktrees/refactor_gaps.md`
(Gaps 5 + 6 + 7): the extracted pure modules (`decision.rs`,
`error_classify.rs`, `json_parse.rs`, `article_writer.rs`) now have direct
unit tests alongside the end-to-end screening tests that previously covered
them only transitively.

## `decision.rs` - `resolve_article_decision` + helpers (`decision_test.rs`)

Pure per-article decision pipeline (criterion-match -> finalize -> augment ->
override-annotate -> auto-label). Both stage-1 and stage-2 call this so the
pipeline lives in one place.

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_override_annotates_when_resolver_differs` | Resolver flips LLM decision -> reasoning carries `[App override: ...]` |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_override_absent_when_resolver_agrees` | No override annotation when resolver agrees with the LLM |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_custom_logic_honors_llm_exclude` | Custom logic in force -> LLM exclude is final (no resolver override) |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_custom_logic_honors_llm_include` | Custom logic in force -> LLM include is final |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_no_custom_logic_uses_priority_resolver` | No custom logic -> critical exclusion beats standard inclusion |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_augments_from_reasoning_global_numbers` | UUID mentioned in reasoning but absent from matched arrays is augmented |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_collects_auto_label_criteria` | Inclusion + Exclusion matches produce auto-label pairs |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_evidence_sections_from_map` | Evidence-sections label read from the enhanced-evidence map |
| `src-tauri/tests/screening/decision_test.rs::resolve_article_decision_evidence_sections_none_when_absent` | Abstract-mode (no evidence) yields `None` |
| `src-tauri/tests/screening/decision_test.rs::build_global_criterion_numbering_sequential_via_decision_module` | Inclusion `[1]..[N]` then exclusion `[N+1]..` |
| `src-tauri/tests/screening/decision_test.rs::augment_matched_from_reasoning_via_decision_module` | Moved helper still works via the decision module path |

## `error_classify.rs` - `classify_llm_error` branch coverage (`error_classify_test.rs`)

Direct unit tests for the counter-mutating decision tree (Gap 5). The leaf
classifiers (`is_transient_llm_error`, `is_auth_failure`) are exercised
transitively through `screening_engine_test.rs` (14 call sites); the
counter-mutation branches of `classify_llm_error` were previously untested.

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_non_transient_returns_hard_error` | Non-transient (malformed JSON) -> `HardError`, no counter bump |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_auth_failure_stops_immediately` | Plain 401 -> `Defer` with `should_stop` (threshold = 1) |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_consecutive_transient_threshold_stops` | 3 consecutive transients -> `should_stop` set on the 3rd |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_total_timeout_threshold_stops` | 3 total non-consecutive timeouts -> `should_stop` set |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_first_timeout_warns_slow_llm` | 1st timeout -> `warn_slow_llm = true`, no stop |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_transient_below_threshold_defers_only` | Transient below thresholds -> `Defer`, no stop, no warn |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_increments_consecutive_counter` | Each transient bumps `consecutive_transient_failures` |
| `src-tauri/tests/screening/error_classify_test.rs::classify_llm_error_increments_total_timeouts_for_timeout_only` | Timeout bumps `total_timeouts`; non-timeout transient does not |

## `json_parse.rs` - repair + parse helpers (`json_parse_test.rs`)

Direct unit tests for `repair_truncated_json_array` + `balance_braces` (Gap 6).
`extract_json` has 2 tests via `json_repair_test.rs`; the repair/brace helpers
were previously covered only transitively through end-to-end screening.

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/screening/json_parse_test.rs::repair_truncated_json_array_closes_incomplete_array` | Truncated array (no trailing `]`) -> closed at last `}` |
| `src-tauri/tests/screening/json_parse_test.rs::repair_truncated_json_array_none_for_already_complete` | Array ending in `]` -> `None` (no repair needed) |
| `src-tauri/tests/screening/json_parse_test.rs::repair_truncated_json_array_none_for_non_array` | Non-array input -> `None` |
| `src-tauri/tests/screening/json_parse_test.rs::balance_braces_appends_missing_closing` | Missing closing braces are appended |
| `src-tauri/tests/screening/json_parse_test.rs::balance_braces_prepends_missing_opening` | Missing opening braces are prepended |
| `src-tauri/tests/screening/json_parse_test.rs::balance_braces_noop_for_balanced` | Balanced input passes through unchanged |
| `src-tauri/tests/screening/json_parse_test.rs::balance_braces_ignores_braces_inside_string_literals` | Braces inside JSON string literals are not counted |
| `src-tauri/tests/screening/json_parse_test.rs::extract_json_strips_code_fence` | ```` ```json ```` fence stripped, inner array returned |
| `src-tauri/tests/screening/json_parse_test.rs::extract_json_passes_through_bare_array` | Bare array input returned as-is |
| `src-tauri/tests/screening/json_parse_test.rs::extract_json_extracts_array_from_wrapping_object` | Object wrapping an array -> array extracted |
| `src-tauri/tests/screening/json_parse_test.rs::process_screening_responses_normalizes_decision_case` | `"INCLUDE"` -> `"include"`; unexpected decision -> `"error"` |

## `article_writer.rs` - DB-write helpers (`article_writer_test.rs`)

Direct DB integration tests for `mark_batch_screening_error` (consolidates
the 3 verbatim batch-error loops) + `write_article_screening_result`
(stage-1/stage-2 per-article write).

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/db/article_writer_test.rs::mark_batch_marks_all_articles` | Every article in the batch gets `screening_error = 1` |
| `src-tauri/tests/db/article_writer_test.rs::mark_batch_with_raw_response` | Raw response included in the audit detail |
| `src-tauri/tests/db/article_writer_test.rs::mark_batch_empty_is_noop` | Empty batch -> no audit entries |
| `src-tauri/tests/db/article_writer_test.rs::write_result_updates_article_status` | `include` -> status `included` |
| `src-tauri/tests/db/article_writer_test.rs::write_result_creates_tags` | Suggested tags linked to the article |
| `src-tauri/tests/db/article_writer_test.rs::write_result_creates_auto_labels` | Auto-label criteria produce `Inclusion: ...` labels |
| `src-tauri/tests/db/article_writer_test.rs::write_result_saves_terms_when_flag_set` | `save_terms = true` -> extracted terms saved |
| `src-tauri/tests/db/article_writer_test.rs::write_result_skips_terms_when_flag_unset` | `save_terms = false` -> no terms saved |

## Post-landing gap coverage

Closes the three residual gaps surfaced in the post-landing audit (recorded in
`.worktrees/refactor3.md` §"Post-landing gaps"): the private `update_progress`
helper (tested through its public-API effect on `stage`/`stage_total`), the
`target_article_id` per-article-screening path (previously uncovered
user-facing flow), and the accepted transitive coverage decision for
`ScreeningPromptParts::build_prompt_input` (documented, not unit-tested).

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/screening/screening_two_stage_test.rs::update_progress_populates_stage_fields_in_two_stage_mode` | Private `update_progress` closure-mutation contract: `stage` + `stage_total` fields land correctly after a borderline two-stage run |
| `src-tauri/tests/screening/screening_e2e_test.rs::test_target_article_id_screens_only_that_article` | `RunSyncContext.target_article_id = Some(id)` screens only that article; others stay unscreened |
| `src-tauri/tests/screening/screening_e2e_test.rs::test_target_article_id_nonexistent_is_noop` | Nonexistent target UUID -> clean `Ok(())`, zero LLM calls, all articles unscreened |
| `src-tauri/tests/screening/screening_e2e_test.rs::test_target_article_id_already_screened_is_noop` | Re-targeting an already-screened article -> no-op, zero LLM calls |
