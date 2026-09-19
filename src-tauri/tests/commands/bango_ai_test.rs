//! Bango AI command tests (T5): status, install preflight, verify/remove
//! idempotency, backend switching, monotonic progress, runtime-smoke ordering,
//! and activate-after-success.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bango_lib::commands::bango_ai::{
    activate_orchestrator_backend, build_status, install_bango_ai_inner, install_preflight,
    next_stage_base, persist_backend_after_install_conn, profile_id, stage_progress,
};
use bango_lib::db::app_settings_repo::{
    get_bango_ai_settings, get_llm_backend, set_bango_ai_settings, set_llm_backend,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::llm::backend::LlmBackend;
use bango_lib::llm::local::engine::{
    BangoAiEngine, EngineSettings, HealthProbe, ServerProcess, ServerSpawner, ServerSpec,
};
use bango_lib::llm::local::install::{remove_components, verify_model, verify_runtime};
use bango_lib::llm::local::manifest::{
    local_manifest, BangoAiManifest, LlamaRuntimeManifest, RuntimeArchive, RuntimeMember,
};
use bango_lib::llm::orchestrator::LlmOrchestrator;
use bango_lib::local_ai::manifest::PinnedFile;
use bango_lib::local_ai::paths::resolve_ai_paths_with_base;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

// ── Engine fakes (the production seams are public) ──────────────────────────

struct FakeProcess {
    killed: Arc<AtomicBool>,
}

impl ServerProcess for FakeProcess {
    fn has_exited(&mut self) -> Result<bool, AppError> {
        Ok(false)
    }
    fn kill(&mut self) -> Result<(), AppError> {
        self.killed.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Default)]
struct FakeSpawner {
    killed: Arc<AtomicBool>,
    fail_first: AtomicUsize,
    calls: Mutex<usize>,
}

impl ServerSpawner for FakeSpawner {
    fn spawn(
        &self,
        _spec: &ServerSpec,
        _args: &[String],
    ) -> Result<Box<dyn ServerProcess>, AppError> {
        *self.calls.lock().unwrap() += 1;
        let remaining = self.fail_first.load(Ordering::Relaxed);
        if remaining > 0 {
            self.fail_first.store(remaining - 1, Ordering::Relaxed);
            return Err(AppError::Import("spawn failed (simulated)".to_string()));
        }
        Ok(Box::new(FakeProcess { killed: self.killed.clone() }))
    }
}

struct FakeProbe;

impl HealthProbe for FakeProbe {
    fn healthy(&self, _port: u16) -> bool {
        true
    }
}

fn fake_engine(killed: Arc<AtomicBool>) -> Arc<BangoAiEngine> {
    Arc::new(BangoAiEngine::with_seams(
        Arc::new(FakeSpawner { killed, ..Default::default() }),
        Arc::new(FakeProbe),
    ))
}

fn spec() -> ServerSpec {
    ServerSpec {
        binary: PathBuf::from("/nonexistent/llama-server"),
        model: PathBuf::from("/nonexistent/model.gguf"),
        log: PathBuf::from("/nonexistent/engine.log"),
    }
}

fn paths_in(dir: &Path) -> bango_lib::local_ai::paths::AiPaths {
    resolve_ai_paths_with_base(&dir.join("storage"), Some(&dir.join("data")))
}

#[test]
fn status_reports_derived_state_hardware_and_paths() {
    let conn = test_db();
    let manifest = local_manifest().expect("manifest");
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let engine = fake_engine(Arc::new(AtomicBool::new(false)));

    let status = build_status(&conn, &engine, &manifest, &paths, false).expect("status");
    assert_eq!(status.state, "not_installed");
    assert_eq!(status.backend, "configured_provider");
    assert_eq!(status.profile, profile_id());
    assert_eq!(status.model, "Ornith 1.5 9B");
    assert_eq!(status.license, "MIT");
    assert!(status.model_root.ends_with("model"));
    assert!(status.runtime_root.ends_with("runtimes"));
    assert!(status.download_bytes > 5_000_000_000, "model dominates the download");
    assert!(status.required_bytes > status.download_bytes);
    assert!(status.verdict.installable(), "verdict must be a value, not an error");
    assert!(!status.runtime_ready && !status.model_ready);

    // Installing state wins when the running flag is set.
    let running = build_status(&conn, &engine, &manifest, &paths, true).expect("status");
    assert_eq!(running.state, "installing");
    assert!(running.running);
}

#[test]
fn install_preflight_gates_target_and_disk() {
    let err = install_preflight(
        false,
        Some(100 * 1024 * 1024 * 1024),
        Some(100 * 1024 * 1024 * 1024),
        1024,
    )
    .expect_err("target");
    assert!(err.to_string().contains("not available for this system"), "got: {err}");

    let gib = 1024 * 1024 * 1024;
    // A short MODEL root is named in the error (aifixes1 F11).
    let err = install_preflight(true, Some(gib), Some(9 * gib), 2 * gib).expect_err("model disk");
    assert!(err.to_string().contains("free disk space"), "got: {err}");
    assert!(err.to_string().contains("model"), "names the short root: {err}");

    // A short RUNTIME root is named too.
    let err = install_preflight(true, Some(9 * gib), Some(gib), 2 * gib).expect_err("runtime disk");
    assert!(err.to_string().contains("runtime"), "names the short root: {err}");

    install_preflight(true, Some(3 * gib), Some(3 * gib), 2 * gib).expect("passes");
    install_preflight(true, None, None, 2 * gib).expect("unprobeable disk skips the gate");
}

#[test]
fn verify_and_remove_are_idempotent() {
    let manifest = local_manifest().expect("manifest");
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());

    let runtime_failures = verify_runtime(&paths.runtime_root, &manifest).expect("verify runtime");
    assert!(!runtime_failures.is_empty(), "missing runtime reports failures");
    let model_failures = verify_model(&paths.model_root, &manifest).expect("verify model");
    assert!(!model_failures.is_empty(), "missing model reports failures");

    let roots: Vec<&Path> = vec![paths.model_root.as_path()];
    remove_components(&roots, &paths.runtime_root, &manifest.runtime.version).expect("remove");
    remove_components(&roots, &paths.runtime_root, &manifest.runtime.version)
        .expect("remove is idempotent");
}

