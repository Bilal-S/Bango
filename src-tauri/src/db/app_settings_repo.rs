use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension};

/// Get a setting value by key. Returns None if not found or value is NULL.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    let result = conn
        .query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten();
    Ok(result)
}

/// Set a setting value. Inserts if not exists, updates if exists.
pub fn set_setting(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

/// Get the fulltext storage directory. Returns the configured path or the platform default.
/// Also ensures the directory exists.
pub fn get_fulltext_storage_dir(conn: &Connection) -> Result<String, AppError> {
    let configured = get_setting(conn, "fulltext_storage_dir")?;

    let path = if let Some(ref p) = configured {
        if !p.is_empty() {
            p.clone()
        } else {
            compute_default_storage_dir()
        }
    } else {
        compute_default_storage_dir()
    };

    // Ensure directory exists
    std::fs::create_dir_all(&path).map_err(|e| {
        AppError::Import(format!("Failed to create fulltext storage directory '{}': {}", path, e))
    })?;

    Ok(path)
}

/// Set the fulltext storage directory. Pass None to reset to default.
pub fn set_fulltext_storage_dir(conn: &Connection, path: Option<&str>) -> Result<(), AppError> {
    let value = path.and_then(|p| if p.is_empty() { None } else { Some(p) });
    set_setting(conn, "fulltext_storage_dir", value)?;

    // Ensure the new directory exists
    if let Some(p) = value {
        std::fs::create_dir_all(p).map_err(|e| {
            AppError::Import(format!("Failed to create fulltext storage directory '{}': {}", p, e))
        })?;
    }

    Ok(())
}

/// Compute the platform-specific default storage directory:
/// ~/Documents/Bango/fulltext/
fn compute_default_storage_dir() -> String {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    docs.join("Bango").join("fulltext").to_string_lossy().to_string()
}
