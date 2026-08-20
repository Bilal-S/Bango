//! Post-v006 schema additions (VERSION 7, not yet deployed).
//!
//! Two bundles folded into one migration:
//!
//! 1. **`audit_entries.action` CHECK expansion for `'ai_screen_clear'`** - the
//!    `clear_ai_reasoning` command (trashcan icon in the AI Decision card's
//!    expanded header) writes audit rows with this action so the clear is
//!    visible in the Audit Timeline. Same rename-create-copy-drop pattern as
//!    v003-v006. Preserves the v006 empty-string `article_id` heal.
//!
//! 2. **`article_embeddings` table** - per-article, per-chunk embedding vectors
//!    for semantic search. Keyed on
//!    `(article_id, chunk_index)` where the title+abstract row uses the
//!    sentinel `chunk_index = -1` and per-chunk rows use the matching
//!    `article_chunks.chunk_index` (`>= 0`). The `-1` sentinel (not NULL)
//!    participates correctly in the composite PRIMARY KEY. Regenerable derived
//!    artifact: excluded from project backups, cleared on `reset_project` (via
//!    `rebuild::DROP_TABLES`), and `ON DELETE CASCADE` removes rows when an
//!    article is hard-deleted.
//!
//! Both operations are idempotent (`CREATE TABLE IF NOT EXISTS` + the
//! CHECK-rebuild is re-runnable), so the `heal_partial_migrations`
//! marker-probe pattern (required for `ADD COLUMN` migrations like v003) is
//! not needed here.

pub const VERSION: i32 = 7;

pub const UP_SQL: &str = "\
-- 1. Rebuild audit_entries to add 'ai_screen_clear' to the action CHECK
--    constraint. SQLite CHECK constraints cannot be ALTERed; use the
--    rename-create-copy-drop pattern (same as v003/v004/v005/v006). The
--    `clear_ai_reasoning` command writes 'ai_screen_clear' when the user
--    clears the AI reasoning text + confidence from an article via the AI
--    Decision card's trashcan icon.
ALTER TABLE audit_entries RENAME TO audit_entries_v007_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'note_add', 'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'ai_screen_enhanced', 'manual_override',
        'ai_summary', 'error', 'dedup_auto', 'reference_import',
        'reference_match', 'figure_descriptions',
        'translation', 'translation_error',
        'search_strategy',
        'metadata_edit',
        'ai_screen_clear'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

-- Preserve the v006 heal: empty-string article_id normalization. System-level
-- audit entries written by an older log_error implementation used '' instead
-- of NULL; the FK rejects '' under PRAGMA foreign_keys=ON. Run BEFORE the
-- orphan sweep (which matches `article_id IS NOT NULL`).
UPDATE audit_entries_v007_old SET article_id = NULL WHERE article_id = '';

DELETE FROM audit_entries_v007_old
WHERE article_id IS NOT NULL
  AND article_id NOT IN (SELECT id FROM articles);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v007_old;

DROP TABLE audit_entries_v007_old;

-- 2. Article embedding storage for semantic search. The title+abstract row
--    uses the sentinel chunk_index = -1; per-chunk rows use the matching
--    article_chunks.chunk_index (>=0). The sentinel is a real value (not NULL)
--    so it participates in the composite PRIMARY KEY correctly: SQLite treats
--    NULL values as distinct in a PK, which would defeat INSERT OR REPLACE on
--    the title+abstract row. ON DELETE CASCADE removes rows when an article is
--    hard-deleted (the article delete path enables PRAGMA foreign_keys = ON).
--    The embedding BLOB is a little-endian f32 stream of length
--    `dimensions * 4`.
CREATE TABLE IF NOT EXISTS article_embeddings (
    article_id   TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,
    embedding    BLOB NOT NULL,
    dimensions   INTEGER NOT NULL,
    input_hash   TEXT NOT NULL,
    model_name   TEXT NOT NULL,
    provider     TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    PRIMARY KEY (article_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_article_embeddings_article ON article_embeddings(article_id);
";
