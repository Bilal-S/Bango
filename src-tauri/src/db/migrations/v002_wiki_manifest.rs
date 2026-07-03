//! Post-v001 schema additions: chunk-aware FTS5, article chunk storage,
//! wiki index manifest, and audit_entries expansion.
//!
//! This migration bundles every post-v001 schema change so the migration
//! sequence stays gap-free (v001 -> v002). None of these changes shipped in a
//! tagged release, so consolidating them is safe.
//!
//! Changes (all applied once, in order):
//! 1. `DROP TABLE IF EXISTS wiki_pages_fts;` - the FTS5 virtual table is a
//!    derived cache rebuilt from disk by `rebuild_index_with_manifest`.
//!    Dropping it is safe and lets `fts::ensure_table` recreate it with the
//!    chunk-aware column set (`chunk_index`, `section`, `parent_slug`) on the
//!    next read. FTS5 virtual tables cannot be `ALTER`ed, so the explicit
//!    `DROP` is the supported way to change the column set. Self-heals via
//!    `ensure_index_populated` / `rebuild_index_with_manifest`.
//! 2. `CREATE TABLE article_chunks` - per-article chunk storage populated at
//!    attach time by `extract_sections` + `chunk_sections`. Consumed by
//!    screening and reusable by any per-article retrieval.
//! 3. `CREATE TABLE wiki_index_manifest` - per-file content hashes for every
//!    `wiki/**/*.md` page so the on-demand `wiki_check_for_updates` command
//!    can detect external edits and re-index transparently. Derived cache
//!    (like `wiki_pages_fts`): dropped on `rebuild_schema` / reset and
//!    repopulated from disk on the next check.
//! 4. Rebuild `audit_entries` to add both `figure_descriptions` and
//!    `ai_screen_enhanced` to the `action` CHECK constraint. SQLite CHECK
//!    constraints cannot be ALTERed in place, so the table is rebuilt via the
//!    rename-create-copy-drop pattern. The v001 initial schema is also updated
//!    so fresh DBs get the expanded constraint directly.
//!
//! No section-summary column: section summaries live inside the existing
//! `full_text_ai_summary` column as a `schema_version: 2` superset blob.
//!
//! Note: `has_figures_or_tables` is NOT added here. The column is created
//! directly in v001 (the fresh-DB schema), and since v001 + v002 are both
//! pre-release, there are no v001-only DBs in the wild that need an ALTER.

pub const VERSION: i32 = 2;

pub const UP_SQL: &str = "\
-- 1. Drop the lazily-created FTS5 virtual table so fts::ensure_table recreates it
--    with the chunk-aware column set (chunk_index, section, parent_slug) on the
--    next read. The table is a derived cache rebuilt from disk; dropping it is
--    safe and self-heals via ensure_index_populated / rebuild_index_with_manifest.
DROP TABLE IF EXISTS wiki_pages_fts;

-- 2. Article-level chunk storage. Populated at attach time by
--    extract_sections + chunk_sections. Consumed by screening and reusable by
--    any per-article retrieval.
CREATE TABLE IF NOT EXISTS article_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    section TEXT,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    UNIQUE(article_id, chunk_index)
);
CREATE INDEX IF NOT EXISTS idx_article_chunks_article ON article_chunks(article_id);

-- 3. Wiki index manifest: per-file content hashes for drift detection.
--    One row per wiki/**/*.md page. The directory-level fingerprint lives in
--    app_settings under key 'wiki_dir_hash' (tier-1 fast path). Derived cache:
--    dropped on rebuild_schema / reset and repopulated from disk on next check.
CREATE TABLE IF NOT EXISTS wiki_index_manifest (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL
);

-- 4. Rebuild audit_entries to add 'figure_descriptions' and 'ai_screen_enhanced'
--    to the action CHECK constraint. SQLite CHECK constraints cannot be
--    ALTERed; use the rename-create-copy-drop pattern.
ALTER TABLE audit_entries RENAME TO audit_entries_v002_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'ai_screen_enhanced', 'manual_override',
        'ai_summary', 'error', 'dedup_auto', 'reference_import',
        'reference_match', 'figure_descriptions'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT
);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v002_old;

DROP TABLE audit_entries_v002_old;
";
