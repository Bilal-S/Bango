//! Bango AI live tests (T4 runtime spike + T8 acceptance).
//!
//! All tests here are `#[ignore = "slow"]` and registered in
//! `tests/slow-manifest.toml`. The T4 test hits the network (one ~18 MB
//! GitHub release download) and writes only to a temp dir.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use bango_lib::llm::local::engine::{
    model_path, BangoAiEngine, EngineConfig, HealthProbe, ServerSpec, SystemSpawner, TcpHealthProbe,
};
use bango_lib::llm::local::install::{install_model_profile, install_runtime_bundle, server_path};
use bango_lib::llm::local::manifest::local_manifest;
use bango_lib::local_ai::paths::resolve_ai_paths;
use bango_lib::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};

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
fn bango_ai_acceptance_smoke() {
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

// ── Local structured-response guards (screening + figure descriptions) ──────
//
// llama.cpp's `response_format: json_object` grammar CANNOT emit a bare
// top-level JSON array, so every local structured consumer must either ask
// for an object wrapper or recover one. These tests call the pinned model
// with the REAL prompt builders and run the REAL parsers, so a regression in
// either half fails here instead of in the app.
//
// They use the components already installed by the app when present and SKIP
// (no download) otherwise; the temp-install acceptance smoke above remains
// the clean-machine test.

/// Production storage root (`BANGO_AI_TEST_STORAGE_ROOT` overrides for
/// tests), mirroring `app_settings_repo`'s default `~/Documents/Bango`.
fn installed_storage_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("BANGO_AI_TEST_STORAGE_ROOT") {
        return Some(PathBuf::from(dir));
    }
    dirs::document_dir().map(|docs| docs.join("Bango"))
}

/// Runtime + model + engine log from the user's real install; `None` when
/// either component is missing (callers print a skip notice).
fn installed_components() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let manifest = local_manifest().ok()?;
    let paths = resolve_ai_paths(&installed_storage_root()?);
    let server = server_path(&paths.runtime_root, &manifest.runtime.version);
    let model = model_path(&paths.model_root);
    if !server.is_file() || !model.is_file() {
        return None;
    }
    let log = paths.runtime_root.join("logs").join("llama-server.log");
    Some((server, model, log))
}

/// Started engine against the installed components.
struct LiveAi {
    engine: Arc<BangoAiEngine>,
    config: EngineConfig,
    rt: tokio::runtime::Runtime,
}

fn start_live_ai(test: &str) -> Option<LiveAi> {
    let Some((server, model, log)) = installed_components() else {
        eprintln!("[bango-ai-live] {test}: components not installed; skipping");
        return None;
    };
    let engine =
        Arc::new(BangoAiEngine::with_seams(Arc::new(SystemSpawner), Arc::new(TcpHealthProbe)));
    let spec = ServerSpec { binary: server, model, log };
    let rt =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    let config = rt.block_on(engine.ensure_started(&spec)).expect("engine starts");
    Some(LiveAi { engine, config, rt })
}

impl LiveAi {
    fn json(&self, system: &str, user: &str) -> String {
        self.rt.block_on(json_probe(&self.config, system, user)).0
    }

    fn stop(&self) {
        let _ = self.rt.block_on(self.engine.stop());
    }
}

/// The reported bug: batch size 1 + local `json_object` grammar makes the 9B
/// model answer with a flat `{decision, reasoning}` object; the real screening
/// parser must still produce one result.
#[test]
#[ignore = "slow"]
fn bango_ai_screening_prompt_parses_under_json_object_grammar() {
    use bango_lib::screening::json_parse::process_screening_responses;
    use bango_lib::screening::prompt::{
        build_screening_prompt, AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput,
        SYSTEM_PROMPT,
    };

    let Some(ai) = start_live_ai("screening") else { return };
    let input = ScreeningPromptInput {
        aims: vec![AimEntry { text: "Evaluate UK sugar-reduction policies.".to_string() }],
        inclusion_criteria: vec![CriterionEntry {
            id: "inc-uk".to_string(),
            text: "Geography United Kingdom".to_string(),
            priority: Priority::High,
            global_number: 1,
        }],
        exclusion_criteria: vec![CriterionEntry {
            id: "exc-not-uk".to_string(),
            text: "Not United Kingdom".to_string(),
            priority: Priority::High,
            global_number: 2,
        }],
        articles: vec![ArticleEntry::new(
            "SUGAR CONSUMPTION IN WEST-GERMANY".to_string(),
            "Anonymous".to_string(),
            Some(1985),
            "Sugar consumption in West Germany is described; no UK data is reported.".to_string(),
        )],
        existing_tags: vec![],
        existing_labels: vec![],
        custom_logic: None,
    };
    let user = build_screening_prompt(&input);
    let raw = ai.json(SYSTEM_PROMPT, &user);
    let parsed = process_screening_responses(&raw);
    if let Err(e) = &parsed {
        panic!(
            "screening parser must accept the local json_object response: {e}; raw: {}",
            &raw[..raw.len().min(400)]
        );
    }
    assert_eq!(parsed.expect("parsed").len(), 1, "one article must map to one result");
    ai.stop();
}

