# src-tauri/tests/

## Purpose

Rust integration tests: one self-contained crate per `.rs` file, plus the
extraction home for inline `#[cfg(test)] mod tests` blocks (keeps source
files compact; helpers tested externally are `pub`).

## Ownership

- These are separate crates that the `src-tauri/src/lib.rs` test-aware clippy
  attributes do not reach, so `unwrap()` / `expect()` / `panic!()` are allowed
  here (see `docs/CLAUDE.md` §Error Handling).
- RIS fixture data for citation/reference tests lives in `tests/test-citations/`
  (indexed in the root `AGENTS.md` Child DOX Index).

## Local Contracts

### Test inventory

Repository/KPI tests live in `biblio_repo_tests.rs` (in-memory SQLite via
`run_migrations`). Network builder & serializer unit tests live in
`biblio_networks_test.rs`. Unit-test extractions:
`biblio_normalizer_test.rs`, `biblio_models_test.rs`,
`bibtex_parser_test.rs`, `bibtex_converter_test.rs`, `cr_parser_test.rs`,
`doi_test.rs`, `n1_parser_test.rs`, `screening_engine_test.rs`,
`pdf_extract_test.rs`, `browser_test.rs`. Co-citation integration tests
against RIS fixtures live in `cocitation_data_test.rs`.
`biblio_needs_refresh_test.rs` covers the staleness-flag round-trip.
`auto_translate_test.rs` covers the experimental auto-translate toggle.
`legacy_upgrade_test.rs` covers the full legacy upgrade round-trip.
`reset_project_test.rs` covers `reset_project_inner` (delete-all-data +
VACUUM + wiki-root wipe). `wiki_consolidation_test.rs` +
`wiki_index_drift_test.rs` cover the wiki pipelines. `sections_test.rs` +
`chunking_test.rs` cover the utils text-classification + chunking.

## Work Guidance

- New features ship with integration tests here; when a plan doc carries a
  Test Inventory section it is binding (see `docs/CLAUDE.md` Test-First
  Protocol + `scripts/check-test-inventory.sh`).
- Each file compiles into a ~450MB binary - mind the disk-space contract in
  `src-tauri/src/AGENTS.md` §Verification.

## Verification

`cargo test` from `src-tauri/`.

## Child DOX Index

No child `AGENTS.md` files (`test-citations/` fixture data is indexed in the
root `AGENTS.md`).
