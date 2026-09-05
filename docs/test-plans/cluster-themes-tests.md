# Cluster Thematic Analysis - Test Inventory (cluster1.md §Testing)

Consumed by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).
Rows use the machine-parseable `` `path::fn` `` format the script's regex
expects. All Rust tests live in the external crate
`src-tauri/tests/biblio/biblio_cluster_themes_test.rs` (per `docs/CLAUDE.md`
§Testing and `src-tauri/tests/AGENTS.md`); the pure helpers in
`biblio/thematic.rs` are `pub` + `#[must_use]`. Source plan:
`.worktrees/cluster1.md` (temporary planning doc - never referenced from
production code).

## Rust

| Test identifier | Assertion |
|-----------------|-----------|
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::build_cluster_themes_prompt_coauthorship_contains_members_and_articles` | prompt names members and resolved articles |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::build_cluster_themes_prompt_keyword_contains_terms_and_articles` | prompt names terms and resolved articles and omits the author link protocol |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::build_cluster_themes_prompt_forbids_em_dash` | no U+2014 in prompt |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::system_prompt_requires_thematic_sections` | Overview, Main Themes, Representative Articles present |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::build_cluster_themes_prompt_states_cap_when_truncated` | disclosure line names exact capped/total counts |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_authors_to_articles_returns_included_only` | excluded/non-included articles are filtered out |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_terms_to_articles_returns_included_only` | included scope enforced |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_terms_to_articles_dedupes_across_terms` | shared articles appear once |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_terms_to_articles_matches_tags_and_labels_sources` | tag- and label-sourced terms resolve via article_tags/article_labels |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_articles_applies_top_n_cap_and_ranking` | num_cited DESC then year DESC ranking, cap at 40 |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_members_dispatches_by_network_type` | dispatcher routes author vs term resolvers |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::resolve_members_rejects_unsupported_network_type` | citation/cocitation/bibliographic-coupling variants return Validation |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::link_protocols_per_network_restrict_author_links` | co_occurrence yields the article protocol only |
| `src-tauri/tests/biblio/biblio_cluster_themes_test.rs::build_cluster_themes_prompt_truncates_authors_and_skips_empty_lines` | Authors/Keywords lines truncate on a word boundary; empty values omit the line |

## TypeScript

| Test identifier | Assertion |
|-----------------|-----------|
| `src/__tests__/utils/cluster-members.test.ts::collectClusterMembers_returns_matching_nodes` | matching nodes collected with id and label |
| `src/__tests__/utils/cluster-members.test.ts::collectClusterMembers_ignores_unclustered_nodes` | null cluster and other clusters excluded |
| `src/__tests__/stores/cluster-themes.test.ts::cache_keyed_by_network_and_cluster` | distinct keys per network/cluster |
| `src/__tests__/stores/cluster-themes.test.ts::invalidate_clears_cache` | invalidate empties state |
| `src/__tests__/stores/cluster-themes.test.ts::drops_stale_result_after_invalidate` | late resolve for an invalidated key is discarded |
| `src/__tests__/stores/cluster-themes.test.ts::analyze_reuses_cached_result_without_recalling` | cached cluster redisplays without a second LLM call |
| `src/__tests__/stores/cluster-themes.test.ts::analyze_skips_duplicate_inflight_call` | second click while in flight makes no duplicate call |
| `src/__tests__/stores/cluster-themes.test.ts::analyze_retries_after_error` | errored entry is not served from cache; next click retries |
| `src/__tests__/stores/cluster-themes.test.ts::analyze_drops_stale_response_when_key_reanalyzed` | stale response for a replaced key is discarded; fresh result lands |
| `src/__tests__/composables/use-cluster-themes.test.ts::analyze_invokes_command_and_caches` | command result cached |
| `src/__tests__/composables/use-cluster-themes.test.ts::invalidate_watch_uses_array_of_getters` | recalculateTrigger bump clears entries |
| `src/__tests__/composables/use-cluster-themes.test.ts::reanalyze_forces_fresh_llm_call` | re-analyze bypasses the session cache and re-invokes the command |
| `src/__tests__/components/cluster-legend.test.ts::renders_analyze_button_only_when_single_cluster_and_llm_ready` | button visibility follows selection count and gate |
| `src/__tests__/components/cluster-themes-panel.test.ts::renders_author_and_article_links_as_clickable` | link protocols converted |
| `src/__tests__/components/cluster-themes-panel.test.ts::renders_unknown_link_protocol_as_plain_text` | no href emitted for unknown protocols |
| `src/__tests__/utils/cluster-members.test.ts::collectClusterMembers_skips_hidden_nodes` | hidden (filtered-out) nodes excluded from member collection |
| `src/__tests__/composables/use-cluster-themes.test.ts::copyMarkdown_reports_outcome_via_toast` | clipboard success/failure surfaces via toast, no unhandled rejection |
| `src/__tests__/components/cluster-themes-panel.test.ts::escapes_raw_html_from_llm_output` | raw LLM HTML is escaped; no img/script elements |