# db/

## Purpose

SQLite database layer: repos (article, biblio, journal, tags, labels, criteria,
audit, LLM config, app settings, embeddings, chunks, references, summaries,
gap analysis, original content), migration runner, schema check/rebuild,
maintenance (VACUUM), and the shared `DbState` connection holder.

## Ownership

- Owns: `connection.rs`, `migration.rs`, `schema_check.rs`, `rebuild.rs`,
  `maintenance.rs`, `mod.rs`, `app_settings_repo.rs`, `article_repo/`,
  `biblio_repo/`, `journal_repo.rs`, `tag_repo.rs`, `label_repo.rs`,
  `tag_label_core.rs`, `criteria_repo.rs`, `audit_repo.rs`,
  `llm_config_repo.rs`, `embedding_repo.rs`, `chunk_repo.rs`,
  `reference_repo.rs`, `summary_repo.rs`, `gap_analysis_repo.rs`,
  `saved_report.rs`, `article_original_repo.rs`, `migrations/`.
- Consumed by every backend module that touches the DB.

## Local Contracts

### `connection.rs` - `DbState` + `lock_conn`

Holds `DbState` (`conn: Mutex<Connection>`) and the shared
`lock_conn(conn_mutex) -> Result<MutexGuard<'_, Connection>, AppError>` helper
that maps `Mutex::lock()` poison failures to `AppError::LockPoisoned` (not
`AppError::Database`). Every command handler and engine that locks
`DbState.conn` MUST route through `lock_conn` instead of inlining
`.lock().map_err(...)` so poison errors stay correctly categorized as
application-state errors and the error-mapping boilerplate is not duplicated.
The private `lock_conn` in `commands/wiki_cmd` and `lock_db` in
`translation/engine.rs` were removed in favor of this shared helper. Tested in
`tests/db/lock_poison_test.rs`.

`lock_conn` also times the acquire and emits
`lock_conn: SLOW acquire ({ms}ms)` when > `SLOW_LOCK_THRESHOLD_MS = 100`; this
is the single most valuable signal for mutex-starvation hangs (every other
DB-touching IPC command shows a slow acquire while a long pass holds the
mutex).

### `app_settings_repo.rs` - key/value `app_settings` store

Holds:

- `storage_root` (Bango documents root; `fulltext/`, `ris/`, `wiki-root/`
  derive from it as subdirectories; lazy-migrated from the legacy
  `fulltext_storage_dir` key by `get_storage_root`, which strips a trailing
  `fulltext` segment to derive the root)
- `flag_premium`
- `biblio_needs_refresh` (the bibliometric staleness flag)
- `wiki_needs_refresh` (the LLM Wiki staleness flag)
- `auto_translate` (experimental toggle for translating non-English articles
  to English during AI processing; DB-backed unlike the sibling localStorage
  AI Summary toggles; **default `false` (opt-in)** - absent/garbage value
  falls back to the default)
- `summary_evidence_mode` (project-wide literature-review evidence enrichment;
  `abstract_only` default | `with_summary_facts`)
- `screening_custom_logic` (optional free-text combinatorial screening rules -
  AND/OR gates, hard exclusions, conditional inclusion - authored on the
  Criteria screen Section 4 "Custom Screening Instructions" and injected into
  every screening prompt as a `## Custom Screening Instructions` section after
  `## Priority Rules`; references criteria by their **global number** so the
  LLM, the user, and the reasoning all mean the same thing by "criterion 3";
  empty/absent → no section emitted, byte-identical to pre-feature prompts;
  `commands::criteria::check_rules` runs an LLM consistency review over the
  whole ruleset incl. custom rules; the screening prompt now numbers inclusion
  `1..N` then exclusion continues `N+1..N+M` via `CriterionEntry.global_number`
  so "criterion 11" is unambiguous)
- `openalex_api_key` (AES-256-GCM encrypted; optional; raises rate-limit tier
  from 10 to 100 req/s; **deliberately excluded from
  `PROJECT_PORTABLE_SETTINGS`** - API key, never exported)
- `openalex_mailto` (plaintext string; **included in
  `PROJECT_PORTABLE_SETTINGS`**; if unset, a Bango app default
  `"research@bango.app"` is used)
