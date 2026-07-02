//! Migration v004: add `ai_screen_enhanced` to the `audit_entries.action`
//! CHECK constraint (Tier 3 two-stage screening stage-2 audit entries).
//!
//! SQLite CHECK constraints cannot be `ALTER`ed, so we use the same
//! rename-create-copy-drop pattern as v003. This is the only schema change in
//! Tier 3 (the `article_chunks` table was already created by v003).

pub const VERSION: i32 = 4;

pub const UP_SQL: &str = "\
-- Rebuild audit_entries to add 'ai_screen_enhanced' to the action CHECK
-- constraint (Tier 3 two-stage screening stage 2). SQLite CHECK constraints
-- cannot be ALTERed; use the rename-create-copy-drop pattern.
ALTER TABLE audit_entries RENAME TO audit_entries_v004_old;

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
FROM audit_entries_v004_old;

DROP TABLE audit_entries_v004_old;
";
