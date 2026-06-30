//! Tier 1-4 schema: chunk-aware FTS5 + article chunk storage.
//!
//! Two changes in one migration (Tiers 1-4 ship together):
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
";
