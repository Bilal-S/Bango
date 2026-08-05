//! Database migration runner.
//!
//! **Transactional**: each migration's `up_sql` + `user_version` bump commit atomically.
//! **Self-healing** ([`heal_partial_migrations`]): detects half-applied states from
//! older non-transactional builds (v003 columns exist but `user_version` is stale),
//! advances the version without re-running the dangerous `ALTER TABLE ADD COLUMN`
//! statements (SQLite has no `IF NOT EXISTS` for ADD COLUMN).
//! Kept as a permanent safety net even though the transactional runner prevents new
//! partial states.

use rusqlite::Connection;

use super::migrations;
use crate::error::AppError;

/// Run all pending migrations in version order. Each migration runs inside a
/// single transaction so DDL + `user_version` bump commit atomically.
/// On failure the transaction rolls back and `user_version` stays at the last
/// successful value.
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    // First, heal any partial state left by older non-transactional builds.
    heal_partial_migrations(conn)?;

    let current_version: i32 =
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap_or(0);

    let migrations = migrations::get_migrations();

    for migration in migrations {
        if migration.version > current_version {
            // `unchecked_transaction` borrows `&Connection`, matching our fn signature.
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(migration.up_sql)?;
            tx.pragma_update(None, "user_version", migration.version)?;
            tx.commit()?;
        }
    }

    Ok(())
}

/// Returns `true` if `column` exists on `table`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Detect and heal databases left half-applied by pre-transactional-runner builds.
///
/// Older builds ran `execute_batch` + `pragma_update` as two autocommit statements.
/// A crash between them left the schema changed but `user_version` stale, causing
/// the next launch to re-run non-idempotent `ALTER TABLE ADD COLUMN` statements.
/// This pre-pass detects that state for v003 (the only migration carrying such ALTERs)
/// by checking whether `articles.is_translated` exists while `user_version < 3`.
/// If so, advances the version to 3 so the main loop skips the dangerous re-run.
fn heal_partial_migrations(conn: &Connection) -> Result<(), AppError> {
    let current_version: i32 =
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap_or(0);

    // v003 marker: `articles.is_translated`. If present while user_version < 3, the DDL
    // already committed — advance the version so the main loop skips the re-run.
    if current_version < 3 && column_exists(conn, "articles", "is_translated")? {
        eprintln!(
            "[migrations] heal: detected partially-applied v003 \
             (articles.is_translated exists, user_version={current_version}); \
             advancing user_version to 3 without re-running ALTERs"
        );
        conn.pragma_update(None, "user_version", 3)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn column_exists_detects_real_column() {
        let conn = mem_conn();
        conn.execute_batch("CREATE TABLE t (a TEXT, b INTEGER);").unwrap();
        assert!(column_exists(&conn, "t", "a").unwrap());
        assert!(column_exists(&conn, "t", "b").unwrap());
        assert!(!column_exists(&conn, "t", "c").unwrap());
    }

    #[test]
    fn column_exists_false_for_missing_table() {
        let conn = mem_conn();
        // PRAGMA table_info on a missing table returns no rows, not an error.
        assert!(!column_exists(&conn, "nope", "x").unwrap());
    }

    #[test]
    fn heal_advances_version_when_v003_column_present() {
        let conn = mem_conn();
        // Run the full chain (v001 + v002 + v003), then rewind user_version to
        // 2. This simulates the partial state: v003's DDL committed (including
        // `articles.is_translated`) but the version bump never landed.
        crate::db::migration::run_migrations(&conn).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        assert!(column_exists(&conn, "articles", "is_translated").unwrap());

        // heal should detect the marker and advance user_version to 3 without
        // re-running the dangerous ALTERs.
        heal_partial_migrations(&conn).unwrap();
        let v: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v, 3);
    }

    #[test]
    fn heal_is_noop_when_column_absent() {
        let conn = mem_conn();
        // Build a minimal `articles` table WITHOUT the v003 marker column and
        // set user_version=2. heal must NOT advance the version because the
        // marker is absent (v003 DDL never ran).
        conn.execute_batch(
            "CREATE TABLE articles (id TEXT PRIMARY KEY); \
             PRAGMA user_version = 2;",
        )
        .unwrap();
        assert!(!column_exists(&conn, "articles", "is_translated").unwrap());

        heal_partial_migrations(&conn).unwrap();
        let v: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v, 2, "heal must not advance version when marker column is absent");
    }

    #[test]
    fn heal_is_noop_when_version_already_current() {
        let conn = mem_conn();
        // Full migration chain applied (v001 + v002 + v003 + v004 + v005 +
        // v006 + v007 + v008): version=8, v003 marker column exists. heal must
        // be a no-op because the version is not stale (the heal pre-pass only
        // advances to 3 when the marker exists AND user_version < 3).
        crate::db::migration::run_migrations(&conn).unwrap();
        let v_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v_before, 8);

        heal_partial_migrations(&conn).unwrap();
        let v_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v_after, 8);
    }

    #[test]
    fn v008_restores_audit_entries_article_id_index() {
        let conn = mem_conn();
        crate::db::migration::run_migrations(&conn).unwrap();

        // The index exists after the full chain (v001 creates it, v003→v007
        // drop it via the CHECK-rebuild pattern, v008 restores it).
        let exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_audit_entries_article_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "idx_audit_entries_article_id must exist after v008");
    }
}
