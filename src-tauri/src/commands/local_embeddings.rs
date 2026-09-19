//! Tauri commands for the Bango Local embedding component manager.
//!
//! `get_local_embeddings_status`: state + paths + sizes for the Settings
//! panel. `install_local_embeddings`: gated (target, disk space), atomic,
//! cancellable background install emitting `embedding:component` progress
//! events; idempotent, so it doubles as repair. `verify_local_embeddings`:
//! full SHA-256 re-check (component details "Verify Installation").
//! `remove_local_embeddings`: deletes model + runtime artifacts.
//!
//! Lock discipline: the storage root is resolved under a brief lock burst
//! and released BEFORE any download; downloads never hold the DB mutex.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{Emitter, State};

use crate::db::app_settings_repo;
use crate::db::connection::{lock_conn, DbState};
use crate::embedding::backend::EmbeddingBackend;
use crate::embedding::local::download::{
    self, install_profile, install_runtime, InstallProgress, InstallReport, VerificationFailure,
};
use crate::embedding::local::engine::{EnginePaths, LocalEngine};
use crate::embedding::local::manifest::local_manifest;
use crate::embedding::local::paths::{resolve_ai_paths, MODEL_DIR_NAME};
use crate::embedding::local::profile::{
    LOCAL_EMBEDDING_DIMENSIONS, LOCAL_PROFILE_DIR, LOCAL_PROFILE_ID,
};
use crate::embedding::local::prompt::EmbeddingRole;
use crate::embedding::local::state::assess_installation;
use crate::error::AppError;

/// Managed state for the install lifecycle (one concurrent install; the
/// cancel token is snapshotted by the running install).
#[derive(Default)]
pub struct LocalEmbeddingsInstallState {
    pub cancel_token: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
}

/// Status payload for the Settings Embeddings card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalComponentStatus {
    /// "not_installed" | "repair_required" | "ready" | "installing" |
    /// "unsupported" (in that precedence; derived, not persisted).
    pub state: String,
    /// Whether an install/repair is currently in flight.
    pub running: bool,
    /// Whether the runtime library is installed at its pinned size (the
    /// engine's dylib gate; `state` stays model-scoped).
    pub runtime_ready: bool,
    pub profile: String,
    pub model: String,
    pub license: String,
    pub license_url: String,
    pub supported_target: bool,
    pub installed_bytes: u64,
    pub required_bytes: u64,
    pub download_bytes: u64,
    /// Pinned ONNX Runtime version (Component Details; `null` when the
    /// manifest carries no runtime for this target).
    pub runtime_version: Option<String>,
    /// CPU intra-op threads the local session will use (thread budget).
    pub thread_budget: usize,
    /// Actually-resolved model root (Component Details transparency).
    pub model_root: String,
    pub runtime_root: String,
    /// Whether the OneDrive fallback fired for the model root.
    pub used_fallback: bool,
}

/// Verification outcome for the Component Details "Verify Installation".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOutcome {
    pub healthy: bool,
    pub failures: Vec<VerificationFailure>,
}

fn emit_component(handle: &tauri::AppHandle, progress: &InstallProgress) {
    let _ = handle.emit("embedding:component", progress);
}

