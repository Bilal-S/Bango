# Refactor v1 (code health) - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name.
Any missing test blocks the PR.

Source plan: `.worktrees/refactor1.md` §5.
Tier 0 characterization tests land FIRST: no production file named in the plan
is edited until its Tier 0 test exists, passes against current code, and is
committed.
Tiers 1-5 then refactor production code behind these pins.
Tier 0 Rust verification scope (agreed): `cargo clippy --test
tags_labels_test -- -D warnings` plus `cargo test --test tags_labels_test`.
The full `cargo test` runs at Tier 2 entry, the first tier that edits Rust
production code.

## Tier 0 characterization tests (new; snake_case names per plan §3.3)

Pin current behavior of every production file the refactor touches.
Graph quartet: loading/error rendering, hover tooltip content, cluster vs
temporal color modes, and citation isolation directions.
Controls trio: threshold slider payloads and the search filter emit.
`useExport`: the RIS export scaffold (dialog cancel, tab-export IPC payload,
invoke-error propagation) that T1.3 extracts.
`article-list`: keyboard navigation branches and route deep-link params, the
safety net for T1.7 and T4.2.
Rust: the normalized-name create dedupe contract for tags and labels before
the T2.2 repo consolidation.

| Test identifier | Assertion |
|---|---|
| `src/__tests__/components/citation-network-graph.test.ts::renders_loading_and_error_states` | loading and error banners render from props |
| `src/__tests__/components/citation-network-graph.test.ts::hover_shows_node_tooltip_with_counts` | hovered citation node shows refs/citations counts |
| `src/__tests__/components/citation-network-graph.test.ts::isolation_mode_focuses_ancestry_or_progeny` | isolation direction switches the focused subgraph |
| `src/__tests__/components/citation-network-graph.test.ts::color_mode_switch_updates_node_colors` | cluster vs temporal mode changes node colors |
| `src/__tests__/components/cocitation-network-graph.test.ts::renders_loading_and_error_states` | same contract for the cocitation graph |
| `src/__tests__/components/cocitation-network-graph.test.ts::hover_shows_node_tooltip` | hovered cocitation node tooltip content |
| `src/__tests__/components/cocitation-network-graph.test.ts::color_mode_switch_updates_node_colors` | color-mode parity |
| `src/__tests__/components/keyword-network-graph.test.ts::renders_loading_and_error_states` | same contract for the keyword graph |
| `src/__tests__/components/keyword-network-graph.test.ts::hover_shows_node_tooltip` | hovered keyword node tooltip content |
| `src/__tests__/components/network-graph.test.ts::renders_loading_and_error_states` | same contract for the coauthor base graph |
| `src/__tests__/components/network-graph.test.ts::color_mode_switch_updates_node_colors` | color-mode parity |
| `src/__tests__/components/citation-controls.test.ts::threshold_sliders_emit_updated_values` | minPapers/minLinkStrength sliders emit payloads |
| `src/__tests__/components/citation-controls.test.ts::search_input_emits_filter_string` | search box emits the filter string |
| `src/__tests__/components/cocitation-controls.test.ts::threshold_sliders_emit_updated_values` | slider contract parity |
| `src/__tests__/components/keyword-controls.test.ts::threshold_sliders_emit_updated_values` | slider contract parity |
| `src/__tests__/composables/use-export.test.ts::export_ris_returns_false_when_dialog_cancelled` | cancel path: no invoke, `exporting` resets |
| `src/__tests__/composables/use-export.test.ts::export_ris_for_tab_passes_status_and_errors_flag` | tab export invokes with status + screeningErrorsOnly |
| `src/__tests__/composables/use-export.test.ts::export_ris_reports_invoke_error` | error path sets message and returns false |
| `src/__tests__/views/article-list.test.ts::keyboard_navigation_moves_selection` | arrow/page/home/end keys move selection, wrap pinned |
| `src/__tests__/views/article-list.test.ts::route_deep_link_params_apply_filters` | route query params drive initial filter state |
| `src-tauri/tests/tags_labels_test.rs::create_tag_dedupes_normalized_name` | normalized-name create dedupes to the existing tag |
| `src-tauri/tests/tags_labels_test.rs::create_label_dedupes_normalized_name` | normalized-name create dedupes to the existing label |

## Tier 4 tests (added with the decomposition; same binding rule)

| Test identifier | Assertion |
|---|---|
| `src/__tests__/utils/article-deep-links.test.ts::empty_query_yields_no_params_and_no_flags` | no params -> all flags false |
| `src/__tests__/utils/article-deep-links.test.ts::filter_params_are_parsed_and_flagged` | full filter deep-link parses + sets hasFilterParams |
| `src/__tests__/utils/article-deep-links.test.ts::article_id_only_does_not_set_filter_flag` | articleId-only deep-link skips filter path |
| `src/__tests__/utils/article-deep-links.test.ts::numeric_flags_are_strictly_one` | filterCollapsed/resetFilters only fire on literal '1' |
| `src/__tests__/utils/article-deep-links.test.ts::non_finite_years_do_not_set_filter_flag` | NaN years kept raw but excluded from hasFilterParams |
| `src/__tests__/utils/article-deep-links.test.ts::non_string_values_are_ignored` | arrays/null query values ignored |
| `src/__tests__/utils/article-deep-links.test.ts::empty_comma_list_parses_to_single_empty_string_entry` | `?tags=` parity with the old view parser |

## Existing regression anchors (already green; informational for the checker)

| Test identifier | Guards |
|---|---|
| `src-tauri/tests/summary_repo_test.rs::save_is_upsert_overwriting_existing` | T2.1 upsert semantics |
| `src-tauri/tests/summary_repo_test.rs::clear_summary_removes_row` | T2.1 clear semantics |
| `src-tauri/tests/gap_analysis_repo_test.rs::gap_analysis_enforces_single_row` | T2.1 single-row contract |
| `src-tauri/tests/gap_analysis_repo_test.rs::save_get_round_trip` | T2.1 round trip |
| `src/__tests__/utils/graph-filters.test.ts` (file-level; names contain spaces so the checker does not extract them) | T1.4 threshold predicates |
| `src/__tests__/components/tag-chip.test.ts` + `label-chip.test.ts` (file-level) | T1.5 chip parity |
| `src/__tests__/composables/use-summary.test.ts` + `use-gap-analysis.test.ts` (file-level) | T1.6 scaffold parity |
