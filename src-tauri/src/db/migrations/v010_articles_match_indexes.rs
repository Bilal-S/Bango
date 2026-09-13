//! Match-acceleration indexes on `articles` (VERSION 10).
//!
//! `reference_repo::auto_match_paper_to_article` and the reference-import
//! flows probe `articles` by `LOWER(doi)` and `LOWER(title)`. Neither column
//! had ANY index, so every probe was a full scan of the widest table in the
//! DB (abstract/full-text columns are stored inline) - multiplied by every
//! unmatched reference paper, this dominated `biblio_normalize` runtime on
//! libraries with harvested references/citations.
//!
//! Both indexes are deliberately NON-partial (same planner rationale as the
//! v009 `uq_ref_papers_doi` rebuild): SQLite only uses a partial index when
//! the query's WHERE clause syntactically implies the partial condition, and
//! `LOWER(doi) = ?` does not imply `doi IS NOT NULL`, so a partial clause
//! silently turns every probe back into a full scan.
//!
//! Idempotent (`CREATE INDEX IF NOT EXISTS`) and no `ALTER TABLE ADD COLUMN`,
//! so no `heal_partial_migrations` marker probe is needed. v001 is updated in
//! lockstep (base-migration parity rule) so fresh DBs build the final shape
//! directly.

pub const VERSION: i32 = 10;

pub const UP_SQL: &str = "\
-- Case-insensitive article DOI lookup for reference auto-matching.
CREATE INDEX IF NOT EXISTS idx_articles_doi_lower
    ON articles(LOWER(doi));

-- Case-insensitive article title lookup for title+journal+year matching.
CREATE INDEX IF NOT EXISTS idx_articles_title_lower
    ON articles(LOWER(title));
";
