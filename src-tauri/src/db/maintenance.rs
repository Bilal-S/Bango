//! Database maintenance: reclaim disk space after bulk deletions.
//!
//! [`vacuum_database`] shrinks the SQLite files after `reset_project`. SQLite never
//! auto-shrinks: dropped tables leave free pages reused for future writes but never
//! returned to the filesystem; `VACUUM` rebuilds the file so space is reclaimed.
//!
//! **WAL-mode interaction**: a plain `VACUUM` on a WAL DB writes compacted pages to the
//! WAL, not the main file. This helper temporarily switches to DELETE journal mode
//! (forcing a full checkpoint + removing `-wal`/`-shm`), runs `VACUUM` in rollback mode
//! (definitively shrinking the main file), then restores WAL. Call only at coarse,
//! destructive boundaries — `VACUUM` is `O(n)` over the whole file.

use rusqlite::Connection;

use crate::error::AppError;

/// Reclaim disk space via `VACUUM`. Sequence:
/// 1. Switch to DELETE journal mode (forces WAL checkpoint, removes `-wal`/`-shm`).
/// 2. `VACUUM` — rebuilds the main file in rollback mode, definitively shrinking it.
/// 3. Restore WAL mode (fresh empty sidecar files).
///
/// Each step runs outside any transaction (`VACUUM` + `journal_mode` changes don't
/// support transactions). On in-memory DBs step 1 is a no-op (mode stays `memory`).
///
/// # Errors
///
/// `AppError::Database` on SQL failure. Callers of destructive operations should treat
/// this as non-fatal (data wipe has already happened).
pub fn vacuum_database(conn: &Connection) -> Result<(), AppError> {
    // 1. Switch to DELETE journal mode — forces WAL checkpoint, removes sidecar files.
    conn.query_row("PRAGMA journal_mode=DELETE", [], |_row| Ok(()))?;

    // 2. Rebuild the database file, reclaiming free pages.
    conn.execute("VACUUM", [])?;

    // 3. Restore WAL mode with fresh empty sidecar files.
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
        // tests/db/maintenance_test.rs proves the round trip restores WAL mode.)
        let conn = test_db();
        vacuum_database(&conn).unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "memory");
    }
}
