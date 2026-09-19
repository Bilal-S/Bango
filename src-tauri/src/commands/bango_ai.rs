//! Bango AI commands (T5): status, runtime-first install with an executable
//! smoke and engine self-test, cancel/verify/remove, Test Bango AI, and the
//! LLM backend selection.
//!
//! Lock discipline: storage root + settings resolve in brief DB bursts and
//! are released before any download, spawn, or HTTP call. The engine owns
//! its own internal locks; command code never holds a DB guard across an
//! `.await`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::app_settings_repo;
use crate::db::connection::{lock_conn, DbState};
use crate::error::AppError;
use crate::llm::backend::LlmBackend;
use crate::llm::local::engine::{
    model_path, BangoAiEngine, EngineConfig, EngineSettings, ServerSpec,
};
use crate::llm::local::hardware::{assess, detect, HardwareProfile, HardwareVerdict};
use crate::llm::local::install::{
    assess_model, assess_runtime, install_model_profile, install_runtime_bundle, is_cancelled,
    remove_components, server_path, verify_model, verify_runtime,
};
use crate::llm::local::manifest::{local_manifest, BangoAiManifest};
use crate::llm::local::profile::{LOCAL_LLM_ENGINE_LABEL, LOCAL_LLM_PROFILE_ID};
use crate::llm::orchestrator::LlmOrchestrator;
use crate::local_ai::download::{
    available_bytes, InstallProgress, InstallReport, VerificationFailure,
};
use crate::local_ai::paths::{resolve_ai_paths, AiPaths};
use crate::local_ai::state::Assessment;

/// Install slot state (mirrors the embedding install state).
#[derive(Default)]
pub struct BangoAiInstallState {
    pub cancel_token: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
}

/// Clears the running flag on every exit path.
struct RunningGuard(Arc<AtomicBool>);

impl RunningGuard {
    fn try_acquire(running: &Arc<AtomicBool>) -> Option<Self> {
        running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(running.clone()))
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Status payload for the Bango AI panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BangoAiStatus {
    /// Derived install state: `installing` > `unsupported` > `ready` |
    /// `repair_required` | `not_installed`.
    pub state: String,
    pub running: bool,
    pub supported_target: bool,
    pub engine_state: String,
    pub busy: bool,
    pub profile: String,
    pub model: String,
    pub license: String,
    pub license_url: String,
    pub engine: String,
    pub runtime_version: String,
    pub runtime_ready: bool,
    pub model_ready: bool,
    pub installed_bytes: u64,
    pub required_bytes: u64,
    pub download_bytes: u64,
    pub context: i32,
    pub threads: usize,
    pub reasoning: bool,
    pub hardware: HardwareProfile,
    pub verdict: HardwareVerdict,
    pub model_root: String,
    pub runtime_root: String,
    pub used_fallback: bool,
    pub log_path: String,
    pub backend: String,
}

/// Verify/remove outcome.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOutcome {
    pub healthy: bool,
    pub failures: Vec<VerificationFailure>,
}

/// Test Bango AI timings (information only).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BangoAiTestOutcome {
    pub model_load_ms: u64,
    pub response_ms: u64,
    pub tokens_per_second: f64,
    pub tokens: u64,
    pub effective_context: i32,
    pub detail: String,
}

/// Install result summary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BangoAiInstallOutcome {
    pub state: String,
    pub runtime_bytes: u64,
    pub model_bytes: u64,
    pub message: String,
}

/// Engine log path inside the AI artifact tree.
#[must_use]
pub fn log_path(paths: &AiPaths) -> PathBuf {
    paths.runtime_root.join("logs").join("llama-server.log")
}

/// Engine settings from the persisted machine-local keys (recommended
/// defaults when absent).
fn persisted_settings(conn: &rusqlite::Connection) -> EngineSettings {
    app_settings_repo::get_bango_ai_settings(conn).unwrap_or_default()
}