/// Read the component status (no install side effects). `state` maps the
/// plan-§5 machine without persisting it: `installing` (running flag) >
/// `unsupported` (target gate) > the fast health check
/// (`ready` / `repair_required` / `not_installed`).
#[tauri::command]
pub fn get_local_embeddings_status(
    db_state: State<'_, DbState>,
    install_state: State<'_, LocalEmbeddingsInstallState>,
) -> Result<LocalComponentStatus, AppError> {
    let manifest = local_manifest()?;
    let storage_root = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::get_storage_root(&conn)?
    };
    let paths = resolve_ai_paths(Path::new(&storage_root));
    let running = install_state.running.load(Ordering::SeqCst);
    let supported = download::supported_target();
    let state = if running {
        "installing".to_string()
    } else if !supported {
        "unsupported".to_string()
    } else {
        assess_installation(&paths.model_root, &manifest).as_str().to_string()
    };
    // Runtime readiness (T6 panel surfaces it; the engine's dylib gate uses
    // the same check): the current target's library at its pinned size.
    let runtime_ready = download::runtime_lib_file(&paths.runtime_root, &manifest)
        .zip(manifest.runtime_file_for_current_target())
        .is_some_and(|(lib, file)| download::runtime_library_healthy(&lib, file.lib_size));
    // Disk-gate derivation mirrors the install command: an existing profile
    // directory (repair/update stages a second copy), NOT the derived state -
    // `installing`/`unsupported` must not double the estimate when nothing
    // is installed.
    let is_installed = paths.model_root.join(LOCAL_PROFILE_DIR).exists();
    // Installed size whenever any install exists (a repair_required profile
    // still has files on disk - the panel should show what's there). The
    // healthy runtime library counts too (findings-7 4.4): without it the
    // panel understates the footprint relative to download_bytes.
    let runtime_lib_bytes = download::runtime_lib_file(&paths.runtime_root, &manifest)
        .zip(manifest.runtime_file_for_current_target())
        .filter(|(lib, file)| download::runtime_library_healthy(lib, file.lib_size))
        .map_or(0, |(lib, _)| std::fs::metadata(lib).map_or(0, |m| m.len()));
    let installed_bytes = if is_installed {
        manifest
            .files
            .iter()
            .map(|f| {
                std::fs::metadata(paths.model_root.join(LOCAL_PROFILE_DIR).join(&f.name))
                    .map_or(0, |m| m.len())
            })
            .sum::<u64>()
            + runtime_lib_bytes
    } else {
        runtime_lib_bytes
    };
    Ok(LocalComponentStatus {
        state,
        running,
        runtime_ready,
        profile: LOCAL_PROFILE_ID.to_string(),
        model: manifest.model.clone(),
        license: manifest.license.clone(),
        license_url: manifest.license_url.clone(),
        supported_target: supported,
        installed_bytes,
        required_bytes: manifest.required_disk_bytes(is_installed),
        // Full transfer cost: model files + the current target's runtime
        // archive (matches the install progress totals).
        download_bytes: manifest.total_install_bytes(),
        runtime_version: manifest
            .runtime_file_for_current_target()
            .map(|_| manifest.runtime.as_ref().map_or_else(String::new, |r| r.version.clone())),
        thread_budget: crate::embedding::local::thread_budget::embedding_thread_budget(
            std::thread::available_parallelism().map_or(4, std::num::NonZero::get),
        ),
        model_root: paths.model_root.to_string_lossy().to_string(),
        runtime_root: paths.runtime_root.to_string_lossy().to_string(),
        used_fallback: paths.used_fallback,
    })
}

/// Read the embedding backend selection for this machine
/// (`configured_provider` | `bango_local`).
#[tauri::command]
pub fn get_embedding_backend(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    Ok(app_settings_repo::get_embedding_backend(&conn)?.as_str().to_string())
}

/// Set the embedding backend selection. The capability triple resets to
/// `unknown` so the next probe (offline for local, HTTP for cloud)
/// re-evaluates under the new backend - selection never implies readiness.
#[tauri::command]
pub fn set_embedding_backend(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<LocalEngine>>,
    backend: String,
) -> Result<String, AppError> {
    let parsed = EmbeddingBackend::parse_exact(&backend)
        .ok_or_else(|| AppError::Validation(format!("unknown embedding backend: '{backend}'")))?;
    let conn = lock_conn(&db_state.conn)?;
    app_settings_repo::set_embedding_backend(&conn, parsed)?;
    app_settings_repo::reset_embedding_status(&conn)?;
    // L8 (findings-7, partial): drop any loaded local session on a switch so
    // the on-device footprint does not outlive the selection (an idle-time
    // unload stays out of scope; the ORT env/DLL remain process-resident by
    // design - see embedding/AGENTS.md).
    engine.reset();
    Ok(parsed.as_str().to_string())
}

