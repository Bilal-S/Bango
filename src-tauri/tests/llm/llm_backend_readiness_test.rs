//! Bango AI readiness tests (T6): backend-aware gates for generation,
//! embeddings, wiki ingest, batch phases, and the translation worker.
//!
//! The runtime fixture installs the real pinned member layout once into a
//! temp data dir (via the `BANGO_TEST_DATA_LOCAL_DIR` debug knob); each test
//! installs its own model profile under its temp storage root.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use bango_lib::db::app_settings_repo::{
    set_embedding_backend, set_llm_backend, set_setting, STORAGE_ROOT_KEY,
};
use bango_lib::db::llm_config_repo::{restore_config_raw, RawLlmConfigRow};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::backend::EmbeddingBackend;
use bango_lib::embedding::local::download::runtime_lib_file;
use bango_lib::embedding::local::profile::LOCAL_PROFILE_DIR as EMBEDDING_PROFILE_DIR;
use bango_lib::llm::backend::LlmBackend;
use bango_lib::llm::local::engine::{
    BangoAiEngine, HealthProbe, ServerProcess, ServerSpawner, ServerSpec,
};
use bango_lib::llm::local::install::runtime_version_dir;
use bango_lib::llm::local::manifest::{local_manifest, BangoAiManifest};
use bango_lib::llm::local::policy::ALLOWED_CONTEXTS;
use bango_lib::llm::local::profile::LOCAL_LLM_PROFILE_DIR;
use bango_lib::llm::readiness::{embedding_generation_ready, has_usable_llm};
use bango_lib::local_ai::paths::resolve_ai_paths;
use bango_lib::translation::worker::resolve_translation_llm;
use rusqlite::Connection;

/// Production has a managed engine from startup; tests construct one once so
/// the reserved-port resolver works.
fn ensure_engine() {
    struct NoopSpawner;
    impl ServerSpawner for NoopSpawner {
        fn spawn(
            &self,
            _spec: &ServerSpec,
            _args: &[String],
        ) -> Result<Box<dyn ServerProcess>, bango_lib::error::AppError> {
            Err(bango_lib::error::AppError::Import("noop".to_string()))
        }
    }
    struct NoopProbe;
    impl HealthProbe for NoopProbe {
        fn healthy(&self, _port: u16) -> bool {
            false
        }
    }
    static ENGINE: OnceLock<BangoAiEngine> = OnceLock::new();
    ENGINE.get_or_init(|| BangoAiEngine::with_seams(Arc::new(NoopSpawner), Arc::new(NoopProbe)));
}

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Shared temp data dir with the runtime bundle installed exactly once.
fn data_dir() -> &'static Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("BANGO_TEST_DATA_LOCAL_DIR", dir.path());
        install_runtime(dir.path(), &local_manifest().unwrap());
        dir
    });
    dir.path()
}

/// Sparse file at the exact pinned size (keeps the multi-GB fixture instant).
fn write_sized(path: &Path, size: u64) {
    let file = std::fs::File::create(path).unwrap();
    file.set_len(size).unwrap();
}

fn install_runtime(data: &Path, manifest: &BangoAiManifest) {
    let archive = manifest.archive_for_current_target().expect("archive");
    let runtime_root = data.join("Bango").join("ai").join("runtimes");
    let version_dir = runtime_version_dir(&runtime_root, &manifest.runtime.version);
    std::fs::create_dir_all(&version_dir).unwrap();
    for member in &archive.members {
        let base = Path::new(&member.path).file_name().unwrap();
        write_sized(&version_dir.join(base), member.size);
    }
    for alias in &archive.aliases {
        let size = archive
            .members
            .iter()
            .find(|m| {
                Path::new(&m.path).file_name().and_then(|n| n.to_str())
                    == Some(alias.target.as_str())
            })
            .map_or(0, |m| m.size);
        write_sized(&version_dir.join(&alias.path), size);
    }
    std::fs::write(
        version_dir.join("manifest.json"),
        serde_json::json!({
            "name": manifest.runtime.name,
            "version": manifest.runtime.version,
            "target": archive.target,
        })
        .to_string(),
    )
    .unwrap();
}

fn write_model(model_root: &Path, manifest: &BangoAiManifest) {
    let profile_dir = model_root.join(LOCAL_LLM_PROFILE_DIR);
    std::fs::create_dir_all(&profile_dir).unwrap();
    for file in &manifest.files {
        write_sized(&profile_dir.join(&file.name), file.size);
    }
    std::fs::write(profile_dir.join("manifest.json"), serde_json::to_string(manifest).unwrap())
        .unwrap();
}

