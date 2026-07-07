//! Post-v003 schema addition: single-row persistence for the Research Gap
//! Analysis report.
//!
//! The gap report is a regenerable derived artifact (like the literature
//! review persisted in `summary`), not source data. It is produced by
//! `commands::summary::analyze_research_gaps` from the included corpus and
//! rendered in the AI Summary view's "Research Gaps" segment.
//!
//! Pure `CREATE TABLE IF NOT EXISTS`: idempotent, so the
//! `heal_partial_migrations` marker-probe pattern (required for
//! `ALTER TABLE ADD COLUMN` migrations like v003) is not needed here.

pub const VERSION: i32 = 4;

pub const UP_SQL: &str = "\
-- Research Gap Analysis report (single-row, mirrors the `summary` table).
-- Cleared on project import/reset alongside `summary` (see `export::project`
-- and `db::rebuild::DROP_TABLES`); NOT exported in project backups.
CREATE TABLE IF NOT EXISTS gap_analysis (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    citation_style TEXT NOT NULL DEFAULT 'APA',
    generated_at TEXT NOT NULL,
    gap_text TEXT NOT NULL
);
";
