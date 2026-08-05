//! Startup upgrade orchestration.
//!
//! Exposes the schema-status probe and the one-shot legacy upgrade command.
//! The upgrade backs up the legacy DB to `app_data_dir`, rebuilds the schema,
//! reloads the bundled journal index, and restores user data via
//! `project::import_project`. The backup file is never deleted.
//!
//! Loop-safety (1.0.26 -> 2.x upgrade): a webview `window.location.reload()`
//! runs in the SAME Rust process, so managed state is NOT recomputed.
//! `get_startup_status` re-probes the LIVE schema on every call (falling back
//! to the snapshot only if the live probe errors), and `perform_legacy_upgrade`
//! updates the managed snapshot after a successful rebuild. Either layer alone
//! breaks the loop; both together make it structurally impossible.

use std::fs;
use std::io::Write;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::db::connection::DbState;
use crate::db::rebuild;
use crate::db::schema_check::{check_schema, SchemaStatus};
use crate::error::AppError;
use crate::export::legacy_project::export_legacy_project;
use crate::export::project::import_project;

/// Startup schema classification captured in `lib.rs` setup so the frontend
/// has a snapshot before any command runs. Wrapped in `Mutex` so
/// `perform_legacy_upgrade` can keep it honest after rebuilding the schema
/// (layer 2 of loop-safety).
///
/// `get_startup_status` does NOT rely on this snapshot as source of truth
/// at query time: it re-probes the live schema. The snapshot exists only as a
/// fallback (if the live probe fails) and as observability.
#[derive(Debug)]
pub struct StartupStatus {
    pub schema: Mutex<SchemaStatus>,
}

impl StartupStatus {
    /// Read the current snapshot value. A poisoned mutex is treated as
    /// `Legacy` (fail-safe: the upgrade would rather re-run than silently skip).
    fn snapshot(&self) -> SchemaStatus {
        self.schema.lock().map(|g| *g).unwrap_or(SchemaStatus::Legacy)
    }

    /// Update the snapshot after a schema change (e.g. successful upgrade).
    fn set(&self, status: SchemaStatus) {
        if let Ok(mut g) = self.schema.lock() {
            *g = status;
        }
    }
}

/// Frontend-facing schema status. `needsLegacyUpgrade` is true only for the
/// `Legacy` case; `Current` and `FreshDb` are both safe to bootstrap from.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatusResponse {
    pub needs_legacy_upgrade: bool,
}

/// Decide whether the legacy upgrade is needed, given a live DB probe result
/// and the setup-time snapshot fallback. Pure function for unit testing without
/// a Tauri runtime.
///
/// Returns `true` only if the live probe says `Legacy`, OR (only when the
/// live probe itself errored) the snapshot said `Legacy`.
#[must_use]
pub fn legacy_upgrade_needed(live: Result<SchemaStatus, AppError>, fallback: SchemaStatus) -> bool {
    match live {
        Ok(status) => status == SchemaStatus::Legacy,
        Err(_) => fallback == SchemaStatus::Legacy,
    }
}

/// Read the startup schema status. Re-probes the LIVE schema so a successful
/// `perform_legacy_upgrade` is immediately reflected even though the webview
/// reloads in the same process (layer 1 of loop-safety). If the live probe
/// errors, falls back to the setup-time snapshot.
#[tauri::command]
pub fn get_startup_status(
    db_state: State<'_, DbState>,
    status: State<'_, StartupStatus>,
) -> StartupStatusResponse {
    let snapshot = status.snapshot();
    let live =
        crate::db::connection::lock_conn(&db_state.conn).and_then(|conn| check_schema(&conn));
    StartupStatusResponse { needs_legacy_upgrade: legacy_upgrade_needed(live, snapshot) }
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
/// 6. Update the managed `StartupStatus` snapshot so the post-reload
///    `get_startup_status` call agrees with the live schema.
///
/// The backup file is never deleted. If any step fails, the returned
/// `AppError` includes the backup path in its message when one was already
/// written, so the user can locate their data.
#[tauri::command]
pub fn perform_legacy_upgrade(
    app: AppHandle,
    db_state: State<'_, DbState>,
    startup_status: State<'_, StartupStatus>,
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
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
        let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        import_project(&conn, &backup_json).map_err(|e| {
            let msg = format!(
                "Data reload failed after schema rebuild. Your backup is safe at {}. Error: {e}",
                backup_path.display()
            );
            AppError::Import(msg)
        })?;
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0)).unwrap_or(0)
    };

    // 6. Keep the managed snapshot honest. The schema is now Current; publishing
    //    that here means `get_startup_status` returns false even if its live
    //    probe later fails for some reason (defense-in-depth against the reload
    //    loop). Re-probe under the lock to avoid races with concurrent callers.
    {
        let post_status =
            crate::db::connection::lock_conn(&db_state.conn).and_then(|conn| check_schema(&conn));
        match post_status {
            Ok(s) => startup_status.set(s),
            Err(e) => eprintln!(
                "[legacy_upgrade] warning: post-upgrade schema probe failed (snapshot not updated): {e:#}"
            ),
        }
    }

    eprintln!(
        "[legacy_upgrade] completed; restored {article_count} articles from {}",
        backup_path.display()
    );

    Ok(LegacyUpgradeResult {
        backup_path: backup_path.to_string_lossy().to_string(),
        article_count,
    })
}
