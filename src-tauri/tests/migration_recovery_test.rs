//! Integration tests for migration crash-recovery.
//!
//! These tests simulate the exact partial-state corruption that older
//! non-transactional builds left behind: v003's `ALTER TABLE ADD COLUMN`
//! statements committed, but `user_version` stayed at 2 because the app was
//! force-closed between `execute_batch` and `pragma_update`. The pre-fix
//! runner re-ran v003 on the next launch and crashed with
//! "duplicate column name: is_translated".
//!
//! The transactional runner + `heal_partial_migrations` pre-pass must:
//! 1. Detect the partial state (marker column exists, version stale).
//! 2. Advance `user_version` to 3 without re-running the ALTERs.
//! 3. Leave the schema fully usable.

use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

/// Return the SQLite `user_version` pragma value.
fn user_version(conn: &Connection) -> i32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0)).expect("user_version")
}

/// Return true if `column` exists on `table`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).expect("prepare");
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).expect("query_map");
    for row in rows {
        if row.expect("row") == column {
            return true;
        }
    }
    false
}

/// Simulate the partial v003 state that crashed pre-fix builds:
/// v001+v002 fully applied, v003's marker column manually added, version=2.
fn build_partial_v003_state(conn: &Connection) {
    // Run the full chain once (in-memory fresh DB), then rewind to v002.
    run_migrations(conn).expect("initial migrations");
    // Roll user_version back to 2 - simulating the pre-fix runner having
    // committed v003's DDL but not reached the version bump.
    conn.pragma_update(None, "user_version", 2).expect("rewind to v2");
    // The v003 columns already exist from the full run above, so the
    // partial state is now: v003 schema present, user_version=2.
    // This is exactly what a crashed pre-fix build leaves behind.
    assert!(column_exists(conn, "articles", "is_translated"));
    assert_eq!(user_version(conn), 2);
}

#[test]
fn run_migrations_recovers_from_partial_v003_state() {
    let conn = create_connection().expect("connection");
    build_partial_v003_state(&conn);

    // Pre-fix: this would crash with "duplicate column name: is_translated".
    // Post-fix: the heal pre-pass detects the marker and advances the version.
    run_migrations(&conn).expect("recovery should succeed");

    // Version is now 3 - v003 was NOT re-run, just acknowledged.
    assert_eq!(user_version(&conn), 3);

    // All v003 schema artifacts are present and usable.
    assert!(column_exists(&conn, "articles", "is_translated"));
    assert!(column_exists(&conn, "articles", "translation_status"));
    assert!(column_exists(&conn, "articles", "translation_error"));
    assert!(column_exists(&conn, "articles", "translated_at"));

    let has_table = |name: &str| -> bool {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [name],
                |r| r.get(0),
            )
            .expect("table count");
        count > 0
    };
    assert!(has_table("article_original_content"));
    assert!(has_table("article_original_chunks"));
    assert!(has_table("article_chunks"));

    // The DB is fully usable: insert into `articles` exercises the translation
    // columns (NOT NULL defaults), and insert into `audit_entries` exercises
    // the expanded CHECK constraint.
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) \
         VALUES ('a1', 'T', 'A', 'Smith', 'working')",
        [],
    )
    .expect("insert article");
    conn.execute(
        "INSERT INTO audit_entries (id, action, source) \
         VALUES ('au1', 'translation', 'ai')",
        [],
    )
    .expect("insert translation audit row");
}

#[test]
fn run_migrations_is_idempotent_on_clean_db() {
    // Sanity check: running twice on a fresh DB does not fail (the existing
    // db_test.rs covers this too, but assert it here alongside the recovery
    // case so the contrast is explicit).
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("first run");
    let v1 = user_version(&conn);
    run_migrations(&conn).expect("second run");
    let v2 = user_version(&conn);
    assert_eq!(v1, v2, "idempotent: version must not change on re-run");
}

#[test]
fn run_migrations_on_fresh_db_has_full_translation_schema() {
    // Regression guard: the heal pre-pass must NOT skip v003 on a fresh DB
    // (where the marker column is absent). The full v003 DDL must run.
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    assert_eq!(user_version(&conn), 3);
    assert!(column_exists(&conn, "articles", "is_translated"));
    assert!(column_exists(&conn, "articles", "translation_status"));
}
