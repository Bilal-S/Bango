use rusqlite::Connection;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use crate::error::AppError;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

/// Lock-acquire threshold (ms) for `[screening:diag]` slow-warn. 100ms
/// isolates mutex starvation (e.g. Phase B PDF-parse holding the DbState mutex)
/// from normal DB work. Diagnostics-only, no behavioral effect.
const SLOW_LOCK_THRESHOLD_MS: u128 = 100;

/// Lock the shared SQLite connection. Maps poisoned-mutex failures to
/// [`AppError::LockPoisoned`] (not [`AppError::Database`]). Every caller locking
/// `DbState.conn` MUST route through this helper.
///
/// Diagnostics: times the acquire; emits `[screening:diag] lock_conn: SLOW
/// acquire ({elapsed}ms)` above [`SLOW_LOCK_THRESHOLD_MS`] - the primary signal
/// for mutex-starvation hangs.
pub fn lock_conn(conn_mutex: &Mutex<Connection>) -> Result<MutexGuard<'_, Connection>, AppError> {
    let start = Instant::now();
    let guard = conn_mutex.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    let elapsed_ms = start.elapsed().as_millis();
    if elapsed_ms > SLOW_LOCK_THRESHOLD_MS {
        eprintln!(
            "[screening:diag] lock_conn: SLOW acquire ({elapsed_ms}ms) - likely mutex starvation"
        );
    }
    Ok(guard)
}

/// Lock any managed-state mutex that does not hold a DB `Connection`
/// (batch-import progress, chunk-rebuild progress). Maps poisoned-mutex
/// failures to [`AppError::LockPoisoned`] - the same contract as
/// [`lock_conn`] without the DB slow-acquire diagnostic (these locks guard
/// short in-memory state updates, not SQL work).
pub fn lock_state<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))
}

/// Lock a managed-state mutex, RECOVERING from poison by taking the inner
/// guard (`into_inner`). Reserved for intentionally-best-effort state slots
/// (scrape / wiki-ingest cancel tokens, the startup schema snapshot) where a
/// panic under the lock must not wedge the feature: the shared slot stays
/// readable/writable, which is preferable to propagating
/// [`AppError::LockPoisoned`]. Every call site MUST document why recovery is
/// the correct choice for that slot.
pub fn lock_state_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn create_connection_at(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    /* busy_timeout MUST be set before any multi-connection topology: without it,
    two connections contending for the single SQLite writer lock return
    SQLITE_BUSY immediately. 5000ms matches worst-case import transaction hold time. */
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}

/// Create an in-memory SQLite connection for testing (mirrors `create_connection_at`).
pub fn create_connection() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}
