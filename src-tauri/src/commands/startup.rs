//! Startup upgrade orchestration.
//!
//! Exposes the schema-status probe and the one-shot legacy upgrade command to
//! the frontend. The upgrade backs up the legacy DB to `app_data_dir`, rebuilds
//! the schema, reloads the bundled journal index, and restores user data via
//! `project::import_project`. The backup file is intentionally never deleted so
//! the user can recover if anything goes wrong.

use std::fs;
use std::io::Write;

use tauri::{AppHandle, Manager, State};

use crate::db::connection::DbState;
use crate::db::rebuild;
use crate::db::schema_check::{check_schema, SchemaStatus};
use crate::error::AppError;
use crate::export::legacy_project::export_legacy_project;
use crate::export::project::import_project;

/// Startup schema classification stored as managed state at setup time so the
/// frontend can read it without re-probing the (still locked) connection.
#[derive(Debug, Clone, Copy)]
pub struct StartupStatus {
    pub schema: SchemaStatus,
}

/// Frontend-facing schema status. `needsLegacyUpgrade` is true only for the
/// `Legacy` case; `Current` and `FreshDb` are both safe to bootstrap from.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatusResponse {
    pub needs_legacy_upgrade: bool,
}

/// Read the startup schema status computed during setup.
#[tauri::command]
pub fn get_startup_status(status: State<'_, StartupStatus>) -> StartupStatusResponse {
    StartupStatusResponse { needs_legacy_upgrade: status.schema == SchemaStatus::Legacy }
}

/// Result of a successful legacy upgrade, returned to the frontend so it can
/// surface the backup path to the user (and log it).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyUpgradeResult {
    /// Absolute path to the backup file written under `app_data_dir`.
    pub backup_path: String,
    /// Number of articles restored.
    pub article_count: i64,
}

/// Perform the legacy -> current schema upgrade.
///
/// Steps (all on the live connection, in order):
/// 1. `export_legacy_project` -> JSON backup of the legacy DB.
/// 2. Write the backup to `app_data_dir/bango_legacy_backup_<ts>.bango.json`.
/// 3. `rebuild_schema` -> drop all tables, reset `user_version`, re-run
///    migrations (creates the current schema).
/// 4. Reload the bundled journal index into the freshly created table.
/// 5. `import_project(backup)` -> restore user data + auto journal rematch.
///
/// The backup file is never deleted by this command. If any step fails, the
/// returned `AppError` includes the backup path in its message when one was
/// already written, so the user can locate their data.
#[tauri::command]
pub fn perform_legacy_upgrade(
    app: AppHandle,
    db_state: State<'_, DbState>,
) -> Result<LegacyUpgradeResult, AppError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Import(format!("Failed to resolve app data dir for backup: {e}")))?;
    fs::create_dir_all(&app_data_dir).map_err(|e| {
        AppError::Import(format!("Failed to create app data dir {}: {e}", app_data_dir.display()))
    })?;

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let backup_path = app_data_dir.join(format!("bango_legacy_backup_{timestamp}.bango.json"));

    // 1. Export legacy DB to JSON, re-checking the schema under the same lock to
    //    confirm we're not double-upgrading.
    let backup_json = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let json = export_legacy_project(&conn).map_err(|e| {
            eprintln!("[legacy_upgrade] export_legacy_project failed: {e:?}");
            e
        })?;
        match check_schema(&conn) {
            Ok(SchemaStatus::Legacy) => {}
            Ok(other) => {
                eprintln!("[legacy_upgrade] schema is no longer legacy ({other:?}); aborting");
                return Err(AppError::Validation(format!(
                    "Database is not in the legacy state (got {other:?}); upgrade aborted."
                )));
            }
            Err(e) => return Err(e),
        }
        json
    };

    // 2. Persist the backup before touching the schema.
    let mut file = fs::File::create(&backup_path).map_err(|e| {
        AppError::Import(format!("Failed to create backup file {}: {e}", backup_path.display()))
    })?;
    file.write_all(backup_json.as_bytes()).map_err(|e| {
        AppError::Import(format!("Failed to write backup file {}: {e}", backup_path.display()))
    })?;
    eprintln!("[legacy_upgrade] backup written to {}", backup_path.display());

    // 3. Rebuild schema (drops all tables incl. legacy, re-runs migrations).
    {
        let mut conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        rebuild::rebuild_schema(&mut conn).map_err(|e| {
            let msg = format!(
                "Schema rebuild failed after backup was written to {}. Error: {e}",
                backup_path.display()
            );
            AppError::Import(msg)
        })?;
    }

    // 4. Reload the bundled journal index into the new empty table.
    if let Err(e) = crate::load_journal_index_if_empty_handle(&app) {
        eprintln!("[legacy_upgrade] warning: journal index reload failed: {e:#}");
    }

    // 5. Restore user data from the backup (also re-matches journals).
    let article_count = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        import_project(&conn, &backup_json).map_err(|e| {
            let msg = format!(
                "Data reload failed after schema rebuild. Your backup is safe at {}. Error: {e}",
                backup_path.display()
            );
            AppError::Import(msg)
        })?;
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0)).unwrap_or(0)
    };

    eprintln!(
        "[legacy_upgrade] completed; restored {article_count} articles from {}",
        backup_path.display()
    );

    Ok(LegacyUpgradeResult {
        backup_path: backup_path.to_string_lossy().to_string(),
        article_count,
    })
}
