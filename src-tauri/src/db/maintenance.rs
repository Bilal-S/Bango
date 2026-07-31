//! Database maintenance: reclaim disk space after bulk deletions.
//!
//! [`vacuum_database`] is the single entry point for shrinking the on-disk
//! SQLite files (`bango.db` + `bango.db-wal`) after a large wipe such as
//! `reset_project`. SQLite never auto-shrinks the database file: dropped tables
//! leave free pages that are reused for future writes but never returned to the
//! filesystem. `VACUUM` rebuilds the file so the space is reclaimed.
//!
//! ## WAL-mode interaction
//!
//! The database runs in WAL mode (`PRAGMA journal_mode=WAL`). A plain `VACUUM`
//! on a WAL-mode database writes the compacted pages to the WAL rather than
//! rewriting the main file in place, so the main `.db` file keeps its old size
//! and the total on-disk footprint does not shrink (it can even grow from
//! VACUUM overhead accumulating in the WAL).
//!
//! To actually shrink the file, the helper temporarily switches to DELETE
//! journal mode, which forces a full checkpoint and removes the `-wal`/`-shm`
//! files. `VACUUM` then runs in rollback-journal mode, definitively rewriting
//! and shrinking the main file. WAL mode is restored afterward so the
//! connection returns to its normal operating mode with fresh, empty sidecar
//! files.
//!
//! ## When to call
//!
//! Only at coarse, infrequent, destructive boundaries - primarily
//! `reset_project_inner` (Delete All Data). `VACUUM` is `O(n)` over the whole
//! database file (it rewrites every page), so it must NOT be called on
//! per-article deletes or other hot paths.

use rusqlite::Connection;

use crate::error::AppError;

/// Reclaim disk space by rebuilding the database file with `VACUUM`.
///
/// The sequence is:
/// 1. `PRAGMA journal_mode=DELETE` - switch out of WAL mode. This forces a
///    full checkpoint of the WAL into the main db file and deletes the
///    `bango.db-wal` and `bango.db-shm` sidecar files.
/// 2. `VACUUM` - rebuild the main db file (now in rollback-journal mode) so
///    free pages from dropped tables are returned to the filesystem.
/// 3. `PRAGMA journal_mode=WAL` - restore WAL mode, recreating fresh empty
///    sidecar files.
///
/// Each step runs **outside any transaction**: `VACUUM` cannot run inside a
/// transaction, and `journal_mode` changes cannot happen inside one either.
/// The caller (`reset_project_inner`) has already committed the schema rebuild
/// by the time this is invoked.
///
/// On an in-memory test connection, `journal_mode=DELETE` is a no-op (the mode
/// stays `memory`) and `VACUUM` is a harmless no-op for size, so the helper is
/// safe to call from in-memory test fixtures.
///
/// # Errors
///
/// Returns [`AppError::Database`] on any SQL failure. Callers wrapping
/// destructive operations (e.g. `reset_project_inner`) are expected to treat a
/// failure here as non-fatal: the data wipe has already happened, so a VACUUM
/// failure should be logged, not surfaced as a hard error.
pub fn vacuum_database(conn: &Connection) -> Result<(), AppError> {
    // 1. Switch to DELETE journal mode. On a WAL database this forces a full
    //    checkpoint and removes the -wal / -shm sidecar files so the VACUUM
    //    operates directly on the main file. On an in-memory DB the mode stays
    //    "memory" (the pragma is a no-op there).
    conn.query_row("PRAGMA journal_mode=DELETE", [], |_row| Ok(()))?;

    // 2. Rebuild the database file, reclaiming the free pages left by the
    //    dropped tables. In rollback-journal mode this definitively shrinks
    //    the main file.
    conn.execute("VACUUM", [])?;

    // 3. Restore WAL mode so the connection returns to its normal operating
    //    mode. This recreates fresh, empty -wal / -shm sidecar files.
    conn.query_row("PRAGMA journal_mode=WAL", [], |_row| Ok(()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::run_migrations;
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn vacuum_runs_clean_on_empty_migrated_db() {
        // VACUUM on a freshly migrated in-memory database is a no-op for size
        // but must execute without error. This guards the no-transaction
        // contract and the journal-mode round trip on the cheapest possible
        // fixture.
        let conn = test_db();
        vacuum_database(&conn).unwrap();
    }

    #[test]
    fn vacuum_runs_clean_after_dropping_user_tables() {
        // Mirror the reset_project_inner shape: drop the user tables, then
        // VACUUM. The helper must succeed even though the DB now has a mix of
        // surviving tables (journal_index) and dropped ones.
        let conn = test_db();
        conn.execute_batch(
            "DROP TABLE IF EXISTS articles;\n\
             DROP TABLE IF EXISTS audit_entries;\n\
             DROP TABLE IF EXISTS app_settings;\n",
        )
        .unwrap();
        vacuum_database(&conn).unwrap();
    }

    #[test]
    fn vacuum_restores_wal_mode_on_in_memory_db() {
        // On an in-memory DB the journal mode is "memory" and cannot be set to
        // WAL. The helper must not error when the final journal_mode=WAL pragma
        // is a no-op. (On a file-backed WAL DB the file-backed test in
        // tests/maintenance_test.rs proves the round trip restores WAL mode.)
        let conn = test_db();
        vacuum_database(&conn).unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "memory");
    }
}