/// RAII guard so `running` clears even when the command returns early.
struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Install (or repair) the local embedding components. Idempotent: files
/// already matching their pins are skipped. Emits `embedding:component`
/// progress events; cancellable via `cancel_local_embeddings_install`.
#[tauri::command]
pub async fn install_local_embeddings(
    app_handle: tauri::AppHandle,
    db_state: State<'_, DbState>,
    install_state: State<'_, LocalEmbeddingsInstallState>,
    engine: State<'_, Arc<LocalEngine>>,
) -> Result<InstallReport, AppError> {
    if install_state.running.swap(true, Ordering::SeqCst) {
        return Err(AppError::Validation(
            "A local embeddings installation is already running.".to_string(),
        ));
    }
    let _guard = RunningGuard(Arc::clone(&install_state.running));
    // Reset the cancel token right after winning the running slot: a Cancel
    // click during the gates below is honored (it aborts at the first
    // in-flight check) instead of being silently overwritten.
    install_state.cancel_token.store(false, Ordering::SeqCst);
    if !download::supported_target() {
        return Err(AppError::Validation(
            "Bango Local is not available for this system. You can continue using your \
             configured embedding provider."
                .to_string(),
        ));
    }
    let manifest = local_manifest()?;
    // Brief lock: resolve the storage root, then release before any download.
    let storage_root = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::get_storage_root(&conn)?
    };
    let paths = resolve_ai_paths(Path::new(&storage_root));
    std::fs::create_dir_all(&paths.model_root)
        .map_err(|e| AppError::Import(format!("cannot create model directory: {e}")))?;
    // Repair/update stages a full second copy while the old install lives,
    // so the gate doubles the payload when an installation already exists.
    let existing_install = paths.model_root.join(LOCAL_PROFILE_DIR).exists();
    let required = manifest.required_disk_bytes(existing_install);
    match download::available_bytes(&paths.model_root) {
        Some(free) if free < required => {
            return Err(AppError::Validation(format!(
                "Not enough disk space: about {} MB required, {} MB available.",
                required / 1_048_576,
                free / 1_048_576
            )));
        }
        Some(_) => {}
        None => {
            // Probe failed (exotic volume): the gate is skipped, not silent.
            eprintln!(
                "[local-embeddings] disk-space probe failed for {}; skipping the free-space gate",
                paths.model_root.display(),
            );
        }
    }
    let cancel = Arc::clone(&install_state.cancel_token);
    let handle = app_handle.clone();
    // L3 (findings-7): both phases report against ONE combined total with a
    // per-phase offset so the UI bar is monotonic 0-100 (the model phase's
    // own overall counters are remapped; the runtime phase adds the model
    // payload as its baseline). Skipped-but-healthy files jump the bar
    // forward, never backward.
    let total = manifest.total_install_bytes();
    let model_phase_total = manifest.total_download_bytes();
    let result = install_profile(
        &paths.model_root,
        &manifest,
        &|progress| {
            let mut mapped = progress;
            mapped.overall_total = total;
            emit_component(&handle, &mapped);
        },
        &cancel,
    )
    .await;
    let mut report = match result {
        Ok(report) => report,
        Err(e) => {
            emit_component(
                &app_handle,
                &InstallProgress {
                    phase: "error".to_string(),
                    file: String::new(),
                    file_bytes: 0,
                    file_total: 0,
                    overall_bytes: 0,
                    overall_total: total,
                    message: Some(e.to_string()),
                },
            );
            return Err(e);
        }
    };

    // Step 2: the ONNX Runtime component (archive download + extraction;
    // idempotent - an existing library skips). Only when the manifest pins a
    // runtime for this target.
    if manifest.runtime_file_for_current_target().is_some() {
        let handle = app_handle.clone();
        let runtime_report = install_runtime(
            &paths.runtime_root,
            &manifest,
            &|progress| {
                let mut mapped = progress;
                mapped.overall_bytes += model_phase_total;
                mapped.overall_total = total;
                emit_component(&handle, &mapped);
            },
            &cancel,
        )
        .await;
        let runtime_report = match runtime_report {
            Ok(report) => report,
            Err(e) => {
                // Terminal state on the progress stream (Q3): the runtime
                // step failed after the model phase already reported.
                emit_component(
                    &app_handle,
                    &InstallProgress {
                        phase: "error".to_string(),
                        file: String::new(),
                        file_bytes: 0,
                        file_total: 0,
                        overall_bytes: 0,
                        overall_total: total,
                        message: Some(format!("ONNX Runtime install failed: {e}")),
                    },
                );
                return Err(e);
            }
        };
        report.downloaded += runtime_report.downloaded;
        report.skipped += runtime_report.skipped;
        report.bytes_downloaded += runtime_report.bytes_downloaded;
    }

    // Step 3 (plan §5 self-test): drop any stale session, then load fresh
    // and embed a probe. A failure surfaces as an install error (components
    // stay on disk; the status command will show repair_required).
    engine.reset();
    let engine_paths = EnginePaths {
        model_root: paths.model_root.clone(),
        runtime_root: paths.runtime_root.clone(),
    };
    let probe = engine.embed(&engine_paths, &["probe".to_string()], EmbeddingRole::Query).await;
    let probe_dims = match probe {
        Ok((_, dims)) if dims == LOCAL_EMBEDDING_DIMENSIONS as i32 => dims,
        Ok((_, dims)) => {
            let message = format!(
                "local embedding self-test returned {dims} dimensions, expected \
                 {LOCAL_EMBEDDING_DIMENSIONS}"
            );
            emit_component(
                &app_handle,
                &InstallProgress {
                    phase: "error".to_string(),
                    file: String::new(),
                    file_bytes: 0,
                    file_total: 0,
                    overall_bytes: 0,
                    overall_total: total,
                    message: Some(message.clone()),
                },
            );
            return Err(AppError::Import(message));
        }
        Err(e) => {
            emit_component(
                &app_handle,
                &InstallProgress {
                    phase: "error".to_string(),
                    file: String::new(),
                    file_bytes: 0,
                    file_total: 0,
                    overall_bytes: 0,
                    overall_total: total,
                    message: Some(format!("local embedding self-test failed: {e}")),
                },
            );
            return Err(e);
        }
    };
    let _ = probe_dims;

    // Step 4: persist the capability triple when the local backend is the
    // ACTIVE selection, so recall/generation gates open without a cloud
    // probe. Cloud-selected installs leave the cloud triple untouched.
    {
        let conn = lock_conn(&db_state.conn)?;
        if app_settings_repo::get_embedding_backend(&conn)? == EmbeddingBackend::BangoLocal {
            app_settings_repo::set_embedding_status(
                &conn,
                crate::db::app_settings_repo::EmbeddingStatus::Enabled,
                LOCAL_PROFILE_ID,
                LOCAL_EMBEDDING_DIMENSIONS as i32,
            )?;
        }
    }

    emit_component(
        &app_handle,
        &InstallProgress {
            phase: "done".to_string(),
            file: String::new(),
            file_bytes: 0,
            file_total: 0,
            overall_bytes: total,
            overall_total: total,
            message: Some(format!(
                "Installed {} file(s) ({} skipped).",
                report.downloaded, report.skipped
            )),
        },
    );
    Ok(report)
}

