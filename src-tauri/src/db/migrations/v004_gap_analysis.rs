//! Post-v003 schema additions: single-row persistence for the Research Gap
//! Analysis report AND the `audit_entries.action` CHECK expansion for the
//! Search Strategy Builder.
//!
//! Two concerns ride in this single migration because v004 had not shipped
//! yet when the Search Strategy Builder (spec §8.4) landed:
//!
//! 1. The gap report is a regenerable derived artifact (like the literature
//!    review persisted in `summary`), not source data. It is produced by
//!    `commands::summary::analyze_research_gaps` from the included corpus and
//!    rendered in the AI Summary view's "Research Gaps" segment.
//! 2. The Search Strategy Builder writes a system-level `search_strategy`
//!    audit row on success (via `audit_repo::log_system_action`), so the
//!    `audit_entries.action` CHECK constraint must include `'search_strategy'`.
//!    SQLite CHECK constraints cannot be ALTERed; this migration uses the
//!    rename-create-copy-drop pattern (mirrors v003's two audit rebuilds).
//!
//! Pure `CREATE TABLE IF NOT EXISTS` + CHECK rebuild (no `ALTER TABLE ADD
//! COLUMN`): idempotent, so the `heal_partial_migrations` marker-probe
//! pattern (required for ADD COLUMN migrations like v003) is not needed here.

pub const VERSION: i32 = 4;

pub const UP_SQL: &str = "\
-- 1. Research Gap Analysis report (single-row, mirrors the `summary` table).
--    Cleared on project import/reset alongside `summary` (see `export::project`
--    and `db::rebuild::DROP_TABLES`); NOT exported in project backups.
CREATE TABLE IF NOT EXISTS gap_analysis (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    citation_style TEXT NOT NULL DEFAULT 'APA',
    generated_at TEXT NOT NULL,
    gap_text TEXT NOT NULL
);

-- 2. Rebuild audit_entries to add 'search_strategy' to the action CHECK
--    constraint. SQLite CHECK constraints cannot be ALTERed; use the
--    rename-create-copy-drop pattern (same as v003's two audit rebuilds).
--    The Search Strategy Builder writes this row on success via
--    `audit_repo::log_system_action` (article_id = NULL, source = 'ai').
ALTER TABLE audit_entries RENAME TO audit_entries_v004_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
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
FROM audit_entries_v004_old;

DROP TABLE audit_entries_v004_old;
";
