//! Startup schema detection.
//! Legacy v1 (commit 665ec93) shipped `article_references`; current v1 creates
//! `reference_papers` + `article_reference_links` + `journal_index`. Both register
//! `user_version=1`, so we detect the legacy schema via `sqlite_master`, not the pragma.

use rusqlite::Connection;

use crate::error::AppError;

/// Startup schema probe result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaStatus {
    /// Current: has `reference_papers`, `article_reference_links`, and `journal_index`.
    Current,
    /// Legacy: has `article_references` and/or missing the current reference/journal tables.
    /// Requires a backup + rebuild upgrade.
    Legacy,
    /// Fresh: no user tables yet (migrations will create them).
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

/// Classify the live schema. Rules (checked in order):
/// 1. Legacy `article_references` present → `Legacy`.
/// 2. `articles` present but `reference_papers` absent → `Legacy`.
/// 3. No data tables → `FreshDb`.
/// 4. Otherwise → `Current`.
pub fn check_schema(conn: &Connection) -> Result<SchemaStatus, AppError> {
    let has_article_references = table_exists(conn, "article_references")?;
    let has_reference_papers = table_exists(conn, "reference_papers")?;
    let has_articles = table_exists(conn, "articles")?;

    if has_article_references {
        return Ok(SchemaStatus::Legacy);
    }

    // Older install with `articles` but none of the current reference tables → legacy.
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
