//! Database migration runner.
//!
//! Migrations run on every app startup ([`crate::lib::run`] setup hook). The
//! runner is **transactional** and **self-healing**:
//!
//! - **Transactional**: each migration's `up_sql` + `user_version` bump are
//!   committed atomically in a single transaction. A crash between the DDL and
//!   the version bump cannot leave a partial state - the entire migration
//!   rolls back and retries cleanly on the next launch.
//! - **Self-healing pre-pass** ([`heal_partial_migrations`]): detects databases
//!   left in a half-applied state by older non-transactional builds (e.g. v003
//!   schema columns exist but `user_version` is still 2 because the app was
//!   force-closed between `execute_batch` and `pragma_update`). It advances
//!   `user_version` to the highest fully-applied migration without re-running
//!   the dangerous `ALTER TABLE ADD COLUMN` statements, which have no
//!   `IF NOT EXISTS` guard in SQLite.
//!
//! The heal pre-pass is a one-time bridge for installs corrupted by pre-fix
//! builds. Once every install has the transactional runner, no new partial
//! states can be created, but the pre-pass is kept as a cheap safety net.

use rusqlite::Connection;

use super::migrations;
use crate::error::AppError;

/// Run all pending migrations in version order.
///
/// Each migration runs inside a single transaction so the schema DDL and the
/// `user_version` bump commit atomically. If a migration fails, the
/// transaction rolls back, `user_version` stays at the last successful value,
/// and the caller can surface the error to the user without leaving the
/// database in a half-migrated state.
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    // First, heal any partial state left by older non-transactional builds.
    heal_partial_migrations(conn)?;

    let current_version: i32 =
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap_or(0);

    let migrations = migrations::get_migrations();

    for migration in migrations {
        if migration.version > current_version {
            // `unchecked_transaction` borrows `&Connection` (not `&mut`),
            // matching this function's signature and avoiding churn across the
            // 40+ call sites of `run_migrations`.
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

/// Detect and heal databases left in a half-applied migration state by
/// pre-transactional-runner builds.
///
/// ## Background
///
/// Older builds ran `execute_batch(up_sql)` and `pragma_update(user_version)`
/// as two separate autocommit statements. A crash or force-quit between them
/// left the schema changed but `user_version` stale, so the next launch
/// re-ran the migration and crashed on the non-idempotent `ALTER TABLE ADD
/// COLUMN` statements (SQLite has no `ADD COLUMN IF NOT EXISTS`).
///
/// This pre-pass detects that state for v003 (the only migration carrying
/// `ALTER TABLE ADD COLUMN` on `articles`) by checking whether
/// `articles.is_translated` exists while `user_version < 3`. If so, it
/// advances `user_version` to 3 and logs the recovery, letting the main loop
/// skip the dangerous re-run.
///
/// Once all installs run the transactional runner, no new partial states are
/// created. This pre-pass remains as a permanent, cheap safety net for the
/// existing corrupted installs.
fn heal_partial_migrations(conn: &Connection) -> Result<(), AppError> {
    let current_version: i32 =
        conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap_or(0);

    // v003 marker: `articles.is_translated`. If the column exists but the
    // version pragma is stale, the v003 DDL already committed - advance the
    // version so the main loop skips it.
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
        // v006): version=6, v003 marker column exists. heal must be a no-op
        // because the version is not stale (the heal pre-pass only advances to
        // 3 when the marker exists AND user_version < 3).
        crate::db::migration::run_migrations(&conn).unwrap();
        let v_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v_before, 6);

        heal_partial_migrations(&conn).unwrap();
        let v_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(v_after, 6);
    }
}
