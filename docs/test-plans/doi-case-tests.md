# DOI canonicalization + case-insensitive matching - binding test inventory

Binding `file::function` inventory for the DOI canonicalization plan
(`.worktrees/doifix.md`), enforced by `scripts/check-test-inventory.sh` via
`npm run check:all`.

## Tier 1 - normalization + case-insensitive comparisons

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/doi_test.rs::normalize_doi_strips_doi_org_prefix` | `https://doi.org/10.1/AbC` -> `10.1/abc` |
| `src-tauri/tests/doi_test.rs::normalize_doi_strips_prefix_mixed_case` | `HTTPS://DOI.ORG/10.1/x` -> `10.1/x` |
| `src-tauri/tests/doi_test.rs::normalize_doi_strips_dx_and_http_prefixes` | `http://dx.doi.org/10.1/x` -> `10.1/x` |
| `src-tauri/tests/doi_test.rs::normalize_doi_strips_doi_scheme_prefix` | `doi:10.1/AbC` and `doi: 10.1/AbC` -> `10.1/abc` |
| `src-tauri/tests/doi_test.rs::normalize_doi_lowercases` | `10.1/AbC` -> `10.1/abc` |
| `src-tauri/tests/doi_test.rs::normalize_doi_trims_whitespace` | `" 10.1/AbC "` -> `10.1/abc` |
| `src-tauri/tests/doi_test.rs::normalize_doi_filters_placeholders` | `NA`, `N/A`, `NULL`, `NONE`, `-` -> None |
| `src-tauri/tests/doi_test.rs::normalize_doi_empty_and_whitespace_none` | `""`, `"   "` -> None |
| `src-tauri/tests/doi_test.rs::normalize_doi_prefix_placeholder_to_none` | `doi: NA`, `doi: N/A`, `doi: -` -> None (strip-then-filter order) |
| `src-tauri/tests/doi_test.rs::normalize_doi_double_prefix_strips_once` | `https://doi.org/doi:10.1/x` -> `doi:10.1/x`; `doi:https://doi.org/10.2/y` -> `https://doi.org/10.2/y` |
| `src-tauri/tests/doi_test.rs::normalize_doi_scheme_multispace_separator` | `doi:  10.1/x` -> `10.1/x` |
| `src-tauri/tests/dedup_test.rs::doi_exact_match_case_insensitive` | Dedup pairs articles whose DOIs differ only in case |
| `src-tauri/tests/dedup_test.rs::doi_exact_match_ignores_url_prefix` | `https://doi.org/10.1/x` matches stored `10.1/x` |
| `src-tauri/tests/reference_test.rs::find_paper_by_doi_case_insensitive` | Lookup with different casing finds the stored paper |
| `src-tauri/tests/reference_test.rs::auto_match_paper_to_article_case_insensitive` | Promotion DOI match works across casing |
| `src-tauri/tests/reference_test.rs::find_unmatched_papers_by_doi_case_insensitive` | Post-import paper link works across casing |
| `src-tauri/tests/openalex_search_test.rs::check_dois_in_library_case_insensitive` | Mixed-case stored DOI is reported present, returned lowercase |
| `src-tauri/tests/openalex_mapping_test.rs::mapping_normalizes_doi_via_canonical_helper` | Mapped DOIs are lowercase, trimmed, prefix-stripped, placeholder-filtered |
| `src-tauri/tests/project_backup_test.rs::import_reference_papers_dedups_case_variant_doi` | Restore with differently-cased DOI remaps to existing paper, no duplicate row |
| `src-tauri/tests/project_backup_test.rs::import_articles_normalizes_legacy_doi` | Restored article with `https://doi.org/10.1/AbC` stores `10.1/abc` |
| `src-tauri/tests/article_metadata_test.rs::update_doi_normalizes_to_canonical_form` | Metadata edit stores the lowercase, prefix-stripped DOI |
| `src/__tests__/openalex-store.test.ts::library_doi_check_greys_out_case_variant` | Grey-out fires when stored DOI differs only in case |

## Tier 2 - migration v009

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/doi_case_migration_test.rs::migration_heals_mixed_case_and_prefixed_dois` | Mixed-case and `https://doi.org/`-prefixed `articles.doi` + `reference_papers.doi` become canonical |
| `src-tauri/tests/doi_case_migration_test.rs::migration_merges_case_variant_duplicate_papers` | Dupe papers collapse to one survivor; links remapped; dupes deleted |
| `src-tauri/tests/doi_case_migration_test.rs::migration_merge_preserves_match_state_and_counts` | Matched dupe wins survivorship; counters recounted from links |
| `src-tauri/tests/doi_case_migration_test.rs::migration_rebuilds_doi_index_case_insensitive` | Insert of a case-variant DOI violates the unique index |
| `src-tauri/tests/doi_case_migration_test.rs::migration_idempotent_on_canonical_data` | Re-running v009 `UP_SQL` on canonical data succeeds and changes nothing |
| `src-tauri/tests/doi_case_migration_test.rs::migration_nulls_prefix_placeholder_dois` | `doi: NA` / `doi: N/A` / `doi: -` heal to NULL in both tables |
| `src-tauri/tests/doi_case_migration_test.rs::migration_heals_double_prefixed_doi` | Exactly one prefix strip (URL wins / scheme-only), matching the helper |
| `src-tauri/tests/doi_case_migration_test.rs::migration_heals_multispace_scheme_separator` | `doi:  10.1/x` heals to `10.1/x` |
| `src-tauri/tests/doi_case_migration_test.rs::doi_lookup_uses_lower_doi_index` | The three DOI lookup shapes seek `uq_ref_papers_doi` (no SCAN) |

## Tier 3 - filename matching

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/batch_import_test.rs::full_text_phase_matches_case_variant_filename` | `{DOI}.pdf` with different casing attaches to the right article |
| `src-tauri/tests/batch_import_test.rs::citations_phase_matches_case_variant_ris_filename` | `{DOI}_references.ris` with different casing imports |
| `src-tauri/tests/batch_import_test.rs::citations_phase_directory_index_prefers_lowercase_named_file` | Case-variant siblings resolve to the exactly-lowercase-named file |
| `src-tauri/tests/batch_import_test.rs::find_file_case_insensitive_prefers_exact_match` | Exact-name path wins over a case-variant sibling |
| `src-tauri/src/batch_import/full_text_phase.rs::build_fulltext_match_map_keys_are_lowercase` | Map keys use lowercase DOI file stems |
