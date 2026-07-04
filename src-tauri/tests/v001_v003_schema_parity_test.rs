//! v001 fresh-DB vs v003 migrated-DB schema parity tests.
//!
//! Option B (gap remediation): verifies the FINAL post-migration state has
//! all translation tables, columns, and the expanded audit_entries CHECK
//! constraint. The original plan called for comparing v001-only vs fully
//! migrated `sqlite_master` output, but SQLite `ALTER TABLE ADD COLUMN` has no
//! `IF NOT EXISTS` guard, so updating v001 to declare the columns would make
//! v003's ALTERs fail on fresh installs with "duplicate column name". Instead,
//! v001 stays as the legacy base and this test asserts that running the full
//! sequential migration chain (v001 -> v002 -> v003) produces the complete
//! translation schema. This catches real drift (e.g. someone breaks v003)
//! without the duplicate-column risk.

use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

/// Assert that a column exists on `articles` with the expected type.
fn assert_articles_column(conn: &Connection, name: &str, expected_type: &str) {
    let mut stmt =
        conn.prepare("PRAGMA table_info(articles)").expect("PRAGMA table_info(articles) prepares");
    let mut found = false;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
        .expect("query_map runs");
    for row in rows {
        let (col_name, col_type) = row.expect("row reads");
        if col_name == name {
            assert_eq!(
                col_type, expected_type,
                "articles.{name} type mismatch: expected {expected_type}, got {col_type}"
            );
            found = true;
            break;
        }
    }
    assert!(found, "articles.{name} column must exist after migration");
}

/// Assert that a table exists.
fn assert_table_exists(conn: &Connection, name: &str) {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |row| row.get(0),
        )
        .expect("table existence query runs");
    assert_eq!(exists, 1, "table {name} must exist after migration");
}

#[test]
fn translation_tables_match_between_v001_and_v003() {
    // Run the full migration chain on a fresh in-memory DB.
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");

    // Translation originals tables exist.
    assert_table_exists(&conn, "article_original_content");
    assert_table_exists(&conn, "article_original_chunks");

    // The 4 translation columns on `articles` exist.
    assert_articles_column(&conn, "is_translated", "INTEGER");
    assert_articles_column(&conn, "translation_status", "TEXT");
    assert_articles_column(&conn, "translation_error", "TEXT");
    assert_articles_column(&conn, "translated_at", "TEXT");

    // The audit_entries CHECK constraint accepts 'translation' and
    // 'translation_error' actions. We verify this by inserting rows with
    // those actions; a too-narrow CHECK would reject them.
    let audit_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO audit_entries (id, timestamp, action, source) \
         VALUES (?1, '2026-01-01T00:00:00Z', 'translation', 'ai')",
        [&audit_id],
    )
    .expect("'translation' action accepted by audit_entries CHECK");
    let audit_id2 = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO audit_entries (id, timestamp, action, source) \
         VALUES (?1, '2026-01-01T00:00:00Z', 'translation_error', 'ai')",
        [&audit_id2],
    )
    .expect("'translation_error' action accepted by audit_entries CHECK");

    // The article_chunks table (reverted v002 content carried by v003) exists.
    assert_table_exists(&conn, "article_chunks");
}
