//! Tier 1-4 schema: chunk-aware FTS5 + article chunk storage + audit entries
//! expansion.
//!
//! Three changes in one migration (Tiers 1-4 ship together):
//! 1. `DROP TABLE IF EXISTS wiki_pages_fts;` - the FTS5 virtual table is a
//!    derived cache rebuilt from disk by `rebuild_index_with_manifest`. Dropping
//!    it is safe and lets `fts::ensure_table` recreate it with the chunk-aware
//!    column set (`chunk_index`, `section`, `parent_slug`) on the next read.
//!    FTS5 virtual tables cannot be `ALTER`ed, so the explicit `DROP` is the
//!    supported way to change the column set. Self-heals via
//!    `ensure_index_populated` / `rebuild_index_with_manifest`.
//! 2. `CREATE TABLE article_chunks` - per-article chunk storage populated at
//!    attach time by `extract_sections` (T1.1) + `chunk_sections` (T1.2).
//!    Consumed by screening (T3.2+) and reusable by any per-article retrieval.
//! 3. Rebuild `audit_entries` to add both `figure_descriptions` (Tier 2 Phase 4)
//!    and `ai_screen_enhanced` (Tier 3 two-stage screening stage-2 entries) to
//!    the `action` CHECK constraint. SQLite CHECK constraints cannot be ALTERed
//!    in place, so the table is rebuilt via the rename-create-copy-drop pattern.
//!    The v001 initial schema is also updated so fresh DBs get the expanded
//!    constraint directly.
//!
//! No `ALTER TABLE articles`: section summaries (T1.3) live inside the existing
//! `full_text_ai_summary` column as a `schema_version: 2` superset blob, so no
//! new column is needed.

pub const VERSION: i32 = 3;

pub const UP_SQL: &str = "\
-- Drop the lazily-created FTS5 virtual table so fts::ensure_table recreates it
-- with the chunk-aware column set (chunk_index, section, parent_slug) on the
-- next read. The table is a derived cache rebuilt from disk; dropping it is
-- safe and self-heals via ensure_index_populated / rebuild_index_with_manifest.
DROP TABLE IF EXISTS wiki_pages_fts;

-- Article-level chunk storage. Populated at attach time (T3.1) by
-- extract_sections (T1.1) + chunk_sections (T1.2). Consumed by screening
-- (T3.2+) and reusable by any per-article retrieval. Created here so the
-- schema is in place when the chunking primitives land, even though screening
-- does not read it until Tier 3.
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

-- Rebuild audit_entries to add 'figure_descriptions' (Tier 2 Phase 4) and
-- 'ai_screen_enhanced' (Tier 3 two-stage screening stage 2) to the action CHECK
-- constraint. SQLite CHECK constraints cannot be ALTERed; use the
-- rename-create-copy-drop pattern.
ALTER TABLE audit_entries RENAME TO audit_entries_v003_old;

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
FROM audit_entries_v003_old;

DROP TABLE audit_entries_v003_old;
";
