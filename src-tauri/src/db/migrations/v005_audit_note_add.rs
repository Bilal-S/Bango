//! Post-v004 schema additions:
//!
//! 1. Extend the `audit_entries.action` CHECK constraint to include
//!    `'note_add'`. The `update_article_notes` command previously reused
//!    `'status_change'` for note-addition audit rows, which made the audit
//!    trail misleading (a note edit appeared as a status change). This
//!    migration adds the dedicated `'note_add'` action value so note edits
//!    are correctly categorized.
//!
//! 2. Add a covering index on the translation-state columns of `articles`.
//!    The crash-recovery query `get_stranded_translation_articles` filters on
//!    `translation_status IN ('queued','running') AND is_translated = 0`,
//!    which runs on every app startup. Without an index this is a full table
//!    scan of up to 10,000 rows on a cold cache - especially costly on
//!    Windows where real-time antivirus intercepts every page read. The
//!    composite index makes that lookup an index range scan.
//!
//! SQLite CHECK constraints cannot be ALTERed; the audit rebuild uses the
//! rename-create-copy-drop pattern (same as v003's two audit rebuilds and
//! v004's single rebuild).
//!
//! Both operations are idempotent (CHECK rebuild via rename-create-copy-drop;
//! `CREATE INDEX IF NOT EXISTS`), so the `heal_partial_migrations`
//! marker-probe pattern (required for ADD COLUMN migrations like v003) is not
//! needed here.

pub const VERSION: i32 = 5;

pub const UP_SQL: &str = "\
-- Rebuild audit_entries to add 'note_add' to the action CHECK constraint.
-- SQLite CHECK constraints cannot be ALTERed; use the rename-create-copy-drop
-- pattern (same as v003/v004). The `update_article_notes` command now writes
-- 'note_add' instead of reusing 'status_change'.
ALTER TABLE audit_entries RENAME TO audit_entries_v005_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'note_add', 'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'ai_screen_enhanced', 'manual_override',
        'ai_summary', 'error', 'dedup_auto', 'reference_import',
        'reference_match', 'figure_descriptions',
        'translation', 'translation_error',
        'search_strategy'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v005_old;

DROP TABLE audit_entries_v005_old;

-- Index the translation-state columns so the startup stranded-recovery query
-- (`translation_status IN ('queued','running') AND is_translated = 0`) is an
-- index range scan instead of a full table scan. Also speeds up
-- `get_translatable_import_ids` which filters on `is_translated` + status.
CREATE INDEX IF NOT EXISTS idx_articles_translation_status
    ON articles(translation_status, is_translated);
";
