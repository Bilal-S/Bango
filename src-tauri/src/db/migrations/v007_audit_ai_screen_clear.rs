//! Post-v006 schema addition:
//!
//! Extend the `audit_entries.action` CHECK constraint to include
//! `'ai_screen_clear'`. The new `clear_ai_reasoning` Tauri command (which
//! powers the trashcan icon in the AI Decision card's expanded header) writes
//! audit rows with this action so the clear is visible in the Audit Timeline.
//!
//! SQLite CHECK constraints cannot be ALTERed; the audit rebuild uses the
//! rename-create-copy-drop pattern (same as v003/v004/v005/v006).
//!
//! The operation is idempotent (CHECK rebuild via rename-create-copy-drop is
//! re-runnable), so the `heal_partial_migrations` marker-probe pattern
//! (required for ADD COLUMN migrations like v003) is not needed here.

pub const VERSION: i32 = 7;

pub const UP_SQL: &str = "\
-- Rebuild audit_entries to add 'ai_screen_clear' to the action CHECK constraint.
-- SQLite CHECK constraints cannot be ALTERed; use the rename-create-copy-drop
-- pattern (same as v003/v004/v005/v006). The `clear_ai_reasoning` command
-- writes 'ai_screen_clear' when the user clears the AI reasoning text +
-- confidence from an article via the AI Decision card's trashcan icon.
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
";
