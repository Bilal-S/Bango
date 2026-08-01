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

    // Version is now 8 - v003 was NOT re-run (heal advanced to 3), then v004
    // + v005 + v006 + v007 + v008 ran normally on top.
    assert_eq!(user_version(&conn), 8);

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
    // Fresh DB runs the full chain through v008.
    assert_eq!(user_version(&conn), 8);
    assert!(column_exists(&conn, "articles", "is_translated"));
    assert!(column_exists(&conn, "articles", "translation_status"));
}

/// v006 must heal historical malformed audit rows that have
/// `article_id = ''` (empty string) instead of the correct `NULL`. Without
/// the heal UPDATE that runs before the orphan DELETE, the empty-string rows
/// would survive the orphan sweep (which only matches `article_id IS NOT
/// NULL`) and then crash the subsequent `INSERT ... SELECT` with
/// `FOREIGN KEY constraint failed (19)` when `PRAGMA foreign_keys=ON`.
///
/// This test simulates a pre-v006 DB carrying such a row: run the full chain
/// through v005, rewind to v005, insert the malformed row with FK off, then
/// run migrations to v006 and verify the row is preserved with
/// `article_id IS NULL`.
#[test]
fn v006_heals_empty_string_article_id_to_null() {
    let conn = create_connection().expect("connection");

    // Run the full chain once so the v005 audit_entries schema exists, then
    // rewind to v005 so v006 will run on the next `run_migrations` call.
    run_migrations(&conn).expect("initial migrations through v006");
    conn.pragma_update(None, "user_version", 5).expect("rewind to v5");

    // Seed one article + a bound audit entry (the legitimate case).
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) \
         VALUES ('survivor-1', 'S', 'A', 'Smith', 'working')",
        [],
    )
    .expect("seed article");
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
         VALUES ('bound-1', 'survivor-1', '2026-01-01T00:00:00Z', 'import', 'ok', 'system')",
        [],
    )
    .expect("seed bound audit");

    // Insert the malformed empty-string system entry. v005's audit_entries
    // has the FK constraint, so disable FK to allow the bad row in (mirrors
    // how it entered historical DBs: imports ran with FK off).
    conn.execute("PRAGMA foreign_keys = OFF", []).expect("disable fk");
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
         VALUES ('legacy-empty-1', '', '2026-01-01T00:00:00Z', 'error', 'legacy', 'system')",
        [],
    )
    .expect("seed empty-string audit");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    // Pre-condition: the malformed row exists with article_id = ''.
    let empty_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE id = 'legacy-empty-1' AND article_id = ''",
            [],
            |row| row.get(0),
        )
        .expect("count empty");
    assert_eq!(empty_count, 1, "pre: malformed empty-string row must exist");

    // Run v006 (and any later migrations). Pre-heal this would crash during
    // the `INSERT ... SELECT` rebuild with FOREIGN KEY constraint failed.
    run_migrations(&conn).expect("v006 heal should succeed");

    assert_eq!(user_version(&conn), 8);

    // The malformed row must be preserved (not dropped) AND normalized to NULL.
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_entries WHERE id = 'legacy-empty-1'", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(row_count, 1, "healed row must be preserved (not dropped by orphan sweep)");

    let is_null: bool = conn
        .query_row(
            "SELECT article_id IS NULL FROM audit_entries WHERE id = 'legacy-empty-1'",
            [],
            |row| row.get(0),
        )
        .expect("check null");
    assert!(is_null, "empty-string article_id must be healed to NULL by v006");

    // The legitimate bound entry must also survive the rebuild.
    let bound_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE id = 'bound-1' AND article_id = 'survivor-1'",
            [],
            |row| row.get(0),
        )
        .expect("count bound");
    assert_eq!(bound_count, 1, "legitimate bound audit entry must survive v006 rebuild");
}
