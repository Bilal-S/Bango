# db/migrations/

## Purpose

SQLite schema migrations, one file per version (v001-v009), plus the `mod.rs`
registry that feeds the transactional runner in `../migration.rs`.

## Ownership

- Owns: `mod.rs` (the `get_migrations()` registry of `Migration { version,
  up_sql }` entries) + `v001_initial.rs` through
  `v009_doi_canonicalization.rs`.
- The runner (`../migration.rs`) and the schema classification + rebuild
  layer (`schema_check.rs`, `rebuild.rs`) stay in `../AGENTS.md`.

## Local Contracts

### Transactional runner (`../migration.rs`)

**Transactional**: each migration's `up_sql` + `user_version` bump run in a
single `unchecked_transaction` so a crash between the DDL and the version pragma
rolls back cleanly. **Self-healing pre-pass** (`heal_partial_migrations`):
detects DBs corrupted by older non-transactional builds by probing for the v003
marker column (`articles.is_translated`) while `user_version < 3`; if present
it advances `user_version` to 3 without re-running the dangerous
`ALTER TABLE ADD COLUMN` statements (SQLite has no `IF NOT EXISTS` for ADD
COLUMN). Future migrations that add another `ALTER TABLE ... ADD COLUMN` MUST
extend `heal_partial_migrations` with a marker-column check. 5 inline unit
tests + `tests/db/migration_recovery_test.rs`.

### Base-migration parity rule

When a migration changes something a fresh database must also end up with
(CHECK values, index shapes, column definitions), `v001_initial.rs` is updated
in the same change so fresh DBs build the final shape directly instead of
running heal-style repair. Precedents: v007 adds `ai_screen_clear` to the
initial `audit_entries.action` CHECK; v009 creates the non-partial `LOWER(doi)`
index directly. Parity is pinned by
`tests/db/v001_v003_schema_parity_test.rs`.

### CHECK-constraint rebuild pattern

SQLite CHECK constraints cannot be ALTERed. Adding an `audit_entries.action`
value requires a rename-create-copy-drop rebuild (the v003-v007
RENAME-CREATE-INSERT-DROP pattern), and every such rebuild drops
`idx_audit_entries_article_id` - v008 exists to restore it.

### Version inventory

- **`v001_initial.rs`** (VERSION 1) - the base schema (core tables, articles,
  audit, references, and the initial `audit_entries.action` CHECK + DOI
  index; full table list in `docs/bango-v5-spec.md` §2.2). Amended in
  lockstep with later migrations per the parity rule above, so fresh DBs run
  it as their only migration path.
- **`v002_wiki_manifest.rs`** (VERSION 2, deployed) - contains only
  `CREATE TABLE wiki_index_manifest` (per-file content hashes for Wiki
  external-edit drift detection). The FTS5 drop, `article_chunks` creation, and
  `audit_entries` rebuild were in v002 pre-release but moved to v003 after v002
  was deployed with only `wiki_index_manifest`.
- **`v003_articles_translations.rs`** (VERSION 3) - carries the reverted v002
  content (FTS5 drop, `article_chunks`, `audit_entries` rebuild with
  `figure_descriptions` + `ai_screen_enhanced`) plus translation schema:
  `articles` columns (`is_translated`, `translation_status`, `translation_error`,
  `translated_at`), `article_original_content` + `article_original_chunks`
  tables, and `audit_entries` CHECK expansion for `translation` +
  `translation_error`. The `ALTER TABLE ADD COLUMN` statements have no
  `IF NOT EXISTS` guard; the transactional runner + `heal_partial_migrations`
  pre-pass are the contract that prevents duplicate-column crashes on re-run.
- **`v004_gap_analysis.rs`** (VERSION 4) - creates the single-row
  `gap_analysis` table (a regenerable derived artifact written by
  `commands::summary::analyze_research_gaps`) and expands the
  `audit_entries.action` CHECK with `'search_strategy'` (the Search Strategy
  Builder writes a system-level `search_strategy` audit row) via the
  rename-create-copy-drop pattern.