- `openalex_retrieve_references` (plaintext boolean; gates the OpenAlex
  Reference + Citation Harvest; defaults to `false`; **included in
  `PROJECT_PORTABLE_SETTINGS`**)
- `embedding_model_override` (optional plaintext embedding-model name;
  **premium-only** - the `set_embedding_model_override` command rejects
  non-premium callers. When set, `probe_embedding_support` tries it FIRST,
  ahead of the provider-default and the configured chat model. When the
  override fails (404/405/auth), the probe falls back to standard
  auto-detection. Empty/whitespace = cleared. **Deliberately excluded from
  `PROJECT_PORTABLE_SETTINGS`** - machine-local.)
- `embedding_status` / `embedding_model` / `embedding_dimensions` (triple-state
  capability flag; see `embedding/AGENTS.md`)
- `project_name` (optional plaintext, up to `PROJECT_NAME_MAX_LEN = 60` chars;
  user-editable Dashboard title. `set_project_name` trims + hard-caps via
  `chars().take(50)`; empty/whitespace-only stores NULL. **Included in
  `PROJECT_PORTABLE_SETTINGS`**. **Import contract**: unlike the other portable
  settings (which preserve the target machine's value when the backup omits
  them), `project_name` is *project identity* - when the backup omits it the
  import path explicitly clears the target's pre-existing name (NULL) so the
  dashboard reverts to the fallback.)

#### Staleness flags

`mark_biblio_needs_refresh(conn)` is called by every mutation that changes
data bibliometrics depends on (RIS/BibTeX import in `commands/import.rs`,
project backup restore in `commands/export_cmd::import_project_backup`,
reference/citation import + CR extraction + reference promotion in
`commands/references.rs`, tag/label/status/override/bulk edits in
`commands/articles.rs`, and AI screening completion in
`commands/screening.rs`). `clear_biblio_needs_refresh` runs only after
`biblio_normalize` commits successfully; `get_biblio_needs_refresh` powers the
frontend `biblio_get_needs_refresh` command. Absent key = fresh (false).

`mark_wiki_needs_refresh(conn)` is called by every mutation that changes the
Wiki's content sources (`full_text` attach/delete in `commands/full_text.rs`,
AI-summary regen in `commands/summary.rs::generate_article_ai_summary`) plus
the same corpus mutations that set the biblio flag (RIS/BibTeX import, project
backup restore, reference/citation import, tag/label/status/override/bulk
edits, AI screening completion). `clear_wiki_needs_refresh` runs only after
`wiki_ingest`/`wiki_rebuild` commits; `get_wiki_needs_refresh` powers the
frontend `wiki_get_needs_refresh` command that drives the Update button's
enabled/disabled state in `wiki-toolbar.vue` (`needsRefresh=true` -> primary
indigo + enabled; `false` -> grey + disabled). The former `autoIngestIfStale()`
auto-trigger in `wiki-view.vue` was REMOVED so visiting the Wiki tab never
surprises the user with a multi-minute LLM + bibliometrics pipeline; updates
are explicit via the Update button. `clear_wiki_needs_refresh` also runs in
`wiki_delete_wiki` (defense-in-depth alongside the AGENTS.md removal). Absent
key = fresh (false). Tested in `wiki_full_text_refresh_test.rs` +
`wiki_test.rs::delete_wiki_de_initializes_by_removing_agents_md`.

### `article_repo/` - article CRUD + queries + mutations

**Directory module** (refactor v6): split from the former 2,270-line
`article_repo.rs` into `mod.rs` (shared constants + row mapper + `pub use`
re-exports) + 10 submodules (`screening_queries`, `insert`, `query`,
`mutations`, `metadata`, `bulk_ops`, `full_text`, `translation`, `doi_journal`,
`delete`). Public API unchanged: `crate::db::article_repo::*` import paths
resolve identically via the `mod.rs` re-exports.

#### `ArticleQuery` contract

