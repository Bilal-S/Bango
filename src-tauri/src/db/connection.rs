use rusqlite::Connection;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use crate::error::AppError;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

/// Threshold above which a `lock_conn` acquire is logged as "slow" via
/// `[screening:diag]`. 100ms is well above the normal uncontended acquire
/// (microseconds) and below the human-perceptible freeze threshold, so it
/// cleanly isolates mutex starvation (e.g. a long PDF-parse pass in Phase B
/// holding the DbState mutex) from normal DB work. Diagnostics-only: the
/// warning carries no behavioral effect.
const SLOW_LOCK_THRESHOLD_MS: u128 = 100;

/// Lock the shared SQLite connection, mapping a poisoned-mutex failure to
/// [`AppError::LockPoisoned`] (not [`AppError::Database`]) so the failure is
/// correctly categorized as an application-state error rather than a SQL error.
///
/// Every Tauri command handler and engine that locks `DbState.conn` MUST route
/// through this helper instead of inlining `.lock().map_err(...)`, so the error
/// mapping stays uniform and the poison-error duplication does not re-emerge.
///
/// Diagnostics: times the acquire and emits `[screening:diag] lock_conn: SLOW
/// acquire ({elapsed}ms)` when it exceeds [`SLOW_LOCK_THRESHOLD_MS`]. This is
/// the single most valuable signal for mutex-starvation hangs (e.g. the Phase B
/// chunk-backfill holding the DbState mutex across a long PDF-parse pass): every
/// other DB-touching IPC command will show a slow acquire while the holder is
/// busy, proving the freeze is mutex contention rather than a stuck SQL
/// statement. The log line is sample-unlimited because a sustained stall
/// produces one line per waiter per acquire, which is exactly the evidence we
/// want; in normal operation no line fires at all.
pub fn lock_conn(conn_mutex: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>, AppError> {
    let start = Instant::now();
    let guard = conn_mutex.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    let elapsed_ms = start.elapsed().as_millis();
    if elapsed_ms > SLOW_LOCK_THRESHOLD_MS {
        eprintln!(
            "[screening:diag] lock_conn: SLOW acquire ({elapsed_ms}ms) — likely mutex starvation"
        );
    }
    Ok(guard)
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