- **`v005_audit_note_add.rs`** (VERSION 5) - rebuilds `audit_entries` to add
  `'note_add'` to the `action` CHECK; creates
  `idx_articles_translation_status`. Idempotent. Frontend `AuditAction` type +
  `formatAuditAction` labels + `audit-timeline.vue` `actionLabels` include
  `note_add`. **Audit entry coalescing**
  (`audit_repo::create_or_update_entry`): when a second audit entry with the
  same `article_id + action + source` arrives within `COALESCE_WINDOW_SECS`
  (300 seconds / 5 minutes), the existing row is **updated** (details +
  timestamp) instead of inserting a new row. Tested in
  `tests/db/audit_coalesce_test.rs`.
- **`v006_audit_metadata_edit.rs`** (VERSION 6) - extends the
  `audit_entries.action` CHECK to include `'metadata_edit'` so in-place
  metadata field edits are correctly categorized. **Heal: empty-string
  `article_id` normalization** - the rebuild also runs
  `UPDATE audit_entries_v006_old SET article_id = NULL WHERE article_id = ''`
  BEFORE the orphan `DELETE` so historical malformed rows are healed rather
  than crashing the subsequent `INSERT ... SELECT`. The
  `update_article_metadata` Tauri command writes `action = 'metadata_edit'`,
  coalesced within the 5-min window. **Title is `TEXT NOT NULL`**, so its
  binding arm rejects empty/whitespace-only input with `AppError::Validation`.
- **`v007_audit_clear_and_embeddings.rs`** (VERSION 7, not yet deployed) -
  extends `audit_entries.action` CHECK with `'ai_screen_clear'` (powers the
  `clear_ai_reasoning` command) + creates the `article_embeddings` table.
  Idempotent. v001 is updated so fresh DBs get `ai_screen_clear` in the initial
  CHECK directly.
- **`v008_audit_index_restore.rs`** (VERSION 8) - restores
  `idx_audit_entries_article_id`, dropped by every `audit_entries` CHECK
  rebuild (v003-v007 RENAME-CREATE-INSERT-DROP pattern).
- **`v009_doi_canonicalization.rs`** (VERSION 9) - DOI canonicalization
  (`tests/db/doi_case_migration_test.rs`, inventory
  `docs/test-plans/doi-case-tests.md`). Heals legacy mixed-case, prefixed
  (`https://doi.org/` / `dx.doi.org` / `doi:`), and whitespace-wrapped DOIs in
  `articles` + `reference_papers` via a single CASE one-strip statement that
  is byte-equivalent to `ris::doi::normalize_doi` (URL prefixes before
  `doi:`, placeholders filtered AFTER the strip so `doi: NA` -> NULL);
  merges case-variant duplicate `reference_papers` (survivor by match-state
  rank `matched`/`imported` > `unmatched`, then lowest `rowid`; links
  remapped with collision absorption; counters recounted from links); and
  rebuilds `uq_ref_papers_doi` as a NON-partial expression index on
  `LOWER(doi)`. Non-partial is load-bearing: SQLite's planner only uses a
  partial index when the query's WHERE clause syntactically implies the
  partial condition, and `LOWER(doi) = ?` does not imply `doi IS NOT NULL`,
  so a partial clause turns every DOI lookup into a full scan (UNIQUE
  already treats NULLs as distinct). Statement order is load-bearing: the old
  BINARY index is dropped FIRST (the healing UPDATEs would violate it on
  case-variant data), and the new index is created LAST. v001 is updated so
  fresh DBs get the non-partial `LOWER(doi)` index directly.

## Work Guidance

- When adding an `ALTER TABLE ADD COLUMN` migration, extend
  `heal_partial_migrations` with a marker-column check.
- Update `v001_initial.rs` in the same change whenever a new migration alters
  something fresh DBs need (parity rule above).
- Register every new migration file in `mod.rs::get_migrations()`, sorted by
  version.

## Verification

- `tests/db/migration_recovery_test.rs` (transactional rollback + heal
  pre-pass)
- `tests/db/v001_v003_schema_parity_test.rs` (fresh-v001 vs migrated-v003
  shape parity)
- `tests/db/doi_case_migration_test.rs` (v009; binding inventory
  `docs/test-plans/doi-case-tests.md`)
- `tests/db/audit_coalesce_test.rs` (v005 coalescing window)
- 5 inline unit tests in `../migration.rs`

## Child DOX Index

No child `AGENTS.md` files.
