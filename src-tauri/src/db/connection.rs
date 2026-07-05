use rusqlite::Connection;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::error::AppError;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

/// Lock the shared SQLite connection, mapping a poisoned-mutex failure to
/// [`AppError::LockPoisoned`] (not [`AppError::Database`]) so the failure is
/// correctly categorized as an application-state error rather than a SQL error.
///
/// Every Tauri command handler and engine that locks `DbState.conn` MUST route
/// through this helper instead of inlining `.lock().map_err(...)`, so the error
/// mapping stays uniform and the poison-error duplication does not re-emerge.
pub fn lock_conn(conn_mutex: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>, AppError> {
    conn_mutex.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))
}

pub fn create_connection_at(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    // `busy_timeout` MUST be set on every connection before any multi-connection
    // topology is used (Tier 2 dedicated worker connection): without it, two
    // connections contending for the single SQLite writer lock return
    // `SQLITE_BUSY` immediately instead of waiting. 5000ms matches the typical
    // worst-case hold time of an import transaction.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}

/// Create an in-memory SQLite connection for testing.
pub fn create_connection() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    // Mirror `create_connection_at` so in-memory test fixtures behave the same
    // (busy_timeout is a no-op for an in-memory single-connection DB, but it
    // keeps the two constructors symmetric and documents intent).
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}