#[tokio::test]
async fn set_llm_backend_persists_and_stops_engine() {
    let conn = test_db();
    let killed = Arc::new(AtomicBool::new(false));
    let engine = fake_engine(killed.clone());
    engine.ensure_started(&spec()).await.expect("starts");
    assert_eq!(engine.state().expect("state"), bango_lib::llm::local::engine::EngineState::Ready);

    set_llm_backend(&conn, LlmBackend::BangoAi).expect("persists first");
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::BangoAi);

    engine.reset_off_thread().await.expect("resets");
    assert_eq!(engine.state().expect("state"), bango_lib::llm::local::engine::EngineState::Stopped);
    assert!(killed.load(Ordering::Relaxed), "engine stopped after the switch");
}

#[test]
fn engine_status_and_persisted_settings_agree_after_restart() {
    let conn = test_db();
    set_llm_backend(&conn, LlmBackend::BangoAi).expect("backend");
    set_bango_ai_settings(&conn, &EngineSettings { context: 32_768, threads: 4, reasoning: false })
        .expect("persist settings");

    // A fresh engine "after restart" seeds from the same persisted source the
    // startup path uses; the status panel then agrees with the stored keys.
    let engine = fake_engine(Arc::new(AtomicBool::new(false)));
    engine.update_settings(get_bango_ai_settings(&conn).expect("settings")).expect("seed");
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = local_manifest().expect("manifest");
    let paths =
        resolve_ai_paths_with_base(&dir.path().join("storage"), Some(&dir.path().join("data")));
    let status = build_status(&conn, &engine, &manifest, &paths, false).expect("status");
    assert_eq!(status.context, 32_768, "status reports the persisted context");
    assert_eq!(status.threads, 4, "status reports the persisted threads");
}