/// Build the status payload (testable without Tauri state).
pub fn build_status(
    conn: &rusqlite::Connection,
    engine: &BangoAiEngine,
    manifest: &BangoAiManifest,
    paths: &AiPaths,
    running: bool,
) -> Result<BangoAiStatus, AppError> {
    let backend = app_settings_repo::get_llm_backend(conn)?;
    let supported = crate::local_ai::download::supported_target();
    let runtime_assessment = assess_runtime(&paths.runtime_root, manifest);
    let model_assessment = assess_model(&paths.model_root, manifest);
    let runtime_ready = runtime_assessment == Assessment::Ready;
    let model_ready = model_assessment == Assessment::Ready;
    let state = if running {
        "installing"
    } else if !supported {
        "unsupported"
    } else if runtime_ready && model_ready {
        "ready"
    } else if runtime_assessment == Assessment::NotInstalled
        && model_assessment == Assessment::NotInstalled
    {
        "not_installed"
    } else {
        "repair_required"
    };
    let hardware = detect();
    let settings = engine.settings()?;
    let required_bytes = manifest.required_disk_bytes(
        runtime_assessment != Assessment::NotInstalled
            || model_assessment != Assessment::NotInstalled,
    );
    Ok(BangoAiStatus {
        state: state.to_string(),
        running,
        supported_target: supported,
        engine_state: engine.state()?.as_str().to_string(),
        busy: engine.is_busy()?,
        profile: manifest.profile.clone(),
        model: manifest.model.clone(),
        license: manifest.license.clone(),
        license_url: manifest.license_url.clone(),
        engine: LOCAL_LLM_ENGINE_LABEL.to_string(),
        runtime_version: manifest.runtime.version.clone(),
        runtime_ready,
        model_ready,
        installed_bytes: directory_bytes(&paths.model_root) + directory_bytes(&paths.runtime_root),
        required_bytes,
        download_bytes: manifest.total_install_bytes(),
        context: settings.context,
        threads: settings.threads,
        reasoning: settings.reasoning,
        // The verdict uses the same REPAIR-AWARE required bytes as the
        // preflight (`required_disk_bytes(existing)`), not the full-download
        // total - otherwise a repair needed only 1.5 GB still reads
        // "insufficient disk" on machines short of the full 7.3 GB (F11).
        verdict: assess(&hardware, (required_bytes / (1024 * 1024)).max(1)),
        hardware,
        model_root: paths.model_root.display().to_string(),
        runtime_root: paths.runtime_root.display().to_string(),
        used_fallback: paths.used_fallback,
        log_path: log_path(paths).display().to_string(),
        backend: backend.as_str().to_string(),
    })
}

fn directory_bytes(root: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries.filter_map(std::result::Result::ok).map(|entry| file_bytes(&entry.path())).sum()
}

fn file_bytes(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if meta.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        return entries.filter_map(std::result::Result::ok).map(|e| file_bytes(&e.path())).sum();
    }
    0
}

/// Preflight gates: unsupported target or short disk fail before any byte.
/// The model root and the runtime root are checked separately so the error
/// names WHICH root is short (aifixes1 F11); an unavailable reading for a root
/// is not a blocker (unknown != insufficient).
pub fn install_preflight(
    supported: bool,
    model_free_bytes: Option<u64>,
    runtime_free_bytes: Option<u64>,
    required_bytes: u64,
) -> Result<(), AppError> {
    if !supported {
        return Err(AppError::Validation(
            "Bango AI is not available for this system. You can continue using your \
             configured provider."
                .to_string(),
        ));
    }
    const GB: u64 = 1024 * 1024 * 1024;
    for (label, free) in [("model", model_free_bytes), ("runtime", runtime_free_bytes)] {
        if let Some(free) = free {
            if free < required_bytes {
                return Err(AppError::Validation(format!(
                    "Not enough free disk space where the {} is stored: {} GB available, {} GB \
                     required. Free up space or choose a different storage location in Settings.",
                    label,
                    free / GB,
                    required_bytes / GB
                )));
            }
        }
    }
    Ok(())
}

/// One progress event with monotonic overall bytes for the runtime-first
/// transaction (`base` is everything already installed this run).
#[must_use]
pub fn stage_progress(
    phase: &str,
    file: &str,
    base: u64,
    file_bytes: u64,
    file_total: u64,
    total: u64,
) -> InstallProgress {
    InstallProgress {
        phase: phase.to_string(),
        file: file.to_string(),
        file_bytes,
        file_total,
        overall_bytes: base + file_bytes.min(file_total),
        overall_total: total,
        message: None,
    }
}

