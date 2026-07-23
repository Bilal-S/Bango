//! Post-v005 schema addition:
//!
//! Extend the `audit_entries.action` CHECK constraint to include
//! `'metadata_edit'`. The new `update_article_metadata` Tauri command (which
//! powers the double-click inline editing in the Article Detail "Metadata"
//! card) writes audit rows with this action so metadata edits are correctly
//! categorized in the Audit Timeline.
//!
//! SQLite CHECK constraints cannot be ALTERed; the audit rebuild uses the
//! rename-create-copy-drop pattern (same as v003/v004/v005).
//!
//! The operation is idempotent (CHECK rebuild via rename-create-copy-drop), so
//! the `heal_partial_migrations` marker-probe pattern (required for ADD COLUMN
//! migrations like v003) is not needed here.

pub const VERSION: i32 = 6;

pub const UP_SQL: &str = "\
-- Rebuild audit_entries to add 'metadata_edit' to the action CHECK constraint.
-- SQLite CHECK constraints cannot be ALTERed; use the rename-create-copy-drop
-- pattern (same as v003/v004/v005). The `update_article_metadata` command
-- writes 'metadata_edit' for in-place metadata field edits (Authors,
-- Affiliation, Journal, Year, Lang, DOI, Keywords).
ALTER TABLE audit_entries RENAME TO audit_entries_v006_old;

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
        'metadata_edit'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

DELETE FROM audit_entries_v006_old
WHERE article_id IS NOT NULL
  AND article_id NOT IN (SELECT id FROM articles);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v006_old;

DROP TABLE audit_entries_v006_old;
";
