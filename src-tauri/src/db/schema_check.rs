//! Startup schema detection.
//!
//! The legacy v1 build (commit 665ec93) shipped a single `article_references`
//! table. The current v1 migration creates `reference_papers` +
//! `article_reference_links` + `journal_index` instead. Because both old and
//! new builds register `user_version = 1`, `migration::run_migrations` silently
//! skips the new DDL on an existing legacy install. We therefore detect the
//! legacy schema by inspecting `sqlite_master` rather than relying on the
//! version pragma.

use rusqlite::Connection;

use crate::error::AppError;

/// Result of the startup schema probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaStatus {
    /// Schema matches the current migration set (has `reference_papers`,
    /// `article_reference_links`, and `journal_index`).
    Current,
    /// Legacy schema detected (has `article_references` and/or is missing the
    /// current reference/journal tables). Requires a backup + rebuild upgrade.
    Legacy,
    /// Brand-new database with no user tables yet (migrations will create them).
    FreshDb,
}

/// Returns true if a table named `name` exists in `sqlite_master`.
fn table_exists(conn: &Connection, name: &str) -> Result<bool, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Probe the live schema and classify it.
///
/// Classification rules (checked in order):
/// 1. If the legacy `article_references` table is present -> `Legacy`.
/// 2. If the current `reference_papers` table is absent but `articles` is
///    present -> `Legacy` (partial/older install missing new tables).
/// 3. If none of the data tables exist -> `FreshDb`.
/// 4. Otherwise -> `Current`.
pub fn check_schema(conn: &Connection) -> Result<SchemaStatus, AppError> {
    let has_article_references = table_exists(conn, "article_references")?;
    let has_reference_papers = table_exists(conn, "reference_papers")?;
    let has_articles = table_exists(conn, "articles")?;

    if has_article_references {
        return Ok(SchemaStatus::Legacy);
    }

    // An older install that has articles but none of the current reference
    // tables is also legacy.
    if has_articles && !has_reference_papers {
        return Ok(SchemaStatus::Legacy);
    }

    if !has_articles {
        return Ok(SchemaStatus::FreshDb);
    }

    Ok(SchemaStatus::Current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn fresh_database_is_fresh() {
        let conn = mem_conn();
        assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::FreshDb);
    }

    #[test]
    fn current_schema_is_current() {
        let conn = mem_conn();
        crate::db::migration::run_migrations(&conn).unwrap();
        assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Current);
    }

    #[test]
    fn legacy_article_references_table_is_legacy() {
        let conn = mem_conn();
        conn.execute_batch("CREATE TABLE articles (id TEXT PRIMARY KEY);").unwrap();
        conn.execute_batch(
            "CREATE TABLE article_references (
                id TEXT PRIMARY KEY,
                parent_id TEXT NOT NULL,
                type INTEGER NOT NULL,
                title TEXT
            );",
        )
        .unwrap();
        assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Legacy);
    }

    #[test]
    fn articles_without_reference_papers_is_legacy() {
        let conn = mem_conn();
        conn.execute_batch("CREATE TABLE articles (id TEXT PRIMARY KEY);").unwrap();
        assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Legacy);
    }
}