/// Advance the stage base after a completed component.
#[must_use]
pub fn next_stage_base(base: u64, component_bytes: u64) -> u64 {
    base.saturating_add(component_bytes)
}

/// Bounded wall clock for the runtime executable smoke (aifixes1 F8: a
/// hanging binary must not hang the install).
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Run the extracted server with `--version` (validates AV/Gatekeeper/deps
/// before the 5.8 GB model download). Bounded by `SMOKE_TIMEOUT`.
pub fn smoke_runtime(server: &Path) -> Result<String, AppError> {
    use std::io::Read;
    let mut child = std::process::Command::new(server)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            AppError::Import(format!(
                "The AI engine could not start on this computer: {e}. On macOS, check \
                 Gatekeeper; on Windows, check antivirus settings."
            ))
        })?;
    let deadline = Instant::now() + SMOKE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AppError::Import(
                        "The AI engine startup check timed out. Verify the installation or \
                         reinstall."
                            .to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(AppError::Import(format!(
                    "The AI engine failed its startup check: {e}"
                )));
            }
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if !status.success() {
        return Err(AppError::Import(
            "The AI engine failed its startup check. Verify the installation or reinstall."
                .to_string(),
        ));
    }
    Ok(format!("{stdout}{stderr}").lines().next().unwrap_or("llama-server").trim().to_string())
}

/// Start the engine and run one tiny completion (the install self-test).
pub async fn self_test(
    engine: &BangoAiEngine,
    spec: &ServerSpec,
) -> Result<EngineConfig, AppError> {
    let config = engine.ensure_started(spec).await?;
    chat_probe(&config, Duration::from_secs(300)).await?;
    Ok(config)
}

/// One non-streaming completion against the loopback server.
pub async fn chat_probe(config: &EngineConfig, timeout: Duration) -> Result<u64, AppError> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| AppError::Import(format!("HTTP client init failed: {e}")))?;
    let body = serde_json::json!({
        "model": config.model,
        "messages": [{"role": "user", "content": "Reply with the single word: ready"}],
        "temperature": 0.0,
    });
    let response = client
        .post(format!("{}/chat/completions", config.endpoint))
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("Bango AI did not respond: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        let hint = if status.as_u16() == 400 {
            " Reduce the Context setting if the prompt is too long."
        } else {
            ""
        };
        return Err(AppError::Import(format!(
            "Bango AI returned an error ({status}).{hint} {text}"
        )));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Import(format!("Bango AI returned invalid JSON: {e}")))?;
    Ok(json["usage"]["completion_tokens"].as_u64().unwrap_or(0))
}

