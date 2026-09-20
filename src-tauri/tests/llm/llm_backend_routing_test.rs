//! Bango AI routing tests (T6): effective config, timeouts, response-format,
//! reasoning, and context-window behavior across the two backends.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use bango_lib::db::app_settings_repo::{
    get_bango_ai_settings, set_bango_ai_settings, set_llm_backend,
};
use bango_lib::db::llm_config_repo::{
    get_config, restore_config_raw, save_config, RawLlmConfigRow,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::llm::backend::LlmBackend;
use bango_lib::llm::effective_config::{effective_context_window, resolve};
use bango_lib::llm::local::engine::{
    BangoAiEngine, HealthProbe, ServerProcess, ServerSpawner, ServerSpec,
};
use bango_lib::llm::local::policy::{EngineSettings, ALLOWED_CONTEXTS};
use bango_lib::llm::orchestrator::{
    local_request_options, resolve_timeout, LlmOrchestrator, LlmRequestType, LocalConfigProvider,
    TemperatureFlagPersister, LOCAL_TIMEOUT_SECS,
};
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};
use rusqlite::Connection;

// ── Fixtures ────────────────────────────────────────────────────────────────

/// Production has a managed engine from startup; tests construct one once so
/// the reserved-port resolver works.
fn ensure_engine() {
    struct NoopSpawner;
    impl ServerSpawner for NoopSpawner {
        fn spawn(
            &self,
            _spec: &ServerSpec,
            _args: &[String],
        ) -> Result<Box<dyn ServerProcess>, AppError> {
            Err(AppError::Import("noop".to_string()))
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

struct StubLocalProvider {
    config: LlmConfig,
    reasoning: bool,
    calls: Arc<AtomicUsize>,
}

impl LocalConfigProvider for StubLocalProvider {
    fn effective_local_config<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<LlmConfig>, AppError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let config = self.config.clone();
        Box::pin(async move { Ok(Some(config)) })
    }

    fn reasoning_enabled(&self) -> bool {
        self.reasoning
    }
}

/// Simulates an unavailable local engine (missing components/unsupported).
struct OfflineProvider;

impl LocalConfigProvider for OfflineProvider {
    fn effective_local_config<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<LlmConfig>, AppError>> + Send + 'a>> {
        Box::pin(async move { Ok(None) })
    }
}

#[derive(Default)]
struct RecordingPersister {
    calls: Arc<AtomicUsize>,
}

impl TemperatureFlagPersister for RecordingPersister {
    fn persist(&self, _skip: bool) {
        self.calls.fetch_add(1, Ordering::Relaxed);
    }
}

fn chat_body(content: &str, tokens: u64) -> String {
    format!(
        r#"{{"choices":[{{"message":{{"content":"{content}"}},"finish_reason":"stop"}}],"usage":{{"total_tokens":{tokens}}}}}"#
    )
}

fn local_config(endpoint: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::BangoAi,
        endpoint_url: endpoint.to_string(),
        api_key_encrypted: Some("local-key".to_string()),
        model_name: "builtin/qwen3.5-2b-ud-q4kxl@r1".to_string(),
        temperature: 0.6,
        skip_temperature: false,
        max_concurrent_requests: 1,
        request_delay_ms: 0,
        context_window_tokens: 16_384,
    }
}

fn cloud_config(endpoint: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Custom,
        endpoint_url: endpoint.to_string(),
        api_key_encrypted: Some("cloud-key".to_string()),
        model_name: "cloud-model".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 5,
        request_delay_ms: 100,
        context_window_tokens: 50_000,
    }
}

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
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

#[tokio::test]
async fn orchestrator_uses_bango_ai_config_when_selected() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_body(chat_body("local-ok", 5))
        .create_async()
        .await;

    let orchestrator = LlmOrchestrator::new(3, 0);
    orchestrator.set_backend_initial(LlmBackend::BangoAi);
    let calls = Arc::new(AtomicUsize::new(0));
    orchestrator.set_local_config_provider(Arc::new(StubLocalProvider {
        config: local_config(&format!("{}/v1", server.url())),
        reasoning: false,
        calls: calls.clone(),
    }));

    // The passed cloud config is ignored on the local path.
    let cloud = cloud_config("http://127.0.0.1:9");
    let (content, tokens) = orchestrator
        .send_opts(&cloud, "system", "user", LlmRequestType::Chat, false)
        .await
        .expect("local call succeeds");
    assert_eq!(content, "local-ok");
    assert_eq!(tokens, 5);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    mock.assert_async().await;
}

#[tokio::test]
async fn orchestrator_uses_stored_config_by_default() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(chat_body("cloud-ok", 7))
        .create_async()
        .await;

    let orchestrator = LlmOrchestrator::new(3, 0);
    let (content, tokens) = orchestrator
        .send_opts(&cloud_config(&server.url()), "system", "user", LlmRequestType::Chat, false)
        .await
        .expect("cloud call succeeds");
    assert_eq!(content, "cloud-ok");
    assert_eq!(tokens, 7);
    mock.assert_async().await;
}