/// Cancel a running install (checked between chunks and before renames).
#[tauri::command]
pub fn cancel_local_embeddings_install(
    install_state: State<'_, LocalEmbeddingsInstallState>,
) -> Result<(), AppError> {
    install_state.cancel_token.store(true, Ordering::SeqCst);
    Ok(())
}

/// Full SHA-256 re-verification of the installed profile (hashes ~218 MB,
/// so the hashing runs on the blocking pool; the command is async so it
/// never freezes the main thread). Guarded: rejected while an install runs.
#[tauri::command]
pub async fn verify_local_embeddings(
    db_state: State<'_, DbState>,
    install_state: State<'_, LocalEmbeddingsInstallState>,
) -> Result<VerifyOutcome, AppError> {
    if install_state.running.load(Ordering::SeqCst) {
        return Err(AppError::Validation(
            "A local embeddings installation is running; verify afterwards.".to_string(),
        ));
    }
    let manifest = local_manifest()?;
    let storage_root = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::get_storage_root(&conn)?
    };
    let paths = resolve_ai_paths(Path::new(&storage_root));
    let model_root = paths.model_root.to_path_buf();
    let runtime_root = paths.runtime_root.to_path_buf();
    let failures = tokio::task::spawn_blocking(move || {
        // Full verification covers BOTH components: model files against
        // their pins, and the runtime library against its pinned size.
        let mut failures = download::verify_installed(&model_root, &manifest).unwrap_or_default();
        if let Err(message) = download::verify_runtime(&runtime_root, &manifest) {
            failures.push(VerificationFailure { name: "onnxruntime".to_string(), reason: message });
        }
        failures
    })
    .await
    .map_err(|e| AppError::Import(format!("verify task panicked: {e}")))?;
    Ok(VerifyOutcome { healthy: failures.is_empty(), failures })
}

/// Remove all local embedding artifacts (model + staging + runtime), from
/// every candidate model root. Guarded: rejected while an install runs.
/// The deletion runs on the blocking pool (~218 MB across roots).
#[tauri::command]
pub async fn remove_local_embeddings(
    db_state: State<'_, DbState>,
    install_state: State<'_, LocalEmbeddingsInstallState>,
    engine: State<'_, Arc<LocalEngine>>,
) -> Result<(), AppError> {
    if install_state.running.load(Ordering::SeqCst) {
        return Err(AppError::Validation(
            "A local embeddings installation is running; cancel it first.".to_string(),
        ));
    }
    let storage_root = {
        let conn = lock_conn(&db_state.conn)?;
        let root = app_settings_repo::get_storage_root(&conn)?;
        // If the local backend was active, its capability triple dies with
        // the artifacts (reset to unknown so a later selection re-probes).
        if app_settings_repo::get_embedding_backend(&conn)? == EmbeddingBackend::BangoLocal {
            app_settings_repo::reset_embedding_status(&conn)?;
        }
        root
    };
    // Drop any loaded session so the library file is not held open.
    engine.reset();
    let paths = resolve_ai_paths(Path::new(&storage_root));
    // Sweep EVERY candidate root: the resolved one, the app-data fallback,
    // and ALWAYS `{storage_root}/model` - a pre-move install is orphaned
    // there when the OneDrive fallback is now active (and vice versa).
    let mut roots = vec![paths.model_root.clone(), Path::new(&storage_root).join(MODEL_DIR_NAME)];
    if let Some(base) = dirs::data_local_dir() {
        roots.push(base.join("Bango").join("ai").join("models"));
    }
    roots.sort();
    roots.dedup();
    let runtime_root = paths.runtime_root.clone();
    tokio::task::spawn_blocking(move || {
        let refs: Vec<&Path> = roots.iter().map(std::convert::AsRef::as_ref).collect();
        download::remove_components(&refs, &runtime_root)
    })
    .await
    .map_err(|e| AppError::Import(format!("remove task panicked: {e}")))?
}