/// Point the conn at a fresh storage root with the model profile installed.
fn ready_storage(conn: &Connection) -> tempfile::TempDir {
    let _ = data_dir();
    let storage = tempfile::tempdir().unwrap();
    set_setting(conn, STORAGE_ROOT_KEY, Some(storage.path().to_str().unwrap())).unwrap();
    write_model(&storage.path().join("model"), &local_manifest().unwrap());
    storage
}

/// Sparse-install the embedding components (model profile + runtime library)
/// so the `bango_local` embedding gate reports ready.
fn install_embedding_components(storage: &Path) {
    let manifest = bango_lib::embedding::local::manifest::local_manifest().unwrap();
    let profile_dir = storage.join("model").join(EMBEDDING_PROFILE_DIR);
    std::fs::create_dir_all(&profile_dir).unwrap();
    for file in &manifest.files {
        write_sized(&profile_dir.join(&file.name), file.size);
    }
    std::fs::write(profile_dir.join("manifest.json"), serde_json::to_string(&manifest).unwrap())
        .unwrap();
    if let Some(file) = manifest.runtime_file_for_current_target() {
        let paths = resolve_ai_paths(storage);
        let lib = runtime_lib_file(&paths.runtime_root, &manifest).unwrap();
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        write_sized(&lib, file.lib_size);
    }
}

fn local_llama_cpp_row() -> RawLlmConfigRow {
    RawLlmConfigRow {
        provider: "llama_cpp".to_string(),
        endpoint_url: "http://127.0.0.1:8080/v1".to_string(),
        api_key_encrypted: None,
        model_name: "some-local-model".to_string(),
        temperature: 0.2,
        skip_temperature: 0,
        max_concurrent_requests: 1,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

#[test]
fn has_usable_llm_is_backend_aware() {
    let conn = test_db();
    assert!(!has_usable_llm(&conn).unwrap(), "configured_provider with no row");

    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    assert!(!has_usable_llm(&conn).unwrap(), "bango_ai without components");

    let _storage = ready_storage(&conn);
    assert!(has_usable_llm(&conn).unwrap(), "bango_ai with installed components");

    set_llm_backend(&conn, LlmBackend::ConfiguredProvider).unwrap();
    assert!(!has_usable_llm(&conn).unwrap(), "switch-back re-evaluates the cloud row");
}

#[test]
fn embedding_director_gate_accepts_bango_ai() {
    ensure_engine();
    let conn = test_db();
    let storage = ready_storage(&conn);
    install_embedding_components(storage.path());
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    set_embedding_backend(&conn, EmbeddingBackend::BangoLocal).unwrap();
    assert!(
        embedding_generation_ready(&conn).unwrap(),
        "local-only setups generate embeddings without a cloud row"
    );
}

#[test]
fn embedding_director_cloud_branch_still_requires_cloud_config() {
    let conn = test_db();
    let _storage = ready_storage(&conn);
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    set_embedding_backend(&conn, EmbeddingBackend::ConfiguredProvider).unwrap();
    assert!(
        !embedding_generation_ready(&conn).unwrap(),
        "chat-backend usability must not green-light the cloud embedding branch"
    );

    restore_config_raw(&conn, &local_llama_cpp_row()).unwrap();
    assert!(embedding_generation_ready(&conn).unwrap(), "cloud row present");
}

#[test]
fn wiki_ingest_and_batch_phases_accept_bango_ai() {
    let conn = test_db();
    let _storage = ready_storage(&conn);
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    // Wiki-ingest gates and batch summary/translation phases all call
    // `has_usable_llm` now.
    assert!(has_usable_llm(&conn).unwrap());
}

#[test]
fn translation_worker_uses_effective_config_and_context() {
    ensure_engine();
    let conn = test_db();
    assert!(resolve_translation_llm(&conn).unwrap().is_none(), "nothing configured");

    restore_config_raw(&conn, &local_llama_cpp_row()).unwrap();
    let (config, context) = resolve_translation_llm(&conn).unwrap().expect("cloud row");
    assert_eq!(context, 50_000, "batch sizing reads the stored cloud context");
    assert_eq!(config.context_window_tokens, context);

    let _storage = ready_storage(&conn);
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    let (config, context) = resolve_translation_llm(&conn).unwrap().expect("local config");
    assert_eq!(config.provider, bango_lib::models::llm_config::LlmProvider::BangoAi);
    assert!(ALLOWED_CONTEXTS.contains(&context), "local context in {ALLOWED_CONTEXTS:?}");
}
