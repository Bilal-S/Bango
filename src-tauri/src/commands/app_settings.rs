use crate::db::app_settings_repo;
use crate::db::connection::DbState;
use crate::error::AppError;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullTextStorageInfo {
    /// The effective directory path (configured or default)
    pub effective_path: String,
    /// Whether a custom path has been configured (vs using default)
    pub is_custom: bool,
    /// The platform default path
    pub default_path: String,
}

/// Get the current fulltext storage directory info
#[tauri::command]
pub fn get_fulltext_storage_dir(
    db_state: tauri::State<'_, DbState>,
) -> Result<FullTextStorageInfo, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let configured = app_settings_repo::get_setting(&conn, "fulltext_storage_dir")?;
    let default_path = compute_default_storage_dir();

    let is_custom = configured.as_ref().is_some_and(|p| !p.is_empty());
    let effective_path = if is_custom {
        configured.unwrap()
    } else {
        default_path.clone()
    };

    // Ensure directory exists
    std::fs::create_dir_all(&effective_path).map_err(|e| {
        AppError::Import(format!(
            "Failed to create fulltext storage directory '{}': {}",
            effective_path, e
        ))
    })?;

    Ok(FullTextStorageInfo {
        effective_path,
        is_custom,
        default_path,
    })
}

/// Set a custom fulltext storage directory. Pass empty string or null to reset to default.
#[tauri::command]
pub fn set_fulltext_storage_dir(
    db_state: tauri::State<'_, DbState>,
    path: Option<String>,
) -> Result<FullTextStorageInfo, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let value = path.as_deref().and_then(|p| if p.is_empty() { None } else { Some(p) });
    app_settings_repo::set_fulltext_storage_dir(&conn, value)?;

    // Return updated info
    let configured = app_settings_repo::get_setting(&conn, "fulltext_storage_dir")?;
    let default_path = compute_default_storage_dir();

    let is_custom = configured.as_ref().is_some_and(|p| !p.is_empty());
    let effective_path = if is_custom {
        configured.unwrap()
    } else {
        default_path.clone()
    };

    Ok(FullTextStorageInfo {
        effective_path,
        is_custom,
        default_path,
    })
}

/// Compute the platform default: ~/Documents/Bango/fulltext/
fn compute_default_storage_dir() -> String {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    docs.join("Bango").join("fulltext").to_string_lossy().to_string()
}