#[test]
fn install_progress_is_monotonic_across_phases() {
    let total = 100u64;
    // Runtime stage maps into [0, runtime_bytes].
    let runtime_start = stage_progress("installing", "engine", 0, 0, 10, total);
    let runtime_end = stage_progress("installing", "engine", 0, 10, 10, total);
    assert_eq!(runtime_start.overall_bytes, 0);
    assert_eq!(runtime_end.overall_bytes, 10);

    // Model stage starts at the runtime's completed bytes and never regresses.
    let base = next_stage_base(0, 10);
    let model_start = stage_progress("downloading", "model", base, 0, 90, total);
    let model_mid = stage_progress("downloading", "model", base, 45, 90, total);
    let model_end = stage_progress("verifying", "model", base, 90, 90, total);
    assert_eq!(model_start.overall_bytes, 10);
    assert_eq!(model_mid.overall_bytes, 55);
    assert_eq!(model_end.overall_bytes, 100);
    assert!(runtime_end.overall_bytes <= model_start.overall_bytes);
    assert!(model_start.overall_bytes <= model_mid.overall_bytes);
    assert!(model_mid.overall_bytes <= model_end.overall_bytes);
    for event in [&runtime_start, &runtime_end, &model_start, &model_mid, &model_end] {
        assert_eq!(event.overall_total, total, "one combined bar across both components");
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn tiny_bundle_zip(member: &str, body: &[u8]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut cursor);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    zip.start_file(member.to_string(), options).unwrap();
    zip.write_all(body).unwrap();
    zip.finish().unwrap();
    cursor.into_inner()
}

#[tokio::test]
async fn repair_install_restarts_engine_before_self_test() {
    // A repair install must stop a running engine before touching artifacts
    // (aifixes1 F15); the self-test restarts it on the promoted files.
    let killed = Arc::new(AtomicBool::new(false));
    let engine = fake_engine(killed.clone());
    engine.ensure_started(&spec()).await.expect("engine starts");

    // Manifest with no archive for the current target: the install fails at
    // the archive lookup - AFTER the stop, BEFORE any download/self-test.
    let manifest = BangoAiManifest {
        profile: profile_id().to_string(),
        model: "Test".to_string(),
        license: "MIT".to_string(),
        license_url: "https://example.invalid".to_string(),
        source_revision: "0123456789abcdef".to_string(),
        files: vec![],
        runtime: LlamaRuntimeManifest {
            name: "llama.cpp".to_string(),
            version: "b9999".to_string(),
            archives: vec![],
        },
    };
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let smoke = |_: &Path| -> Result<String, AppError> { Ok("llama-server".into()) };

    let err = install_bango_ai_inner(
        &paths,
        &engine,
        &manifest,
        &smoke,
        &|_event| {},
        &AtomicBool::new(false),
    )
    .await
    .expect_err("unsupported manifest");
    assert!(err.to_string().contains("not available"), "got: {err}");
    assert!(killed.load(Ordering::Relaxed), "engine stopped before install work");
}

#[tokio::test]
async fn cancel_during_install_returns_before_success() {
    // Cancellation is honored at phase boundaries: a cancelled install must
    // surface the friendly resume message, never success (aifixes1 F8).
    let mut server = mockito::Server::new_async().await;
    let bundle = tiny_bundle_zip("root/llama-server", b"fake-server-bytes");
    let archive_url = format!("{}/bundle.zip", server.url());
    let model_url = format!("{}/model.gguf", server.url());
    server
        .mock("GET", "/bundle.zip")
        .with_status(200)
        .with_body(bundle.clone())
        .create_async()
        .await;

    let target =
        bango_lib::local_ai::manifest::current_target().expect("supported test target").to_string();
    let manifest = BangoAiManifest {
        profile: profile_id().to_string(),
        model: "Test".to_string(),
        license: "MIT".to_string(),
        license_url: "https://example.invalid".to_string(),
        source_revision: "0123456789abcdef".to_string(),
        files: vec![PinnedFile {
            name: "model.gguf".to_string(),
            url: model_url.clone(),
            size: 4,
            sha256: Some(sha256_hex(b"data")),
        }],
        runtime: LlamaRuntimeManifest {
            name: "llama.cpp".to_string(),
            version: "b9999".to_string(),
            archives: vec![RuntimeArchive {
                target,
                name: "bundle.zip".to_string(),
                url: archive_url,
                size: bundle.len() as u64,
                sha256: sha256_hex(&bundle),
                members: vec![RuntimeMember {
                    path: "root/llama-server".to_string(),
                    size: 17,
                    executable: true,
                }],
                aliases: Vec::new(),
            }],
        },
    };

    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let engine = fake_engine(Arc::new(AtomicBool::new(false)));
    let smoke = |_: &Path| -> Result<String, AppError> { Ok("llama-server".to_string()) };

    let err = install_bango_ai_inner(
        &paths,
        &engine,
        &manifest,
        &smoke,
        &|_event| {},
        &AtomicBool::new(true),
    )
    .await
    .expect_err("a cancelled install must never succeed");
    assert!(err.to_string().to_lowercase().contains("cancel"), "got: {err}");
    assert!(
        !paths.model_root.join(bango_lib::llm::local::profile::LOCAL_LLM_PROFILE_DIR).exists(),
        "cancellation must land before the model is promoted"
    );
}

#[tokio::test]
async fn runtime_smoke_fails_before_model_download() {
    let mut server = mockito::Server::new_async().await;
    let bundle = tiny_bundle_zip("root/llama-server", b"fake-server-bytes");
    let archive_url = format!("{}/bundle.zip", server.url());
    let model_url = format!("{}/model.gguf", server.url());
    server
        .mock("GET", "/bundle.zip")
        .with_status(200)
        .with_body(bundle.clone())
        .create_async()
        .await;

    let target =
        bango_lib::local_ai::manifest::current_target().expect("supported test target").to_string();
    let manifest = BangoAiManifest {
        profile: profile_id().to_string(),
        model: "Test".to_string(),
        license: "MIT".to_string(),
        license_url: "https://example.invalid".to_string(),
        source_revision: "0123456789abcdef".to_string(),
        files: vec![PinnedFile {
            name: "model.gguf".to_string(),
            url: model_url.clone(),
            size: 4,
            sha256: Some(sha256_hex(b"data")),
        }],
        runtime: LlamaRuntimeManifest {
            name: "llama.cpp".to_string(),
            version: "b9999".to_string(),
            archives: vec![RuntimeArchive {
                target,
                name: "bundle.zip".to_string(),
                url: archive_url,
                size: bundle.len() as u64,
                sha256: sha256_hex(&bundle),
                members: vec![RuntimeMember {
                    path: "root/llama-server".to_string(),
                    size: 17,
                    executable: true,
                }],
                aliases: Vec::new(),
            }],
        },
    };

    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let engine = fake_engine(Arc::new(AtomicBool::new(false)));
    let smoke =
        |_: &Path| -> Result<String, AppError> { Err(AppError::Import("smoke failed".into())) };

    let err = install_bango_ai_inner(
        &paths,
        &engine,
        &manifest,
        &smoke,
        &|_event| {},
        &AtomicBool::new(false),
    )
    .await
    .expect_err("smoke failure aborts the transaction");
    assert!(err.to_string().contains("smoke failed"), "got: {err}");
    let profile_dir = paths.model_root.join(bango_lib::llm::local::profile::LOCAL_LLM_PROFILE_DIR);
    assert!(
        !profile_dir.exists(),
        "the model download must not start when the runtime smoke fails"
    );
}

#[test]
fn activation_persists_backend_only_after_self_test() {
    let conn = test_db();
    // Failure leaves the previous backend active even when activation was asked.
    persist_backend_after_install_conn(&conn, true, false).expect("no-op on failure");
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::ConfiguredProvider);

    // Success without activation also leaves the selection untouched.
    persist_backend_after_install_conn(&conn, false, true).expect("no-op without activate");
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::ConfiguredProvider);

    // Success + activation flips it.
    persist_backend_after_install_conn(&conn, true, true).expect("activates");
    assert_eq!(get_llm_backend(&conn).unwrap(), LlmBackend::BangoAi);
}

#[tokio::test]
async fn activation_switches_the_live_orchestrator_backend() {
    let orchestrator = LlmOrchestrator::new(3, 500);
    assert_eq!(orchestrator.backend(), LlmBackend::ConfiguredProvider);

    // Failure and no-activate leaves the in-memory backend untouched.
    activate_orchestrator_backend(&orchestrator, true, false).await;
    assert_eq!(orchestrator.backend(), LlmBackend::ConfiguredProvider);
    activate_orchestrator_backend(&orchestrator, false, true).await;
    assert_eq!(orchestrator.backend(), LlmBackend::ConfiguredProvider);

    // Success + activation must switch generation to the local engine without
    // requiring a radio toggle or an app restart.
    activate_orchestrator_backend(&orchestrator, true, true).await;
    assert_eq!(orchestrator.backend(), LlmBackend::BangoAi);
}
