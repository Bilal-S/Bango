//! Shared schema rebuild: drop every user table (preserving only
//! `journal_index`), reset `user_version`, and re-run all migrations.
//!
//! Used by both `reset_project` (settings: delete all data) and the startup
//! legacy upgrade path. Keeping the drop list in one place prevents the two
//! callers from drifting when new tables are added.

use rusqlite::Connection;

use super::migration;
use crate::error::AppError;

/// All tables that hold user/project data, plus the legacy `article_references`
/// table from the old v1 schema. Dropped before migrations rebuild the schema.
///
/// `journal_index` is intentionally NOT listed here: it is system-distributed
/// reference data that survives reset/upgrade and is (re)populated from the
/// bundled portal DB.
const DROP_TABLES: &[&str] = &[
    // Current schema
    "article_reference_links",
    "reference_papers",
    "article_labels",
    "article_tags",
    "audit_entries",
    "articles",
    "criteria",
    "research_aims",
    "tags",
    "labels",
    "llm_config",
    "summary",
    // Research Gap Analysis report (single-row, mirrors `summary`).
    "gap_analysis",
    "app_settings",
    // Article chunk storage (T1.2/T3.1)
    "article_chunks",
    // Translation originals (Plan-A permanent rewrite)
    "article_original_chunks",
    "article_original_content",
    // Bibliometrics tables
    "biblio_network_edges",
    "biblio_network_nodes",
    "biblio_network_meta",
    "biblio_article_terms",
    "biblio_terms",
    "biblio_author_affiliations",
    "biblio_article_authors",
    "biblio_institutions",
    "biblio_authors",
    // Legacy v1 schema (single-table references model)
    "article_references",
    // Wiki FTS5 virtual table. Created lazily by `wiki::fts::ensure_table`
    // (not by migrations), so it must be dropped here explicitly. It is
    // recreated on demand by `ensure_index_populated` (self-heal) when wiki
    // pages are next read. Safe for the legacy upgrade path: the wiki
    // directory on disk is preserved, so the index is rebuilt from it.
    "wiki_pages_fts",
    // Wiki index manifest: per-file content hashes used by
    // `wiki_check_for_updates` to detect external edits. A derived cache
    // (created by migration v002) that self-heals from disk on the next
    // check, exactly like `wiki_pages_fts`.
    "wiki_index_manifest",
];

/// Indexes created by migrations that must be dropped alongside their tables.
const DROP_INDEXES: &[&str] = &[
    "idx_articles_status",
    "idx_articles_status_year",
    "idx_articles_journal_index_id",
    "idx_articles_duplicate_of",
    "idx_articles_screened_at",
    "idx_articles_data_length",
    "idx_articles_sequence_id",
    "idx_articles_changed_at",
    "idx_audit_entries_article_id",
    "idx_criteria_type",
    "uq_ref_papers_doi",
    "uq_ref_papers_title_authors_year",
    "idx_ref_papers_match",
    "idx_ref_papers_matched_article",
    "idx_ref_links_parent",
    "idx_ref_links_paper",
    "idx_ref_links_parent_type",
    "idx_biblio_authors_norm",
    "idx_baa_article",
    "idx_baa_author",
    "idx_biblio_inst_norm",
    "idx_biblio_terms_norm",
    "idx_bat_article",
    "idx_bat_term",
    "idx_bnn_network",
    "idx_bne_network",
    // Article chunk storage (T1.2/T3.1)
    "idx_article_chunks_article",
    // Translation originals (Plan-A permanent rewrite)
    "idx_article_original_chunks_article",
    // Legacy v1 indexes
    "idx_article_references_parent_type",
    "idx_article_references_doi",
    "idx_article_references_match_status",
];

/// Drop all user tables and indexes, reset `user_version` to 0, and re-run
/// migrations from scratch. `journal_index` is preserved.
///
/// Safe to call on legacy, current, or fresh databases.
pub fn rebuild_schema(conn: &mut Connection) -> Result<(), AppError> {
    // PRAGMA foreign_keys cannot be changed inside a transaction; set it on the
    // connection before starting one.
    conn.execute("PRAGMA foreign_keys = OFF", [])?;

    {
        let tx = conn.transaction()?;

        let mut drop_sql = String::new();
        for table in DROP_TABLES {
            drop_sql.push_str(&format!("DROP TABLE IF EXISTS {};\n", table));
        }
        for idx in DROP_INDEXES {
            drop_sql.push_str(&format!("DROP INDEX IF EXISTS {};\n", idx));
        }
        tx.execute_batch(&drop_sql)?;

        tx.commit()?;
    }

    // Re-enable foreign keys (outside the transaction).
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Reset migration version so all migrations re-run from scratch.
    conn.pragma_update(None, "user_version", 0)?;

    // Re-run migrations to rebuild the clean schema.
    migration::run_migrations(conn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn rebuild_creates_current_schema_from_scratch() {
        let mut conn = Connection::open_in_memory().unwrap();
        rebuild_schema(&mut conn).unwrap();

        // Current tables must exist.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='articles'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='reference_papers'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn rebuild_wipes_legacy_article_references() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE articles (id TEXT PRIMARY KEY);
             CREATE TABLE article_references (
                id TEXT PRIMARY KEY,
                parent_id TEXT NOT NULL,
                type INTEGER NOT NULL
             );",
        )
        .unwrap();

        rebuild_schema(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='article_references'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
