use rusqlite::Connection;

use crate::error::AppError;
use super::migrations;

pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    let current_version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0);

    let migrations = migrations::get_migrations();

    for migration in migrations {
        if migration.version > current_version {
            conn.execute_batch(&migration.up_sql)?;
            conn.pragma_update(None, "user_version", migration.version)?;
        }
    }

    Ok(())
}
