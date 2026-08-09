# src-tauri/src/

## Purpose

Rust backend (Tauri 2.x). All application logic: database, AI/LLM features
(screening, summaries, wiki, embeddings, citation finder), import/export,
bibliometrics, scraping, translation, and the Tauri command layer.

## Ownership

- App entry is `lib.rs` (`run()`), which registers all `#[tauri::command]`
  handlers in one `invoke_handler!` list.
- Owns the article state machine, the hard-delete cascade, the journal-index
  loader, and the legacy startup upgrade path (contracts below).
- Module-specific contracts live in the Child DOX Index below.

## Local Contracts

### Article state machine (§4.2 of the spec)

**Moving an article back to `working` from any other status
(`included`/`rejected`/`duplicate`) always resets the screening flags
(`screened_at = NULL`, `screening_error = 0`)** so the article becomes eligible
for re-screening on the next run. Both `update_article_status` and
`bulk_update_article_status` enforce this rule. Without the reset the stale
`screened_at` timestamp survives the status change and excludes the article
from `get_next_unscreened_working_batch`, leaving it stuck in a "previously
screened" limbo that surfaces in the Error tab even though `screening_error` is
`0`. The audit entry notes "(screening flags reset for re-screening)" when the
reset fires. Tested in `tests/status_transition_screening_flags_test.rs`.

### Article hard-delete cascade (`article_repo::delete_article`)

Surfaced via the `delete_article` Tauri command + the red trashcan icon in
`detail-header.vue`). Runs in a single transaction and cleans up ALL related
data. `ON DELETE CASCADE` (enabled via `PRAGMA foreign_keys=ON` on every
connection) auto-removes `article_tags`, `article_labels`, `audit_entries`,
`article_reference_links`, `article_chunks`, `article_original_content`,
`article_original_chunks`, `biblio_article_authors`,
`biblio_author_affiliations`, `biblio_article_terms`. Two FKs lack an
`ON DELETE` clause and are cleaned explicitly BEFORE the `DELETE`:
`articles.duplicate_of` (self-ref - nulled so duplicates are un-merged) and
`reference_papers.matched_article_id` (cleared). Shared reference papers
(linked to other articles) are preserved; orphaned unmatched papers (zero
links + `match_status = 'unmatched'`) are deleted. The `match_status` reset to
`'unmatched'` for previously-matched papers runs AFTER the orphan sweep so a
matched paper with zero links survives the sweep and goes back to the
unmatched pool for re-matching instead of being hard-deleted. On-disk
full-text files are removed (non-fatal on failure). Sets the biblio + wiki
staleness flags. Frontend confirmation dialog owned by
`article-detail-panel.vue`; `useArticleSearch().deleteArticle` invokes the
command and closes the detail panel. Tested in `tests/article_delete_test.rs`.

### Journal-index loader (`lib.rs::load_journal_index_from_path`)

