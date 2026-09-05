# src-tauri/tests/

## Purpose

Rust integration tests: one test binary per area under `tests/<area>/`
(`<area>/main.rs` declares `mod <file>;` for each moved test file), plus the
extraction home for inline `#[cfg(test)] mod tests` blocks (keeps source
files compact; helpers tested externally are `pub`).

## Ownership

- These are separate crates that the `src-tauri/src/lib.rs` test-aware clippy
  attributes do not reach, so `unwrap()` / `expect()` / `panic!()` are allowed
  here (see `docs/CLAUDE.md` §Error Handling).
- RIS fixture data for citation/reference tests lives in `tests/test-citations/`
  (indexed in the root `AGENTS.md` Child DOX Index).
- PDF test assets live in `tests/assets/` (repo root, referenced via
  `../tests/assets/...` relative to the `src-tauri` package cwd).

## Local Contracts

### Area binaries (test-speed contract)

- Test files live in area dirs: `llm/`, `screening/`, `translation/`,
  `embedding/`, `citation_finder/`, `wiki/`, `biblio/`, `zotero/`,
  `openalex/`, `scraping/`, `export/`, `db/`, `ris/`, `bibtex/`, `dedup/`,
  `prisma/`, `utils/`, `crypto/`, `commands/`, `batch_import/`, `summary/`,
  `models/`.
- Each area dir has a `main.rs` listing `mod <file>;` for every file in it.
  Cargo auto-discovers `tests/<dir>/main.rs` as one test target per area.
  NEVER add a new top-level `src-tauri/tests/*.rs` file - it becomes its own
  ~450MB binary and regresses the relink + disk contract.
- New test file: put it in the matching area dir and register its `mod` line
  in that area's `main.rs` (sorted order).
- Area choice follows the `src/` module being tested (e.g. a test of
  `src/llm/client.rs` goes in `tests/llm/`).

### Fast/slow split

- Default `cargo test` runs only fast tests.
- Slow tests are tagged `#[ignore = "slow"]`; each tag MUST be registered in
  `slow-manifest.toml` (checked by `scripts/check-slow-manifest.sh`, wired
  into `npm run check:all`).
- Tagging rule: a test is slow when its cost is dominated by real sleeps,
  LLM retry backoff, polls, or PBKDF2 key derivation.
- `scripts/rust-test.sh` runs the suites: default fast, `--full`,
  `--changed [base]`, `--live` (see `docs/CLAUDE.md` §Testing).

### Test inventory

Repository/KPI tests live in `biblio/biblio_repo_tests.rs` (in-memory SQLite via
`run_migrations`). Network builder & serializer unit tests live in
`biblio/biblio_networks_test.rs`. Unit-test extractions:
`biblio/biblio_normalizer_test.rs`, `biblio/biblio_models_test.rs`,
`bibtex/bibtex_parser_test.rs`, `bibtex/bibtex_converter_test.rs`,
`ris/cr_parser_test.rs`, `ris/doi_test.rs`, `ris/n1_parser_test.rs`,
`screening/screening_engine_test.rs`, `utils/pdf_extract_test.rs`,
`scraping/browser_test.rs`. Co-citation integration tests
against RIS fixtures live in `biblio/cocitation_data_test.rs`.
`biblio/biblio_needs_refresh_test.rs` covers the staleness-flag round-trip.
`translation/auto_translate_test.rs` covers the experimental auto-translate toggle.
`export/legacy_upgrade_test.rs` covers the full legacy upgrade round-trip.
`export/project_backup_test.rs` covers the ProjectBackup export/import round-trip incl.
`full_text_ai_summary` blob preservation (inventory: `docs/test-plans/exim-tests.md`).
`export/reset_project_test.rs` covers `reset_project_inner` (delete-all-data +
VACUUM + wiki-root wipe). `prisma/prisma_report_test.rs` covers the screening
reasons report (primary-reason attribution, general buckets,
multi-assignment counts, Markdown rendering). `wiki/wiki_consolidation_test.rs` +
`wiki/wiki_index_drift_test.rs` cover the wiki pipelines. `utils/sections_test.rs` +
`utils/chunking_test.rs` cover the utils text-classification + chunking.
`biblio/biblio_cluster_themes_test.rs` covers the cluster thematic analysis pure
helpers (resolution dispatcher, three-source term resolution, Top-N cap,
link protocols, prompt builder); binding inventory:
`docs/test-plans/cluster-themes-tests.md`.
`db/doi_case_migration_test.rs` covers migration v009 (DOI canonicalization:
healing, duplicate-paper merge with match-state preservation, index rebuild,
idempotency); binding inventory: `docs/test-plans/doi-case-tests.md`.
Zotero integration: `zotero/zotero_mapping_test.rs` + `zotero/zotero_client_test.rs`
(Tier 1 pure mapping/path/parse), `zotero/zotero_connection_test.rs` (Tier 2
status mapping + collections + preview against mockito),
`zotero/zotero_import_test.rs` (Tier 3 canonical import sequence, key-based
exclusion, version guard, capacity guard, attachment phase),
`zotero/zotero_export_mapping_test.rs` + `zotero/zotero_export_test.rs` (Tier 5 export
mapping + DOI diff + preview counts), `zotero/zotero_write_client_test.rs`
(envelope/authorize/upload parsing, write-error classification, batch
tokens, stored-key reuse policy, mid-run key-expiry abort); binding
inventory: `docs/test-plans/zotero-tests.md`.

## Work Guidance

- New features ship with integration tests here; when a plan doc carries a
  Test Inventory section it is binding (see `docs/CLAUDE.md` Test-First
  Protocol + `scripts/check-test-inventory.sh`).
- The 3 `#[ignore]` real-PDF tests in `utils/sections_test.rs` are known
  broken: multi-column PDF flattening merges the `Results` heading mid-line,
  so the classifier misses it. Fix `pdf_extract` layout handling or
  `sections::classify_sections` before re-enabling them. Run them with
  `cargo test --test utils -- --ignored sections_test`.

## Verification

`npm run test:rust` (fast) / `npm run test:rust:full` (complete) from the repo
root; or `cargo test` from `src-tauri/`.

## Child DOX Index

No child `AGENTS.md` files (`test-citations/` fixture data is indexed in the
root `AGENTS.md`).
