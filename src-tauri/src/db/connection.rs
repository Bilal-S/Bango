use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

use crate::error::AppError;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

pub fn create_connection_at(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

/// Create an in-memory SQLite connection for testing.
pub fn create_connection() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}