/// Runtime-first install transaction (no DB access; caller persists state).
/// `smoke` runs the extracted server before the model downloads (injected so
/// tests can prove the ordering without a real binary).
pub async fn install_bango_ai_inner(
    paths: &AiPaths,
    engine: &BangoAiEngine,
    manifest: &BangoAiManifest,
    smoke: &(dyn Fn(&Path) -> Result<String, AppError> + Send + Sync),
    progress: &(dyn Fn(InstallProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<BangoAiInstallOutcome, AppError> {
    // Stop a running engine first (aifixes1 F15): a repair install replaces
    // the binary/model the engine is using (locked DLLs on Windows), and the
    // self-test restarts it on the promoted artifacts.
    engine.stop().await?;

    let archive = manifest.archive_for_current_target().ok_or_else(|| {
        AppError::Validation(
            "Bango AI is not available for this system. You can continue using your \
             configured provider."
                .to_string(),
        )
    })?;
    let runtime_total = archive.size;
    let model_total = manifest.files.iter().map(|f| f.size).sum::<u64>();
    let total = runtime_total + model_total;

    let runtime_report: InstallReport = install_runtime_bundle(
        &paths.runtime_root,
        manifest,
        &|mut event| {
            event.overall_bytes = event.overall_bytes.min(runtime_total);
            event.overall_total = total;
            progress(event);
        },
        cancel,
    )
    .await?;
    let base = next_stage_base(0, runtime_total);
    progress(stage_progress(
        "verifying",
        LOCAL_LLM_ENGINE_LABEL,
        base,
        runtime_total,
        runtime_total,
        total,
    ));

    // Executable smoke: the small runtime surfaces AV/Gatekeeper/dependency
    // failures before the 5.8 GB model download. `--version` returns in
    // milliseconds, so the brief blocking call is acceptable here.
    let server = server_path(&paths.runtime_root, &manifest.runtime.version);
    // Cancellation before the smoke (F8): no work after this point.
    if is_cancelled(cancel) {
        return Err(AppError::Validation(
            "Bango AI setup was cancelled. You can resume later.".to_string(),
        ));
    }
    smoke(&server)?;

    let model_report = install_model_profile(
        &paths.model_root,
        manifest,
        &|event| {
            let remapped = InstallProgress {
                phase: event.phase.clone(),
                file: event.file.clone(),
                file_bytes: event.file_bytes,
                file_total: event.file_total,
                overall_bytes: base + event.overall_bytes.min(model_total),
                overall_total: total,
                message: event.message.clone(),
            };
            progress(remapped);
        },
        cancel,
    )
    .await?;

    let spec =
        ServerSpec { binary: server, model: model_path(&paths.model_root), log: log_path(paths) };
    // Cancellation before the model self-test (F8).
    if is_cancelled(cancel) {
        return Err(AppError::Validation(
            "Bango AI setup was cancelled. You can resume later.".to_string(),
        ));
    }
    self_test(engine, &spec).await?;

    Ok(BangoAiInstallOutcome {
        state: "ready".to_string(),
        runtime_bytes: runtime_report.bytes_downloaded,
        model_bytes: model_report.bytes_downloaded,
        message: "Bango AI is ready.".to_string(),
    })
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Read the Bango AI status payload.
#[tauri::command]
pub fn get_bango_ai_status(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
    install_state: State<'_, BangoAiInstallState>,
) -> Result<BangoAiStatus, AppError> {
    let manifest = local_manifest()?;
    let conn = lock_conn(&db_state.conn)?;
    let storage_root = app_settings_repo::get_storage_root(&conn)?;
    let paths = resolve_ai_paths(Path::new(&storage_root));
    build_status(
        &conn,
        engine.inner(),
        &manifest,
        &paths,
        install_state.running.load(Ordering::Acquire),
    )
}

/// Install Bango AI (runtime -> smoke -> model -> self-test). `activate`
/// persists the backend selection only after the self-test passes.
#[tauri::command]
pub async fn install_bango_ai(
    app: AppHandle,
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    install_state: State<'_, BangoAiInstallState>,
    activate: bool,
) -> Result<BangoAiInstallOutcome, AppError> {
    let Some(_guard) = RunningGuard::try_acquire(&install_state.running) else {
        return Err(AppError::Validation("A Bango AI install is already running.".to_string()));
    };
    install_state.cancel_token.store(false, Ordering::Release);
    let cancel = install_state.cancel_token.clone();

    let (paths, settings) = {
        let conn = lock_conn(&db_state.conn)?;
        let storage_root = app_settings_repo::get_storage_root(&conn)?;
        (resolve_ai_paths(Path::new(&storage_root)), persisted_settings(&conn))
    };
    engine.update_settings(settings)?;

    let manifest = local_manifest()?;
    let existing = assess_runtime(&paths.runtime_root, &manifest) != Assessment::NotInstalled
        || assess_model(&paths.model_root, &manifest) != Assessment::NotInstalled;
    install_preflight(
        crate::local_ai::download::supported_target(),
        available_bytes(&paths.model_root),
        available_bytes(&paths.runtime_root),
        manifest.required_disk_bytes(existing),
    )?;

    let app_for_events = app.clone();
    let emit = move |event: InstallProgress| {
        let _ = app_for_events.emit("bango_ai:component", &event);
    };
    let outcome =
        install_bango_ai_inner(&paths, engine.inner(), &manifest, &smoke_runtime, &emit, &cancel)
            .await;

    match outcome {
        Ok(result) => {
            {
                let conn = lock_conn(&db_state.conn)?;
                persist_backend_after_install_conn(&conn, activate, true)?;
            }
            activate_orchestrator_backend(orchestrator.inner(), activate, true).await;
            let _ = app.emit(
                "bango_ai:component",
                &InstallProgress {
                    phase: "done".to_string(),
                    file: String::new(),
                    file_bytes: 0,
                    file_total: 0,
                    overall_bytes: 1,
                    overall_total: 1,
                    message: Some(result.message.clone()),
                },
            );
            Ok(result)
        }
        Err(e) => {
            {
                let conn = lock_conn(&db_state.conn)?;
                persist_backend_after_install_conn(&conn, activate, false)?;
            }
            let _ = app.emit(
                "bango_ai:component",
                &InstallProgress {
                    phase: "error".to_string(),
                    file: String::new(),
                    file_bytes: 0,
                    file_total: 0,
                    overall_bytes: 0,
                    overall_total: 1,
                    message: Some(e.to_string()),
                },
            );
            Err(e)
        }
    }
}

/// Cancel a running install.
#[tauri::command]
pub fn cancel_bango_ai_install(install_state: State<'_, BangoAiInstallState>) {
    install_state.cancel_token.store(true, Ordering::Release);
}

/// Verify the installed components (sizes + model SHA-256).
#[tauri::command]
pub async fn verify_bango_ai(
    db_state: State<'_, DbState>,
    install_state: State<'_, BangoAiInstallState>,
) -> Result<VerifyOutcome, AppError> {
    if install_state.running.load(Ordering::Acquire) {
        return Err(AppError::Validation("Cannot verify while an install is running.".to_string()));
    }
    let (paths, manifest) = {
        let conn = lock_conn(&db_state.conn)?;
        let storage_root = app_settings_repo::get_storage_root(&conn)?;
        (resolve_ai_paths(Path::new(&storage_root)), local_manifest()?)
    };
    let outcome = tokio::task::spawn_blocking(move || -> Result<VerifyOutcome, AppError> {
        let mut failures = verify_runtime(&paths.runtime_root, &manifest)?;
        failures.extend(verify_model(&paths.model_root, &manifest)?);
        Ok(VerifyOutcome { healthy: failures.is_empty(), failures })
    })
    .await
    .map_err(|e| AppError::Import(format!("verify task panicked: {e}")))??;
    Ok(outcome)
}

/// Remove all Bango AI artifacts (stops the engine first).
#[tauri::command]
pub async fn remove_bango_ai(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
    install_state: State<'_, BangoAiInstallState>,
) -> Result<(), AppError> {
    if install_state.running.load(Ordering::Acquire) {
        return Err(AppError::Validation("Cannot remove while an install is running.".to_string()));
    }
    engine.inner().stop().await?;
    let (mut roots, runtime_root, version) = {
        let conn = lock_conn(&db_state.conn)?;
        let storage_root = app_settings_repo::get_storage_root(&conn)?;
        let paths = resolve_ai_paths(Path::new(&storage_root));
        let manifest = local_manifest()?;
        let mut roots = vec![paths.model_root.clone()];
        if let Some(fallback) = dirs::data_local_dir() {
            let fallback = fallback.join("Bango").join("ai").join("models");
            if !roots.contains(&fallback) {
                roots.push(fallback);
            }
        }
        (roots, paths.runtime_root.clone(), manifest.runtime.version.clone())
    };
    roots.sort();
    roots.dedup();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        remove_components(&refs, &runtime_root, &version)
    })
    .await
    .map_err(|e| AppError::Import(format!("remove task panicked: {e}")))??;
    Ok(())
}

/// Test Bango AI: start the engine (if needed) and run one tiny completion.
#[tauri::command]
pub async fn test_bango_ai(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
) -> Result<BangoAiTestOutcome, AppError> {
    let paths = {
        let conn = lock_conn(&db_state.conn)?;
        let storage_root = app_settings_repo::get_storage_root(&conn)?;
        resolve_ai_paths(Path::new(&storage_root))
    };
    let settings = {
        let conn = lock_conn(&db_state.conn)?;
        persisted_settings(&conn)
    };
    engine.update_settings(settings)?;
    let manifest = local_manifest()?;
    let spec = ServerSpec {
        binary: server_path(&paths.runtime_root, &manifest.runtime.version),
        model: model_path(&paths.model_root),
        log: log_path(&paths),
    };
    let load_start = Instant::now();
    let config = engine.ensure_started(&spec).await?;
    let model_load_ms = load_start.elapsed().as_millis() as u64;
    let response_start = Instant::now();
    let tokens = chat_probe(&config, Duration::from_secs(300)).await?;
    let response_ms = response_start.elapsed().as_millis() as u64;
    let seconds = (response_ms as f64 / 1000.0).max(0.001);
    Ok(BangoAiTestOutcome {
        model_load_ms,
        response_ms,
        tokens_per_second: tokens as f64 / seconds,
        tokens,
        effective_context: config.context,
        detail: format!("{} is ready.", LOCAL_LLM_ENGINE_LABEL),
    })
}

/// Read the generation backend selection.
#[tauri::command]
pub fn get_llm_backend(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    Ok(app_settings_repo::get_llm_backend(&conn)?.as_str().to_string())
}

/// Persist the generation backend selection, then reset the engine. The DB
/// guard is released before the awaited engine reset.
#[tauri::command]
pub async fn set_llm_backend(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
    orchestrator: State<'_, Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    backend: String,
) -> Result<(), AppError> {
    let parsed = LlmBackend::parse_exact(&backend)
        .ok_or_else(|| AppError::Validation(format!("unknown backend: '{backend}'")))?;
    let stored = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::set_llm_backend(&conn, parsed)?;
        match parsed {
            LlmBackend::ConfiguredProvider => {
                crate::db::llm_config_repo::get_config_no_decrypt(&conn)?
            }
            LlmBackend::BangoAi => None,
        }
    };
    orchestrator.inner().set_backend(parsed, stored.as_ref()).await;
    engine.inner().reset_off_thread().await
}

/// Persist the backend only when activation was requested AND the install
/// succeeded (decision 16: cancel or failure leaves the previous backend).
pub fn persist_backend_after_install_conn(
    conn: &rusqlite::Connection,
    activate: bool,
    succeeded: bool,
) -> Result<(), AppError> {
    if activate && succeeded {
        app_settings_repo::set_llm_backend(conn, LlmBackend::BangoAi)?;
    }
    Ok(())
}

/// Switch the live orchestrator after a successful activation install so
/// generation reaches the local engine without a restart or radio toggle.
/// Must run with no DB guard held: `set_backend` awaits the semaphore resize.
pub async fn activate_orchestrator_backend(
    orchestrator: &LlmOrchestrator,
    activate: bool,
    succeeded: bool,
) {
    if activate && succeeded {
        orchestrator.set_backend(LlmBackend::BangoAi, None).await;
    }
}

/// Engine settings payload for the Advanced panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BangoAiSettingsDto {
    pub context: i32,
    pub threads: usize,
    pub reasoning: bool,
}