`ArticleQuery` carries four tag/label filter vectors: `tags` + `excluded_tags`,
`labels` + `excluded_labels`. The inclusion vectors (`tags`/`labels`) emit
`articles.id IN (SELECT ...)` clauses (article must have the tag/label); the
exclusion vectors (`excluded_tags`/`excluded_labels`) emit
`articles.id NOT IN (SELECT ...)` clauses (article must NOT have the
tag/label). All four are `#[serde(default)]`. Comparison is `LOWER()`-based
(case-insensitive). It also carries two DOI filter fields: `doi:
Option<String>` (case-insensitive partial match) and `doi_empty: bool` (when
true, emits `doi IS NULL OR doi = ''`). The two are mutually exclusive:
`doi_empty` wins if both are set.
It also carries four matched-criteria filter fields (all `#[serde(default)]):
`matched_criteria: Vec<String>` (criterion UUIDs; each AND-combines like the
tag/label vectors, matching if present in EITHER `matched_inclusion_criteria`
OR `matched_exclusion_criteria` via correlated `json_each`), `criteria_unknown:
bool` (>= 1 matched UUID missing from `criteria` - deleted-criterion ghosts),
`criteria_empty: bool` ("Z. No Criteria"; emits the `doi_empty`-style
literal comparison `(matched_inclusion_criteria IS NULL OR = '[]') AND
(matched_exclusion_criteria IS NULL OR = '[]')` on both arrays), and
`exclusion_criteria_empty: bool` ("X. No Exclusion Criteria"; the exclusion
column alone: `matched_exclusion_criteria IS NULL OR = '[]'` - byte-identical
to `prisma::data`'s `records_excluded_general` predicate, so Rejected tab +
this flag reproduces that PRISMA count exactly; the inclusion column is
irrelevant). The
`json_each` branches wrap their calls in a `json_valid` CASE guard so
malformed/NULL JSON never errors (matches `row_to_article`'s
decode-to-empty fallback); `criteria_empty` and `exclusion_criteria_empty`
use exact-string comparisons that are crash-proof by construction (malformed
values simply do not match - mirroring the PRISMA literal). Tested in
`tests/db/article_query_test.rs`; the PRISMA parity is pinned in
`tests/prisma/prisma_test.rs`
(`test_rejected_plus_exclusion_criteria_empty_matches_prisma_general_excluded`).
`count_query_articles(conn, &ArticleQuery) -> i64` counts rows matching the
SAME filters (both share the private `build_article_query_filters` helper, so
they can never drift; sort/limit/offset are ignored). It backs the
`count_query_articles` Tauri command, which the frontend article list calls
alongside `query_articles` whenever `isQueryFiltered` is true so the result
count, pager, and range display reflect the full match set instead of the
current `limit`-capped page.

#### Bulk tag/label add + remove contract

`bulk_add_tag_to_articles`, `bulk_add_label_to_articles`,
`bulk_remove_tag_from_articles`, and `bulk_remove_label_from_articles` each
return `Vec<String>` (the IDs of articles actually affected), so the command
layer can write one coalesced audit entry per affected article and the frontend
toast can report the accurate affected count. Each touched article's
`changed_at` is bumped only when a junction row is inserted/deleted. Tested in
`tests/db/bulk_tag_label_test.rs` (17 tests).

#### Bulk-export-by-ids fetcher

`get_articles_by_ids(conn, ids)` composes `ARTICLE_SELECT_BASE` with a
parameterized `id IN (?,…)` clause - the sole backend read path for the
Article-list bulk action bar "Export" button. Tested in
`tests/db/article_get_by_ids_test.rs`.

#### `clear_ai_reasoning`

Nulls `ai_decision` + `ai_reasoning` + `ai_confidence` so the entire AI
Decision card unmounts. The user's own Include/Exclude choice lives on the
separate `status` field, which stays intact. `screened_at` is preserved so the
screening history survives and the article is NOT re-enqueued. Writes an
`ai_screen_clear` audit entry. Surfaced via the `clear_ai_reasoning` Tauri
command + the trashcan icon in the AI Decision card's expanded header. Tested
via `tests/db/migration_recovery_test.rs` (final `user_version = 7`).

### `biblio_repo/` - bibliometric repos

`kpis`, `authors`, `networks`, `terms`, `institutions`, `normalization`,
`productivity`. Contract: `get_biblio_kpis` returns `BiblioKpis` including
`journal_distribution: Vec<JournalYearData>` (canonical titles via
`journal_index` LEFT JOIN, fallback `UPPER(TRIM(journal))`). `productivity.rs`
exposes `get_author_rankings`, `get_author_detail`,
`get_author_productivity_kpis` - author-level h-index, i10, g-index,
first/last/solo counts scoped to included articles. `networks/` is a directory
module (split from the former monolithic `networks.rs`) with one file per
network type: `persistence.rs`, `labels.rs`, `coauthors.rs`, `citations.rs`,
`keywords.rs`, `cocitation.rs` (on-demand co-citation with 4 normalization
modes: Raw, Cosine, Jaccard, Pearson; `CocitationScope` = included/all
articles). `mod.rs` re-exports the public API unchanged. The full 8-step
bibliometric pipeline is extracted into a pure
`pub fn run_full_normalization(conn)` in `biblio_repo/normalization.rs` and
shared by both `biblio_normalize` and the wiki ingest path.

### `saved_report.rs` + `tag_label_core.rs` - shared repo cores (refactor v1 Tier 2)

`summary` and `gap_analysis` are separate single-row tables sharing one
contract (spec §10.2: `id = 1`, wiped by `reset_project`, excluded from
`ProjectBackup`). `saved_report.rs` owns the shared save/get/clear core;
`summary_repo.rs` and `gap_analysis_repo.rs` are thin wrappers supplying a
`SavedReportTable { table, text_column }` const and mapping the generic
`SavedReport { text, ... }` into their serde-renamed public structs
(`summaryText` / `gapText` IPC shapes unchanged). Table/column identifiers
are compile-time constants - never user input - so the composed SQL is safe;
all values stay bound parameters.

`tag_label_core.rs` holds the mechanical `tags`/`labels` helpers (raw
`id, name, source, color` row read, case-insensitive
`LOWER(name) = LOWER(?1)` lookup, batch exists-check). Tags and labels stay
distinct domain concepts per spec: separate repos, models, and commands;
only the SQL shapes are shared. The normalized-name create-dedupe contract
(existing row returned on case-insensitive match, original casing preserved)
is pinned by `tests/db/tags_labels_test.rs::create_tag_dedupes_normalized_name`
+ `create_label_dedupes_normalized_name`.

### `journal_repo.rs` - journal_index lookup/match

The **single automatic matching function** `match_journal`
(`resolve_journal_id` wraps it; `get_journal_info` is the metadata+aggregates
reader) is the sole entry point for import, project restore, the "Rematch
Journals" command, and the frontend journal edit. Two pure `#[must_use]`
helpers drive the tiers: `normalize_issn` + `normalize_journal_name`.
**Automatic matching is normalized-equality-only; there is intentionally no
LIKE/substring tier** because silent auto-linking during import must not pick
the wrong journal among similar names. `search_journal_index` (+
`JournalIndexMatch` struct) is the **interactive** counterpart for the
article-metadata journal autocomplete: it DOES use LIKE substring (safe because
the user reviews candidates), gated on a 4-char minimum. `articles.journal_index_id`
is populated on import and refreshable via `rematch_journals`; intentionally NOT
round-tripped through project backup/restore (re-derived on import). Tested in
`tests/db/journal_repo_test.rs` (31 tests).

### `chunk_repo.rs` - Tier 3 article chunk storage

`article_chunks` table (created by migration v003). Populated at attach time by
`commands::full_text::populate_chunks_for_attached_text` (extract_sections +
chunk_sections) and cleared on detach. Consumed by
`screening::chunk_retrieval`. Exposes `replace_chunks_for_article`,
`list_chunks_for_article`, `delete_chunks_for_article`, `count_chunks_for_article`,
`get_articles_with_full_text_missing_chunks` (screening-start guard),
`get_articles_with_full_text`, `count_articles_with_full_text`. Tested in
`tests/screening/chunk_retrieval_test.rs`.

### `embedding_repo.rs` - embedding vector storage

CRUD + `list_for_recall` for the `article_embeddings` table (v007). Keyed on
`(article_id, chunk_index)` where the title+abstract row uses the sentinel
`chunk_index = -1`. The `-1` sentinel (not NULL) participates correctly in the
composite PRIMARY KEY; SQLite treats NULL as distinct in a PK, which would
defeat `INSERT OR REPLACE` on the title+abstract row. `ON DELETE CASCADE`
removes rows when an article is hard-deleted. See `embedding/AGENTS.md` for the
director/runner/recall contract. Tested in `tests/embedding/embedding_storage_test.rs`.

### `schema_check.rs` + `rebuild.rs` - startup legacy-DB detection + rebuild

`check_schema` classifies a live DB as `Current` / `Legacy` / `FreshDb` via
`sqlite_master` (the old and new v1 migrations both set `user_version=1`, so
the pragma cannot be trusted). `rebuild_schema` is the shared drop-all-tables
(preserving `journal_index`) + reset `user_version=0` + re-run migrations
helper used by both `commands::export_cmd::reset_project` and the legacy
upgrade path. `DROP_TABLES` includes the lazily-created `wiki_pages_fts` FTS5
virtual table (self-heals via `fts::ensure_index_populated`) and the
`wiki_index_manifest` drift-detection cache (self-heals via
`wiki_check_for_updates`).

### `maintenance.rs` - VACUUM

`vacuum_database` does a journal-mode round trip - `DELETE` (forces a WAL
checkpoint + removes the `-wal`/`-shm` sidecars) → `VACUUM` (rewrites +
shrinks the main file) → `WAL` (restores normal operating mode) - because a
plain `VACUUM` on a WAL-mode DB writes the compacted pages to the WAL rather
than shrinking the main file. Non-fatal. VACUUM is `O(n)` over the DB file, so
it is intentionally NOT called on per-article deletes or other hot paths - only
at coarse, infrequent, destructive boundaries (e.g. `reset_project`). Tested in
`tests/db/maintenance_test.rs` + `tests/export/reset_project_test.rs`.

### `migration.rs` + `migrations/` - transactional runner + version inventory

The transactional runner contract (single `unchecked_transaction` per
migration + the `heal_partial_migrations` self-healing pre-pass) and the
per-version inventory (v001-v009, incl. the CHECK-constraint rebuild pattern
and the base-migration parity rule) live in `migrations/AGENTS.md`.

## Work Guidance

- Route every `DbState.conn` lock through `lock_conn` (not inline `.lock()`).
- Keep `PROJECT_PORTABLE_SETTINGS` in sync with `app_settings` key changes
  (see the DOX rule in `export/AGENTS.md`).
- Schema changes follow `migrations/AGENTS.md` (transactional runner,
  `heal_partial_migrations` marker rule, per-version inventory).

## Verification

- `tests/db/lock_poison_test.rs`, `tests/db/maintenance_test.rs`,
  `tests/export/reset_project_test.rs`, `tests/db/migration_recovery_test.rs`,
  `tests/biblio/biblio_repo_tests.rs`, `tests/biblio/biblio_networks_test.rs`,
  `tests/db/article_metadata_test.rs`, `tests/db/article_query_test.rs`,
  `tests/db/article_get_by_ids_test.rs`, `tests/db/bulk_tag_label_test.rs`,
  `tests/biblio/biblio_needs_refresh_test.rs`, `tests/translation/auto_translate_test.rs`,
  `tests/db/journal_repo_test.rs`, `tests/screening/chunk_retrieval_test.rs`,
  `tests/embedding/embedding_storage_test.rs`, `tests/db/project_name_test.rs`,
  `tests/db/audit_coalesce_test.rs`, `tests/db/article_repo_coverage_test.rs`,
  `tests/prisma/prisma_test.rs` (incl. the `exclusion_criteria_empty` PRISMA parity
  test).

## Child DOX Index

- **`article_repo/`** - directory module (10 submodules). No own `AGENTS.md`;
  the contracts above cover it.
- **`biblio_repo/`** - directory module (`kpis`, `authors`, `networks/`,
  `terms`, `institutions`, `normalization`, `productivity`). No own
  `AGENTS.md`.
- **`migrations/`** - one file per version (v001-v009) + the `mod.rs`
  registry. See `migrations/AGENTS.md` for the transactional runner contract,
  the `heal_partial_migrations` pre-pass, the base-migration parity rule, and
  the per-version inventory.