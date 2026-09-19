//! Bango AI live tests (T4 runtime spike + T8 acceptance).
//!
//! All tests here are `#[ignore = "slow"]` and registered in
//! `tests/slow-manifest.toml`. The T4 test hits the network (one ~18 MB
//! GitHub release download) and writes only to a temp dir.

use std::process::Command;
use std::sync::atomic::AtomicBool;

use bango_lib::llm::local::engine::{
    model_path, BangoAiEngine, EngineConfig, HealthProbe, ServerSpec, SystemSpawner, TcpHealthProbe,
};
use bango_lib::llm::local::install::{install_model_profile, install_runtime_bundle, server_path};
use bango_lib::llm::local::manifest::local_manifest;

/// T4 live: the pinned runtime archive downloads, hash-verifies, extracts the
/// pinned member set, and `llama-server --version` runs.
#[test]
#[ignore = "slow"]
fn bango_ai_runtime_install_and_version() {
    let manifest = local_manifest().expect("manifest");
    let dir = tempfile::tempdir().expect("tempdir");
    let cancel = AtomicBool::new(false);
    let runtime_root = dir.path().to_path_buf();
    let report = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(install_runtime_bundle(&runtime_root, &manifest, &|_| {}, &cancel))
        .expect("runtime bundle installs");

    assert!(
        report.downloaded + report.skipped >= 1,
        "runtime bundle must install (downloaded {}, skipped {})",
        report.downloaded,
        report.skipped
    );
    let server = server_path(&runtime_root, &manifest.runtime.version);
    let output = Command::new(&server).arg("--version").output().expect("--version runs");
    assert!(output.status.success(), "--version must succeed");
    // llama-server prints --version to stderr in this build; accept either.
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("llama-server") || text.to_lowercase().contains("version"),
        "unexpected --version output: {text}"
    );
}

/// Shared: install both components into `dir` and return (server, model).
fn install_all(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let manifest = local_manifest().expect("manifest");
    let cancel = AtomicBool::new(false);
    let runtime_root = dir.join("runtimes");
    let model_root = dir.join("model");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(async {
            install_runtime_bundle(&runtime_root, &manifest, &|_| {}, &cancel)
                .await
                .expect("runtime install");
            install_model_profile(&model_root, &manifest, &|_| {}, &cancel)
                .await
                .expect("model install");
        });
    (server_path(&runtime_root, &manifest.runtime.version), model_path(&model_root))
}

/// One JSON-mode completion with thinking forced off (screening-style).
async fn json_probe(config: &EngineConfig, system: &str, user: &str) -> (String, u64) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .expect("client");
    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "response_format": {"type": "json_object"},
        "chat_template_kwargs": {"enable_thinking": false}
    });
    let response = client
        .post(format!("{}/chat/completions", config.endpoint))
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await
        .expect("completion sends");
    assert!(response.status().is_success(), "status {}", response.status());
    let json: serde_json::Value = response.json().await.expect("json body");
    let content = json["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string();
    let tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    (content, tokens)
}

/// T8 live: full pinned install, real engine start, one screening-style JSON
/// prompt and one JSON summary prompt; asserts parseable JSON and no thinking
/// leakage, prints timing observations.
#[test]
#[ignore = "slow"]
fn bango_ai_ornith_acceptance_smoke() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, model) = install_all(dir.path());
    let engine = std::sync::Arc::new(BangoAiEngine::with_seams(
        std::sync::Arc::new(SystemSpawner),
        std::sync::Arc::new(TcpHealthProbe),
    ));
    let spec = ServerSpec { binary: server, model, log: dir.path().join("llama-server.log") };
    let started = std::time::Instant::now();
    let config = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(engine.ensure_started(&spec))
        .expect("engine starts");
    let load_ms = started.elapsed().as_millis();

    let rt =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    let prompt_start = std::time::Instant::now();
    let (screen_json, screen_tokens) = rt.block_on(json_probe(
        &config,
        "You are a systematic-review screening assistant. Reply with JSON only.",
        "Article: a study of coffee and insomnia. Criterion: mentions sleep quality. \
         Reply with {\"include\": true or false, \"reason\": \"...\"}",
    ));
    let screen_ms = prompt_start.elapsed().as_millis();
    let parsed: serde_json::Value =
        serde_json::from_str(screen_json.trim()).expect("screening JSON parses");
    assert!(parsed.get("include").is_some(), "screening JSON shape: {screen_json}");
    assert!(!screen_json.contains("<think>"), "thinking must stay off");

    let summary_start = std::time::Instant::now();
    let (summary, summary_tokens) = rt.block_on(json_probe(
        &config,
        "Summarize research abstracts. Reply with JSON containing one \"summary\" key.",
        "Summarize: a study of 120 adults found caffeine reduced total sleep by 40 minutes.",
    ));
    let summary_ms = summary_start.elapsed().as_millis();
    let parsed_summary: serde_json::Value =
        serde_json::from_str(summary.trim()).expect("summary JSON parses");
    assert!(parsed_summary.get("summary").is_some(), "summary JSON shape: {summary}");

    println!(
        "bango-ai acceptance: load {load_ms} ms; screening {screen_ms} ms / {screen_tokens} tok; \
         summary {summary_ms} ms / {summary_tokens} tok; ctx {}",
        config.context
    );
    let _ = rt.block_on(engine.stop());
}

/// T8 live: with the debug idle override the server sleeps in place after the
/// idle window (process + port stay alive, `/health` still 200) and a wake
/// request succeeds; prints available-RAM observations across the sleep.
#[test]
#[ignore = "slow"]
fn bango_ai_engine_idle_stop_with_override() {
    // Process-global (edition 2021): the sibling live tests tolerate a short
    // idle window; production never sets this variable.
    std::env::set_var("BANGO_AI_IDLE_SLEEP_SECS", "5");
    let dir = tempfile::tempdir().expect("tempdir");
    let (server, model) = install_all(dir.path());
    let engine = std::sync::Arc::new(BangoAiEngine::with_seams(
        std::sync::Arc::new(SystemSpawner),
        std::sync::Arc::new(TcpHealthProbe),
    ));
    let spec = ServerSpec { binary: server, model, log: dir.path().join("llama-server.log") };
    let config = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(engine.ensure_started(&spec))
        .expect("engine starts");
    let port = engine.port().expect("port");

    fn available_mb() -> u64 {
        use sysinfo::System;
        let mut system = System::new();
        system.refresh_memory();
        system.available_memory() / (1024 * 1024)
    }
    let before_sleep = available_mb();
    std::thread::sleep(std::time::Duration::from_secs(12));
    let after_sleep = available_mb();

    // Native sleep keeps the process + endpoint alive (T4 finding).
    assert!(TcpHealthProbe.healthy(port), "/health stays 200 while sleeping");

    let rt =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    let wake_start = std::time::Instant::now();
    let (content, _) =
        rt.block_on(json_probe(&config, "Reply with JSON only.", "Reply with {\"ready\": true}"));
    let wake_ms = wake_start.elapsed().as_millis();
    assert!(content.contains("ready"), "wake response: {content}");
    println!(
        "bango-ai idle: available RAM {before_sleep} -> {after_sleep} MB across sleep; \
         wake {wake_ms} ms"
    );
    let _ = rt.block_on(engine.stop());
}