#[tokio::test]
async fn local_concurrency_and_timeout_overrides_apply() {
    let orchestrator = LlmOrchestrator::new(3, 0);
    orchestrator.set_backend(LlmBackend::BangoAi, None).await;
    assert_eq!(orchestrator.available_permits(), 1, "local calls serialize");
    assert_eq!(
        resolve_timeout(&LlmRequestType::Screening, true),
        std::time::Duration::from_secs(LOCAL_TIMEOUT_SECS)
    );

    let stored = cloud_config("http://127.0.0.1:9");
    orchestrator.set_backend(LlmBackend::ConfiguredProvider, Some(&stored)).await;
    assert_eq!(orchestrator.available_permits(), 5, "switch-back restores cloud limits");
    assert_eq!(
        resolve_timeout(&LlmRequestType::Screening, false),
        std::time::Duration::from_secs(120)
    );
}

#[test]
fn local_timeout_override_covers_every_request_type() {
    let all = [
        LlmRequestType::Screening,
        LlmRequestType::AiSummary,
        LlmRequestType::ArticleSummary,
        LlmRequestType::TagGeneration,
        LlmRequestType::LabelGeneration,
        LlmRequestType::CriteriaGeneration,
        LlmRequestType::SummaryGeneration,
        LlmRequestType::TestConnection,
        LlmRequestType::Chat,
        LlmRequestType::WikiIngest,
        LlmRequestType::WikiChat,
        LlmRequestType::SectionSummary,
        LlmRequestType::FigureDescription,
        LlmRequestType::EnhancedScreening,
        LlmRequestType::UnifiedSummary,
        LlmRequestType::Translation,
        LlmRequestType::GapAnalysis,
        LlmRequestType::SearchStrategy,
        LlmRequestType::OpenAlexSmartSearch,
        LlmRequestType::Embedding,
        LlmRequestType::CitationFinder,
        LlmRequestType::CitationFinderSplit,
        LlmRequestType::ClusterThematicAnalysis,
    ];
    for request_type in &all {
        assert_eq!(
            resolve_timeout(request_type, true),
            std::time::Duration::from_secs(LOCAL_TIMEOUT_SECS),
            "local override must cover {request_type:?}"
        );
    }
}

#[tokio::test]
async fn unsupported_target_blocks_generation_without_cloud_fallback() {
    let mut server = mockito::Server::new_async().await;
    let cloud_mock = server
        .mock("POST", "/chat/completions")
        .expect(0)
        .with_status(200)
        .with_body(chat_body("must-not-be-called", 1))
        .create_async()
        .await;

    let conn = test_db();
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    let orchestrator = LlmOrchestrator::new(3, 0);
    orchestrator.set_backend_initial(LlmBackend::BangoAi);
    orchestrator.set_local_config_provider(Arc::new(OfflineProvider));

    let err = orchestrator
        .send_opts(&cloud_config(&server.url()), "system", "user", LlmRequestType::Chat, false)
        .await
        .expect_err("local backend without an engine must fail");
    assert!(err.to_string().contains("Bango AI"), "got: {err}");
    cloud_mock.assert_async().await;
}

#[test]
fn effective_context_window_prevents_oversized_local_prompts() {
    ensure_engine();
    let conn = test_db();
    restore_config_raw(&conn, &local_llama_cpp_row()).unwrap();

    assert_eq!(effective_context_window(&conn).unwrap(), 50_000);
    set_llm_backend(&conn, LlmBackend::BangoAi).unwrap();
    let local_window = effective_context_window(&conn).unwrap();
    assert_ne!(local_window, 50_000, "stored cloud context must not leak into local");
    assert!(ALLOWED_CONTEXTS.contains(&(local_window as i32)), "got {local_window}");
}

#[tokio::test]
async fn local_screening_json_intent_reaches_the_client() {
    let mut server = mockito::Server::new_async().await;
    let matcher = mockito::Matcher::AllOf(vec![
        mockito::Matcher::Regex("\"response_format\"".to_string()),
        mockito::Matcher::Regex("json_object".to_string()),
        mockito::Matcher::Regex("\"enable_thinking\":false".to_string()),
    ]);
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_body(matcher)
        .with_status(200)
        .with_body(chat_body("[]", 3))
        .create_async()
        .await;

    let orchestrator = LlmOrchestrator::new(1, 0);
    orchestrator.set_backend_initial(LlmBackend::BangoAi);
    orchestrator.set_local_config_provider(Arc::new(StubLocalProvider {
        config: local_config(&format!("{}/v1", server.url())),
        reasoning: false,
        calls: Arc::new(AtomicUsize::new(0)),
    }));

    orchestrator
        .send_opts(
            &local_config(&format!("{}/v1", server.url())),
            "system",
            "user",
            LlmRequestType::Screening,
            true,
        )
        .await
        .expect("screening call succeeds");
    mock.assert_async().await;
}

