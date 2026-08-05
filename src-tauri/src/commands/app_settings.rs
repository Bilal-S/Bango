use crate::db::app_settings_repo;
use crate::db::connection::DbState;
use crate::error::AppError;

/// Application-level feature flags exposed to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFlagsResponse {
    pub premium: bool,
    pub db_version: i32,
    pub db_max_version: i32,
}

/// Read the current application feature flags, including DB version info.
#[tauri::command]
pub fn get_app_flags(
    flags: tauri::State<'_, crate::AppFlags>,
    db_state: tauri::State<'_, DbState>,
) -> AppFlagsResponse {
    let (db_version, db_max_version) = read_db_versions(&db_state);
    AppFlagsResponse { premium: flags.premium, db_version, db_max_version }
}

fn read_db_versions(db_state: &DbState) -> (i32, i32) {
    let max = crate::db::migrations::get_migrations().last().map(|m| m.version).unwrap_or(0);

    let applied = match crate::db::connection::lock_conn(&db_state.conn) {
        Ok(conn) => {
            conn.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0)).unwrap_or(0)
        }
        Err(_) => 0,
    };

    (applied, max)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRootInfo {
    /// The effective Bango documents root (configured or default).
    pub effective_path: String,
    /// Whether a custom root has been configured (vs using default).
    pub is_custom: bool,
    /// The platform default root path (`~/Documents/Bango`).
    pub default_path: String,
}

/// All on-disk artifacts derive from this root (`fulltext/`, `ris/`,
/// `wiki-root/`). Performs a one-time lazy migration from the legacy
/// `fulltext_storage_dir` key.
#[tauri::command]
pub fn get_storage_root(db_state: tauri::State<'_, DbState>) -> Result<StorageRootInfo, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    let effective_path = app_settings_repo::get_storage_root(&conn)?;
    let default_path = compute_default_storage_root();

    /* `is_custom`: true when stored root differs from platform default.
     * After lazy migration, derived-from-legacy defaults also match, so
     * they correctly report `false`. */
    let is_custom = effective_path != default_path;

    Ok(StorageRootInfo { effective_path, is_custom, default_path })
}

/// Set a custom Bango documents root. Pass empty string or null to reset to
/// the platform default.
#[tauri::command]
pub fn set_storage_root(
    db_state: tauri::State<'_, DbState>,
    path: Option<String>,
) -> Result<StorageRootInfo, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    app_settings_repo::set_storage_root(&conn, path.as_deref())?;

    // Return updated info.
    let effective_path = app_settings_repo::get_storage_root(&conn)?;
    let default_path = compute_default_storage_root();
    let is_custom = effective_path != default_path;

    Ok(StorageRootInfo { effective_path, is_custom, default_path })
}

/// Read the auto-translate toggle. Defaults `true` when absent. Lives in DB
/// so backend processing stages can read it directly (unlike localStorage
/// summary toggles).
#[tauri::command]
pub fn get_auto_translate(db_state: tauri::State<'_, DbState>) -> Result<bool, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_auto_translate(&conn)
}

/// Persist the experimental auto-translate toggle.
#[tauri::command]
pub fn set_auto_translate(
    db_state: tauri::State<'_, DbState>,
    enabled: bool,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::set_auto_translate(&conn, enabled)
}

/// Read the optional custom screening-instructions text. Returns `null` when
/// absent or empty (priority-only behavior).
#[tauri::command]
pub fn get_screening_custom_logic(
    db_state: tauri::State<'_, DbState>,
) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_screening_custom_logic(&conn)
}

/// Persist the custom screening-instructions text. Trimmed; empty disables.
#[tauri::command]
pub fn set_screening_custom_logic(
    db_state: tauri::State<'_, DbState>,
    value: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::set_screening_custom_logic(&conn, &value)
}

/// Read the user-editable Dashboard title. `None` when absent/empty (the
/// dashboard shows "Project Dashboard"). Portable: travels with backups.
#[tauri::command]
pub fn get_project_name(db_state: tauri::State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_project_name(&conn)
}

/// Persist the Dashboard title. Trimmed + hard-capped at
/// `PROJECT_NAME_MAX_LEN` (50), defense-in-depth (frontend `<input maxlength>`
/// is primary). Empty/whitespace clears to NULL → fallback.
#[tauri::command]
pub fn set_project_name(
    db_state: tauri::State<'_, DbState>,
    value: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::set_project_name(&conn, &value)
}

/// Compute the platform default root: `~/Documents/Bango/`.
fn compute_default_storage_root() -> String {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    docs.join("Bango").to_string_lossy().to_string()
}
