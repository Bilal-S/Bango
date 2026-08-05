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

/// Get the current Bango documents root info.
///
/// All on-disk project artifacts derive from this root as subdirectories
/// (`fulltext/`, `ris/`, `wiki-root/`). Performs a one-time lazy migration
/// from the legacy `fulltext_storage_dir` key.
#[tauri::command]
pub fn get_storage_root(db_state: tauri::State<'_, DbState>) -> Result<StorageRootInfo, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    let effective_path = app_settings_repo::get_storage_root(&conn)?;
    let default_path = compute_default_storage_root();

    // `is_custom` is true when the stored `storage_root` differs from the
    // platform default. After lazy migration a derived-from-legacy default
    // path also equals `default_path`, so it correctly reports `false`.
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

/// Read the experimental auto-translate toggle. Defaults to `true` (enabled)
/// when the `auto_translate` key is absent. Powers the Settings -> AI Summaries
/// "Auto Translate" switch. Unlike the sibling localStorage-backed summary
/// toggles, this lives in the database so backend processing stages can read
/// it directly.
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
/// the key is absent or empty (today's priority-only behavior). Powers the
/// Criteria screen -> "Custom Screening Instructions" textarea.
#[tauri::command]
pub fn get_screening_custom_logic(
    db_state: tauri::State<'_, DbState>,
) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_screening_custom_logic(&conn)
}

/// Persist the custom screening-instructions text. The value is trimmed of
/// surrounding whitespace; an empty string is allowed and effectively
/// disables the feature.
#[tauri::command]
pub fn set_screening_custom_logic(
    db_state: tauri::State<'_, DbState>,
    value: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::set_screening_custom_logic(&conn, &value)
}

/// Read the user-editable project name shown in the Dashboard header. Returns
/// `null` when the key is absent or empty (the dashboard renders its
/// "Project Dashboard" fallback in that case). Travels with project backups
/// (portable).
#[tauri::command]
pub fn get_project_name(db_state: tauri::State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_project_name(&conn)
}

/// Persist the user-editable project name. The value is trimmed of surrounding
/// whitespace and hard-capped to `PROJECT_NAME_MAX_LEN` (50) chars as
/// defense-in-depth (the frontend `<input maxlength>` is the primary gate).
/// An empty/whitespace-only value clears the name (stored as NULL) so the
/// dashboard reverts to the "Project Dashboard" fallback, matching the
/// inline-edit "clear to reset" contract.
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