`pub` so `tests/journal_index_load_test.rs` can drive it directly. Copies the
bundled portal DB rows into the empty target `journal_index` using **two
separate connections** - a `SQLITE_OPEN_READ_ONLY` source and the target's own
`unchecked_transaction` - NOT `ATTACH DATABASE`. The previous `ATTACH` +
`INSERT...SELECT FROM portal` implementation failed on Windows when the
bundled source was WAL-mode (SQLite could not acquire the cross-database lock
inside the target's transaction). Resource resolution is 3-tier
(`resource_dir()` → `<exe_dir>/resources/` →
`CARGO_MANIFEST_DIR/resources/`); the loader is invoked at startup
(best-effort, audit-error on failure) and after `reset_project` (blocking,
`Err` on failure so the frontend Toasts). Tested in
`tests/journal_index_load_test.rs` (7 tests incl. the WAL-mode regression +
read-only-source guarantee).

Auto-loads the bundled `journal_index.db` on first startup, and shows a native
modal dialog (via `tauri-plugin-dialog`) if `run_migrations` fails in
`.setup()` - the message names the resolved `app_data_dir` path and the three
database files (`bango.db`, `bango.db-wal`, `bango.db-shm`) to back up or
delete before restarting.

### Platform DB paths (`BonCode.Bango` identifier)

- Windows: `%APPDATA%\BonCode.Bango\bango.db`
- macOS: `~/Library/Application Support/BonCode.Bango/bango.db`
- Linux: `~/.local/share/BonCode.Bango\bango.db`

### `commands/startup.rs`

Exposes `get_startup_status` and `perform_legacy_upgrade` (one-shot:
`export_legacy_project` -> write backup to `app_data_dir` -> `rebuild_schema`
-> journal reload -> `import_project`; backup file is never deleted).
**Loop-safety**: a webview `window.location.reload()` runs in the same Rust
process, so managed state is not recomputed. To prevent an endless reload loop
after a successful upgrade, `get_startup_status` re-probes the LIVE schema on
every call (falling back to the setup-time snapshot only if the live probe
errors), and `perform_legacy_upgrade` updates the managed `StartupStatus`
snapshot (now a `Mutex<SchemaStatus>`) post-success. Pure decision logic lives
in `legacy_upgrade_needed(live, fallback)`; the frontend adds a third
sessionStorage-based guard in `use-startup-upgrade.ts`.

### `commands/tags.rs` + `commands/labels.rs`

Tag & Label management commands (v6.9 standard-taxonomy surfacing). `tags.rs`
owns `STANDARD_STUDY_TAGS` (20 methodology/study-type tags) injected into the
`suggest_tags` prompt as a `## Standard Study-Type Tags` section instructing
the LLM to include up to 4 when relevant. `labels.rs` owns
`STANDARD_WORKFLOW_LABELS` (12 workflow-state labels) injected into the
`suggest_labels` prompt similarly. All standard entries are pre-validated to
pass the 35-char `sanitize_tag_or_label_name` gate (see `screening/AGENTS.md`)
so the backend sanitizer never silently truncates them.

**Staleness-flag contract (bugfix)**: `delete_tag` and `delete_label` set both
`mark_biblio_needs_refresh` + `mark_wiki_needs_refresh` after the repo delete.
These were previously the only two tag/label mutation paths that omitted the
flags, silently desyncing the keyword co-occurrence network
(`biblio_repo/networks/keywords.rs`) and the wiki concept hubs
(`wiki/ingest/concepts.rs`) after a delete - every other tag/label mutation in
`commands/articles.rs` already set them. Tested in `tests/tags_labels_test.rs`.

**Merge ("Replace with...") contract**: `merge_tag` / `merge_label` commands
delegate to `pub fn merge_tag_inner` / `merge_label_inner` (testable without
`State<DbState>`). Each runs inside one `unchecked_transaction`: compute
overlap count BEFORE the destructive `UPDATE OR IGNORE`, call
`tag_repo::merge_tags` / `label_repo::merge_labels` (CASCADE removes overlap
junction rows), write one coalesced `tag_remove` / `label_remove` audit entry
per *reassigned* article via the shared `audit_repo::write_tag_label_audit`
helper (single-entry bulk pattern; detail string
`Replaced "A" -> "B" (merge)` carries both halves), bump `changed_at`, set both
staleness flags. The `MergeResult` (`reassigned_count` excludes co-occurrence
overlaps; `already_had_survivor_count` reports them separately). The
pre-confirm dialog shows an honest upper bound (`from.articleCount`); the real
counts surface in the success toast. Tested in `tests/merge_tags_labels_test.rs`
(15 tests incl. `merge_tag_no_dangling_overlap_rows` CASCADE regression +
`merge_tag_chain_safe` chained-merge safety).

The shared `audit_repo::write_tag_label_audit` helper is the canonical loop for
multi-article tag/label audit entries, reused by both the bulk commands (via
`write_bulk_tag_label_audit` in `commands/articles.rs`) and the merge commands.

### `src-tauri/tests/` - Rust integration tests

Inline `#[cfg(test)] mod tests` blocks are extracted here to keep source files
compact (helpers tested externally are `pub`). Repository/KPI tests live in
`biblio_repo_tests.rs` (in-memory SQLite via `run_migrations`). Network builder
& serializer unit tests live in `biblio_networks_test.rs`. Unit-test
extractions: `biblio_normalizer_test.rs`, `biblio_models_test.rs`,
`bibtex_parser_test.rs`, `bibtex_converter_test.rs`, `cr_parser_test.rs`,
`doi_test.rs`, `n1_parser_test.rs`, `screening_engine_test.rs`,
`pdf_extract_test.rs`, `browser_test.rs`. Co-citation integration tests against
RIS fixtures live in `cocitation_data_test.rs`. `biblio_needs_refresh_test.rs`
covers the staleness-flag round-trip. `auto_translate_test.rs` covers the
experimental auto-translate toggle. `legacy_upgrade_test.rs` covers the full
legacy upgrade round-trip. `reset_project_test.rs` covers `reset_project_inner`
(delete-all-data + VACUUM + wiki-root wipe). `wiki_consolidation_test.rs` +
`wiki_index_drift_test.rs` cover the wiki pipelines. `sections_test.rs` +
`chunking_test.rs` cover the utils text-classification + chunking.

### Criteria harmonization (inclusion/exclusion division of labor)

Inclusion criteria define the SCOPE of a review; exclusion criteria define
INDEPENDENT removal reasons that would otherwise pass the inclusion filter
(publication type, language, animal/in-vitro-only studies, duplicates). An
exclusion criterion must NEVER merely negate an inclusion criterion: the
screening engine already excludes any article that matches no inclusion, so a
negating exclusion is doubly redundant AND bloats search-strategy queries with
self-canceling NOT clauses that fail to run.

Enforced across three prompt builders (each a `pub fn` pure helper with binding
tests):
- `commands::criteria::build_criteria_generation_prompt` - surfaces existing
  opposite-type criteria, caps exclusions lower (6 vs 8), and carries the
  "do not negate" guidance.
- `commands::criteria::build_check_rules_prompt` - the holistic "review my
  ruleset" review flags negations in ALREADY-EXISTING criteria and recommends
  deleting them (catches the generation guard's blind spot).
- `commands::search_strategy::build_search_strategy_prompt` - tells the LLM to
  drop negating exclusions rather than encoding them as self-canceling NOT
  clauses.
Binding inventory: `docs/test-plans/criteria-generation-tests.md` and
`docs/test-plans/search-strategy-tests.md` (enforced by
`scripts/check-test-inventory.sh` via `npm run check:all`).

## Work Guidance

- All LLM calls MUST flow through `LlmOrchestrator` (see `llm/AGENTS.md`),
  never `client::send_chat_completion` directly from command handlers.
- All `DbState.conn` locks MUST route through `db::lock_conn` (see
  `db/AGENTS.md`).
- `PRAGMA foreign_keys=ON` is set on every connection - the cascade contract
  depends on it.
- See `docs/CLAUDE.md` for the project coding rules (Rust/TS error handling,
  naming, LLM orchestrator pattern, DB rules, testing conventions).

## Verification

See each child doc's Verification section + the root footer:
`npm run check:all` (clippy `-D warnings` on the library crate + rustfmt) and
`cargo test`.

## Child DOX Index

Child `AGENTS.md` files exist under the following subdirectories; each owns
its domain-specific contracts. Modules without a child `AGENTS.md` are
described inline.

- **`db/`** - SQLite layer (repos, migrations, connection, rebuild,
  maintenance). See `db/AGENTS.md`.
- **`llm/`** - OpenAI-compatible + Google chat-completion client + the
  centralized LLM orchestrator (concurrency, rate limiting, timeout,
  temperature recovery, embeddings routing). See `llm/AGENTS.md`.
- **`screening/`** - Tier 3 AI screening engine (Abstract/Enhanced/Two-stage)
  + v8.x cancel/timeout/diagnostics contracts. See `screening/AGENTS.md`.
- **`wiki/`** - LLM Wiki knowledge-base (parallel chunked ingest, 5-layer
  pre-seed, FTS5, drift detection, static-site export). See `wiki/AGENTS.md`.
- **`embedding/`** - Semantic search (director, runner, recall, batching).
  See `embedding/AGENTS.md`.
- **`citation_finder/`** - Paste-prose-to-citations matching (three-layer
  pipeline: embedding prefilter → token-Jaccard passage extraction → LLM
  classify). See `citation_finder/AGENTS.md`.
- **`translation/`** - Non-English article translation (worker, wait, language
  detection). See `translation/AGENTS.md`.
- **`batch_import/`** - 4-phase batch import processor. See
  `batch_import/AGENTS.md`.
- **`openalex/`** - OpenAlex catalog search + reference/citation harvest. See
  `openalex/AGENTS.md`.
- **`scraping/`** - Citation Chaser headless-Chrome scraper. See
  `scraping/AGENTS.md`.
- **`export/`** - Project backup serialize/deserialize + legacy upgrade
  emission. See `export/AGENTS.md`.
- **`utils/`** - Pure helpers (sections, chunking, text_tokens, json_repair,
  pdf_extract). See `utils/AGENTS.md`.
- **`commands/`** - Tauri command handlers (one file per feature area; the
  article state machine, hard-delete cascade, startup upgrade, and
  tags/labels contracts are documented above). No own `AGENTS.md`.
- **`models/`** - Serde structs shared across modules. No own `AGENTS.md`.
- **`dedup/`** - Duplicate detection. No own `AGENTS.md`.
- **`ris/`** + **`bibtex/`** + **`prisma/`** - Bibliographic format
  parsers/converters. No own `AGENTS.md`.
- **`crypto/`** - AES-256-GCM encryption helpers (API keys, LLM config). No
  own `AGENTS.md`.
- **`schema/`** - Shared schema types. No own `AGENTS.md`.
- **`biblio/`** + **`summary/`** + **`batch/`** - Bibliometric commands,
  summary prompts/engine, and batch processing helpers. No own `AGENTS.md`.