# src-tauri/src/

## Purpose

Rust backend (Tauri 2.x). All application logic: database, AI/LLM features
(screening, summaries, wiki, embeddings, citation finder), import/export,
bibliometrics, scraping, translation, and the Tauri command layer.

## Ownership

- App entry is `lib.rs` (`run()`), which registers all `#[tauri::command]`
  handlers in one `invoke_handler!` list.
- Owns the article state machine, the hard-delete cascade, and the
  journal-index loader (contracts below). Command-layer contracts (startup
  upgrade, tags/labels, criteria harmonization) live in `commands/AGENTS.md`;
  the integration-test inventory lives in `../tests/AGENTS.md`.
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
`cargo test` (integration-test inventory: `../tests/AGENTS.md`).

Coverage: `cd src-tauri && cargo llvm-cov --html --output-dir target/llvm-cov/html`
(Rust via `cargo-llvm-cov` + `llvm-tools-preview`, report at
`src-tauri/target/llvm-cov/html/html/index.html`). Artifact dirs are
git-ignored.

Disk-space: `src-tauri/target` can balloon into hundreds of GB because each
of the ~136 `src-tauri/tests/*.rs` files compiles into its own ~450MB test
binary (each statically links tauri + GTK/webkit2gtk, headless_chrome, resvg,
the PDF stack (unpdf/pdf-extract/lopdf), and reqwest/rustls; there is no
llama.cpp dependency - "llama.cpp" is only an LLM provider label for a
user-run server), and Cargo never deletes stale hashed binaries across builds.
`target/llvm-cov-target` (~50G) is cargo-llvm-cov's separate target dir.
Reclaim space with `npm run clean:rust` (full `cargo clean`) or
`npm run sweep:rust` (`cargo-sweep -t 14`, removes only artifacts untouched
>14 days - preferred periodic cleanup). After coverage runs,
`cargo llvm-cov clean` drops `target/llvm-cov-target`. `cargo-sweep` must be
installed (`cargo install cargo-sweep`).

## Child DOX Index

Child `AGENTS.md` files exist under the following subdirectories; each owns
its domain-specific contracts. Modules without a child `AGENTS.md` are
described inline.

- **`commands/`** - Tauri command handlers (one file per feature area):
  startup upgrade loop-safety, tags/labels staleness + merge contracts,
  criteria harmonization. See `commands/AGENTS.md`. The article state machine
  and hard-delete cascade contracts stay in this doc (Local Contracts above).
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
- **`models/`** - Serde structs shared across modules. No own `AGENTS.md`.
- **`dedup/`** - Duplicate detection. No own `AGENTS.md`.
- **`ris/`** + **`bibtex/`** - Bibliographic format parsers/converters. No own
  `AGENTS.md`.
- **`prisma/`** - PRISMA flow data (`data.rs`) + the screening reasons report
  (`report.rs`): primary-reason attribution (highest criterion priority wins,
  ties broken by first-assigned order = earliest UUID in the article's matched
  array), multi-assignment counts (one row per matched criterion), "General"
  buckets for articles with no resolvable matched criterion, and the Markdown
  rendering consumed by the `get_prisma_report_markdown` command. With a
  custom project name (`app_settings.project_name`) the report opens with
  `# {Project Name}` over an h2 report title (sections demoted to h3);
  otherwise the report title is the single h1. Frontend exports it as Markdown
  (save dialog) or PDF (print dialog) from the PRISMA view. Tested in
  `tests/prisma_test.rs`, `tests/prisma_svg_test.rs`,
  `tests/prisma_report_test.rs`. No own `AGENTS.md`.
- **`crypto/`** - AES-256-GCM encryption helpers (API keys, LLM config). No
  own `AGENTS.md`.
- **`schema/`** - Shared schema types. No own `AGENTS.md`.
- **`biblio/`** - Bibliometric normalization + `thematic.rs` (cluster
  thematic analysis: member resolution dispatcher, three-source term
  resolution mirroring the keyword network builder, Top-N article cap,
  link-protocol registry, grounded prompt builder; all `pub` + pure, tested
  from `tests/biblio_cluster_themes_test.rs`). Plus **`summary/`** +
  **`batch/`** - summary prompts/engine, and batch processing helpers. No own
  `AGENTS.md`.
