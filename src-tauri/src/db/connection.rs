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
