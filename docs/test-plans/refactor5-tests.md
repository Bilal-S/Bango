# Refactor v5 (`import_project` decomposition) - Test Inventory

Binding per `docs/CLAUDE.md` §Testing (Test-First Protocol).
Enforced by `scripts/check-test-inventory.sh` (wired into `npm run check:all`).

The `file::function` rows below are machine-checked: the script greps each
named test file for the listed function name. Any missing test blocks the PR.

Covers the ID-remap dedup paths that UNIQUE constraints prevent testing
through normal round-trips. Hand-edited backup JSON exercises the import
path directly (the source DB rejects duplicates at INSERT time).

## `project_backup_test.rs` - ID-remap dedup paths

| Test identifier | Assertion |
|---|---|
| `src-tauri/tests/export/project_backup_test.rs::import_reference_papers_dedup_via_doi` | 2 papers sharing a DOI -> second remapped via `paper_id_map`, not duplicated; downstream `article_reference_links` resolve to the surviving id |
| `src-tauri/tests/export/project_backup_test.rs::import_biblio_terms_dedup_composite_key` | 2 terms sharing `normalized_term` but differing `term_type` -> both survive (composite UNIQUE key); `biblio_article_terms` links to the correct one |