#[test]
fn wiki_output_budget_uses_effective_config() {
    let local = local_config("http://127.0.0.1:9/v1");
    assert_eq!(
        bango_lib::llm::client::estimated_output_budget_tokens(&local),
        8_192,
        "Bango AI plans a local output budget, not the cloud default"
    );
}

#[tokio::test]
async fn local_temperature_rejection_does_not_persist_skip_temperature() {
    let mut server = mockito::Server::new_async().await;
    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.6 with this model. Only the default (1) value is supported.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;
    server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_body(chat_body("recovered", 4))
        .expect(1)
        .create_async()
        .await;

    let orchestrator = LlmOrchestrator::new(1, 0);
    let persister_calls = Arc::new(AtomicUsize::new(0));
    orchestrator
        .set_temperature_persister(Arc::new(RecordingPersister { calls: persister_calls.clone() }));
    orchestrator.set_backend_initial(LlmBackend::BangoAi);
    orchestrator.set_local_config_provider(Arc::new(StubLocalProvider {
        config: local_config(&format!("{}/v1", server.url())),
        reasoning: false,
        calls: Arc::new(AtomicUsize::new(0)),
    }));

    let (content, _tokens) = orchestrator
        .send_opts(
            &local_config(&format!("{}/v1", server.url())),
            "system",
            "user",
            LlmRequestType::Chat,
            false,
        )
        .await
        .expect("recovers without persisting");
    assert_eq!(content, "recovered");
    assert_eq!(
        persister_calls.load(Ordering::Relaxed),
        0,
        "local rejections must never write the cloud skip_temperature flag"
    );
}

#[tokio::test]
async fn local_send_json_sends_response_format_json_object() {
    let mut server = mockito::Server::new_async().await;
    let matcher = mockito::Matcher::AllOf(vec![
        mockito::Matcher::Regex("\"response_format\"".to_string()),
        mockito::Matcher::Regex("json_object".to_string()),
    ]);
    let json_ok_body = serde_json::json!({
        "choices": [{ "message": { "content": "{\"ok\":true}" }, "finish_reason": "stop" }],
        "usage": { "total_tokens": 4 }
    })
    .to_string();
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .match_body(matcher)
        .with_status(200)
        .with_body(json_ok_body)
        .create_async()
        .await;

    let orchestrator = LlmOrchestrator::new(1, 0);
    orchestrator.set_backend_initial(LlmBackend::BangoAi);
    orchestrator.set_local_config_provider(Arc::new(StubLocalProvider {
        config: local_config(&format!("{}/v1", server.url())),
        reasoning: false,
        calls: Arc::new(AtomicUsize::new(0)),
    }));

    let (content, _tokens) = orchestrator
        .send_json(
            &local_config(&format!("{}/v1", server.url())),
            "system",
            "user",
            LlmRequestType::ArticleSummary,
        )
        .await
        .expect("json call succeeds");
    assert_eq!(content, "{\"ok\":true}");
    mock.assert_async().await;
}

#[test]
fn structured_types_force_thinking_off_prose_honors_toggle() {
    // Structured (json intent) always disables thinking.
    let structured = local_request_options(true, false);
    assert!(structured.json_mode);
    assert_eq!(structured.enable_thinking, Some(false));
    let structured_reasoning = local_request_options(true, true);
    assert_eq!(structured_reasoning.enable_thinking, Some(false));

    // Prose honors the toggle: off -> force false; on -> server default.
    let prose = local_request_options(false, false);
    assert!(!prose.json_mode);
    assert_eq!(prose.enable_thinking, Some(false));
    let prose_reasoning = local_request_options(false, true);
    assert_eq!(prose_reasoning.enable_thinking, None);
}

#[test]
fn save_llm_config_rejects_the_bango_ai_runtime_provider() {
    let conn = test_db();
    let config = local_config("http://127.0.0.1:9/v1");
    let err = save_config(&conn, &config).expect_err("runtime provider must never persist");
    assert!(err.to_string().contains("managed automatically"), "got: {err}");
    assert!(get_config(&conn).unwrap().is_none(), "no row was written");
}

#[test]
fn resolver_uses_persisted_context_and_threads() {
    ensure_engine();
    let conn = test_db();
    set_llm_backend(&conn, LlmBackend::BangoAi).expect("backend");
    set_bango_ai_settings(&conn, &EngineSettings { context: 8_192, threads: 2, reasoning: false })
        .expect("persist settings");

    // One source of truth: the resolver returns the persisted window, and the
    // settings repo agrees (no RAM recommendation overrides an explicit key).
    let config = resolve(&conn).expect("resolve").expect("local config");
    assert_eq!(config.context_window_tokens, 8_192, "persisted context wins");
    let settings = get_bango_ai_settings(&conn).expect("settings");
    assert_eq!(settings.context, 8_192);
    assert_eq!(settings.threads, 2, "persisted threads win");
    assert!(ALLOWED_CONTEXTS.contains(&settings.context));
}
