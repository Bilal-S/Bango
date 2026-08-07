# OpenAlex Search Integration - Test Inventory

Binding test inventory for the OpenAlex Search Integration feature.
Enforced by `scripts/check-test-inventory.sh` via `npm run check:all` once
wired into the `PLAN_DOCS` array (add `docs/test-plans/openalex-tests.md`
when implementation begins).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

## Rust pure helpers + mapping (`openalex::mapping`, `openalex::search`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/openalex_mapping_test.rs::reconstruct_abstract_basic` | Inverted index with multiple words/positions reconstructs to correct string |
| `src-tauri/tests/openalex_mapping_test.rs::reconstruct_abstract_empty` | Empty `HashMap` returns empty string |
| `src-tauri/tests/openalex_mapping_test.rs::reconstruct_abstract_null_index` | `None` inverted index returns empty string |
| `src-tauri/tests/openalex_mapping_test.rs::truncate_snippet_word_boundary` | 250-char abstract truncates at last word boundary <= 200 chars + `...` |
| `src-tauri/tests/openalex_mapping_test.rs::truncate_snippet_under_200_no_ellipsis` | Abstract <= 200 chars returns as-is (no `...`) |
| `src-tauri/tests/openalex_mapping_test.rs::map_work_to_new_article_full` | Full Work object maps to `NewArticle` with all fields populated |
| `src-tauri/tests/openalex_mapping_test.rs::map_work_to_new_article_minimal` | Work with null biblio/source maps without panic |
| `src-tauri/tests/openalex_mapping_test.rs::map_work_strips_doi_prefix` | `https://doi.org/10.xxx/yyy` -> `10.xxx/yyy` (lowercased) |
| `src-tauri/tests/openalex_mapping_test.rs::map_work_publication_date_to_date_column` | `publication_date` maps to `NewArticle.date` |
| `src-tauri/tests/openalex_mapping_test.rs::map_work_eissn_differs_from_issn_l` | `eissn` is the first ISSN != `issn_l` |
| `src-tauri/tests/openalex_mapping_test.rs::deserialize_harvest_response_missing_fields` | Harvest response (missing `cited_by_count`/`keywords`) deserializes after `#[serde(default)]` fix |
| `src-tauri/tests/openalex_search_test.rs::build_search_url_basic_query` | Plain query builds correct URL with `search=` param |
| `src-tauri/tests/openalex_search_test.rs::build_search_url_has_abstract_always_on` | URL always contains `filter=has_abstract:true` |
| `src-tauri/tests/openalex_search_test.rs::build_search_url_is_retracted_default_off` | URL contains `is_retracted:false` by default |
| `src-tauri/tests/openalex_search_test.rs::build_search_url_url_encodes_query` | Special chars in query are percent-encoded |
| `src-tauri/tests/openalex_search_test.rs::build_search_url_with_filters` | Year range + type + OA filters compose correctly |

## Rust import pipeline (`commands::openalex`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/openalex_import_test.rs::import_single_openalex_article` | Single work inserts into `articles` with `import_source = "openalex"` |
| `src-tauri/tests/openalex_import_test.rs::import_openalex_runs_dedup_classify` | Import runs `classify_imported_articles` (non-dupes -> `working`) |
| `src-tauri/tests/openalex_import_test.rs::import_openalex_audit_entry` | Import writes `action = 'import'` audit entry |
| `src-tauri/tests/openalex_import_test.rs::import_openalex_duplicate_doi_skip` | Duplicate DOI is skipped (not double-inserted) |
| `src-tauri/tests/openalex_import_test.rs::check_dois_in_library_batch` | Batch DOI check returns correct subset |
| `src-tauri/tests/openalex_import_test.rs::harvest_referenced_works_batch` | Referenced works fetched + inserted into `reference_papers` + links created |

## Smart Search LLM prompt (`openalex::smart_search`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/openalex_smart_search_test.rs::build_smart_search_prompt_includes_aims` | Prompt embeds each research aim's text |
| `src-tauri/tests/openalex_smart_search_test.rs::build_smart_search_prompt_includes_criteria` | Prompt embeds inclusion + exclusion criteria |
| `src-tauri/tests/openalex_smart_search_test.rs::build_smart_search_prompt_states_char_limit` | System + user prompts state the 1500-char budget |
| `src-tauri/tests/openalex_smart_search_test.rs::build_smart_search_prompt_leverages_stemming` | Prompt warns against redundant synonyms/stems (OpenAlex stems natively) |
| `src-tauri/tests/openalex_smart_search_test.rs::build_smart_search_prompt_wildcard_discipline` | Prompt restricts wildcard usage to quoted multi-word phrases |
| `src-tauri/tests/openalex_smart_search_test.rs::parse_smart_search_response_valid_json` | Valid JSON fixture parses into `SmartSearchQuery` |
| `src-tauri/tests/openalex_smart_search_test.rs::parse_smart_search_response_malformed_json` | Malformed JSON yields `AppError` (not panic) |
| `src-tauri/tests/openalex_smart_search_test.rs::parse_smart_search_response_with_code_fences` | JSON wrapped in ```json fences still parses |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_under_limit_unchanged` | Short query returned verbatim (no-op under budget) |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_truncates_at_top_level_operator` | Over-long query cut at top-level group boundary; trailing group dropped |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_keeps_parens_balanced` | Result has matching `(`/`)` count |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_does_not_split_inside_phrase` | Cut never lands inside a quoted phrase (quotes stay balanced) |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_falls_back_to_whitespace` | Flat word list cut at last word boundary within budget |
| `src-tauri/tests/openalex_smart_search_test.rs::truncate_search_query_zero_max_returns_empty` | `max_len = 0` returns empty string |
| `src-tauri/tests/openalex_smart_search_test.rs::parse_smart_search_response_truncates_overlong_query` | 2000-char fixture parses to <= `MAX_SEARCH_QUERY_LEN` |

## Frontend store (`stores/openalex.ts`)

| Test identifier | Assertion |
|---|---|
| `src/__tests__/openalex-store.test.ts::search_updates_results` | Calling `search()` updates `results` + `totalCount` |
| `src/__tests__/openalex-store.test.ts::pagination_state` | Changing `perPage` / `currentPage` triggers re-search |
| `src/__tests__/openalex-store.test.ts::library_doi_check_greys_out` | Results with DOIs in library get `alreadyInLibrary = true` |
| `src/__tests__/openalex-store.test.ts::smart_search_mode_gated_on_llm_configured` | Smart Search button visible only when `smartSearchAvailable = true` |
| `src/__tests__/openalex-store.test.ts::reference_harvest_toggle_defaults_off` | `retrieveReferenceDetails` defaults to false |