/// Read the machine-local engine settings.
#[tauri::command]
pub fn get_bango_ai_settings(db_state: State<'_, DbState>) -> Result<BangoAiSettingsDto, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    let settings = app_settings_repo::get_bango_ai_settings(&conn)?;
    Ok(BangoAiSettingsDto {
        context: settings.context,
        threads: settings.threads,
        reasoning: settings.reasoning,
    })
}

/// Persist engine settings; a process-visible change (context/threads)
/// resets the running engine off-thread so the next start applies it.
#[tauri::command]
pub async fn set_bango_ai_settings(
    db_state: State<'_, DbState>,
    engine: State<'_, Arc<BangoAiEngine>>,
    context: i32,
    threads: usize,
    reasoning: bool,
) -> Result<(), AppError> {
    use crate::llm::local::policy::{clamp_context, EngineSettings, MAX_LLM_THREADS};
    let settings = EngineSettings {
        context: clamp_context(context),
        threads: threads.clamp(1, MAX_LLM_THREADS),
        reasoning,
    };
    {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::set_bango_ai_settings(&conn, &settings)?;
    }
    let previous = engine.inner().settings()?;
    engine.inner().update_settings(settings.clone())?;
    if previous.context != settings.context || previous.threads != settings.threads {
        engine.inner().reset_off_thread().await?;
    }
    Ok(())
}

/// The pinned profile id (exposed for tests and the status payload).
#[must_use]
pub fn profile_id() -> &'static str {
    LOCAL_LLM_PROFILE_ID
}