/// The same grammar cannot emit the bare array the figure-description prompt
/// historically requested; the prompt + parser must agree on an object wrapper.
#[test]
#[ignore = "slow"]
fn bango_ai_figure_description_prompt_parses_under_json_object_grammar() {
    use bango_lib::summary::prompt::{
        build_figure_description_prompt, parse_figure_descriptions_response,
        FIGURE_DESCRIPTION_SYSTEM_PROMPT,
    };
    use bango_lib::utils::sections::{Caption, CaptionKind};

    let Some(ai) = start_live_ai("figure descriptions") else { return };
    let captions = vec![Caption {
        kind: CaptionKind::Figure,
        number: "1".to_string(),
        caption: "Trends in sugar-sweetened beverage purchases in the UK, 2015-2019.".to_string(),
        following_sentence: None,
    }];
    let user = build_figure_description_prompt("UK sugar tax evaluation", &captions);
    let raw = ai.json(FIGURE_DESCRIPTION_SYSTEM_PROMPT, &user);
    let parsed = parse_figure_descriptions_response(&raw);
    if let Err(e) = &parsed {
        panic!(
            "figure descriptions parser must accept the local json_object response: {e}; raw: {}",
            &raw[..raw.len().min(400)]
        );
    }
    assert_eq!(parsed.expect("parsed").len(), 1);
    ai.stop();
}

/// Live shape audit for the remaining `send_json` generation consumers: their
/// real prompts must remain parseable under the local `json_object` grammar.
///
/// Criteria generation is intentionally absent: on the pinned 9B it looped
/// past 5,400 generated tokens without an EOS inside the 600 s probe budget
/// (the documented no-output-cap risk, bounded by the 1800 s local timeout in
/// production), so it cannot serve as a shape guard yet.
#[test]
#[ignore = "slow"]
fn bango_ai_structured_consumers_parse_under_json_object_grammar() {
    let Some(ai) = start_live_ai("structured consumers") else { return };
    let aims = vec![ResearchAim {
        id: "aim-1".to_string(),
        text: "Evaluate the impact of the UK Soft Drinks Industry Levy on sugar consumption."
            .to_string(),
        created_at: String::new(),
    }];
    let inclusion = vec![Criterion {
        id: "inc-1".to_string(),
        criterion_type: CriterionType::Inclusion,
        text: "UK geography".to_string(),
        priority: Priority::High,
        created_at: String::new(),
    }];
    let exclusion = vec![Criterion {
        id: "exc-1".to_string(),
        criterion_type: CriterionType::Exclusion,
        text: "Not United Kingdom".to_string(),
        priority: Priority::High,
        created_at: String::new(),
    }];

    // Search strategy (typed `SearchStrategyResult` parse).
    let (system, user) = bango_lib::commands::search_strategy::build_search_strategy_prompt(
        &aims, &inclusion, &exclusion,
    );
    let raw = ai.json(&system, &user);
    bango_lib::commands::search_strategy::parse_search_strategy_response(&raw).unwrap_or_else(
        |e| panic!("search strategy parse failed: {e}; raw: {}", &raw[..raw.len().min(400)]),
    );

    // OpenAlex smart search (typed `SmartSearchQuery` parse).
    let (system, user) =
        bango_lib::openalex::smart_search::build_smart_search_prompt(&aims, &inclusion, &exclusion);
    let raw = ai.json(&system, &user);
    bango_lib::openalex::smart_search::parse_smart_search_response(&raw).unwrap_or_else(|e| {
        panic!("openalex smart search parse failed: {e}; raw: {}", &raw[..raw.len().min(400)])
    });

    ai.stop();
}
