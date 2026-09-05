# Search Strategy Builder - Test Inventory

Binding test inventory for the Search Strategy Builder feature (spec §8.4).
Enforced by `scripts/check-test-inventory.sh` via `npm run check:all`.

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function/`it(` name. Any missing test blocks
the PR.

## Pure helpers (`commands::search_strategy`)

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_includes_aims_text` | The user prompt embeds each research aim's text. |
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_includes_all_eight_databases` | The system prompt names all 8 Boolean databases (PubMed, Scopus, Web of Science, Cochrane, EBSCOhost, JSTOR, ScienceDirect, arXiv). |
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_includes_arxiv_andnot` | The system prompt carries the arXiv-specific `ANDNOT` operator guidance. |
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_includes_semantic_scholar_advisory` | The system prompt emits the Semantic Scholar non-Boolean advisory. |
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_handles_empty_criteria` | The prompt builds successfully when both criteria slices are empty (aims-only is a valid input). |
| `src-tauri/tests/commands/search_strategy_test.rs::parse_response_parses_valid_eight_database_fixture` | A complete JSON fixture with all 8 database fields deserializes into `SearchStrategyResult`. |
| `src-tauri/tests/commands/search_strategy_test.rs::parse_response_returns_error_on_malformed_json` | Malformed JSON yields an `AppError` (not a panic). |
| `src-tauri/tests/commands/search_strategy_test.rs::parse_response_tolerates_code_fences` | After `send_json` migration, fence-stripping moved upstream into `prepare_llm_json`; this test confirms the parse fn handles the post-`prepare_llm_json` cleaned payload. |
| `src-tauri/tests/commands/search_strategy_test.rs::build_prompt_includes_harmonization_guidance` | The user prompt warns that negating exclusions are redundant and must not be encoded as self-canceling NOT clauses. |
