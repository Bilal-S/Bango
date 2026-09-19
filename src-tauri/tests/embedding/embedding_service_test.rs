//! Router tests for the embedding service (`embedding::service`).
//!
//! The cloud branch is exercised against a mockito server so no real provider
//! is contacted. The local branch is state-gated in this tier: the tests pin
//! the actionable error contract until the inference engine lands.

use std::sync::Arc;

use bango_lib::embedding::backend::EmbeddingBackend;
use bango_lib::embedding::local::engine::LocalEngine;
use bango_lib::embedding::local::prompt::EmbeddingRole;
use bango_lib::embedding::local::state::INSTALL_MANIFEST_NAME;
use bango_lib::embedding::service::EmbeddingService;
use bango_lib::llm::orchestrator::LlmOrchestrator;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

/// Build a minimal `LlmConfig` pointing at `endpoint` for `provider`
/// (mirrors `embedding_provider_test.rs`).
fn config(provider: LlmProvider, endpoint: &str, model: &str) -> LlmConfig {
    LlmConfig {
        provider,
        endpoint_url: endpoint.to_string(),
        api_key_encrypted: Some("test-key".to_string()),
        model_name: model.to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

#[tokio::test]
async fn service_cloud_query_routes_through_orchestrator() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[{"embedding":[0.1,0.2],"index":0}],"usage":{"total_tokens":3}}"#)
        .create_async()
        .await;

    let service =
        EmbeddingService::new(Arc::new(LlmOrchestrator::new(1, 0)), Arc::new(LocalEngine::new()));
    let cfg = config(LlmProvider::Openai, &server.url(), "gpt-4o");
    let storage = tempfile::tempdir().unwrap();

    let (vectors, dims) = service
        .embed(
            EmbeddingBackend::ConfiguredProvider,
            storage.path(),
            &cfg,
            &["which planet is known as the red planet".to_string()],
            "text-embedding-3-small",
            EmbeddingRole::Query,
        )
        .await
        .expect("cloud embed succeeds");

    mock.assert_async().await;
    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0], vec![0.1, 0.2]);
    assert_eq!(dims, 2);
}

#[tokio::test]
async fn service_cloud_documents_route_through_orchestrator() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"data":[{"embedding":[0.5,0.5],"index":0},{"embedding":[0.7,0.7],"index":1}]}"#,
        )
        .create_async()
        .await;

    let service =
        EmbeddingService::new(Arc::new(LlmOrchestrator::new(1, 0)), Arc::new(LocalEngine::new()));
    let cfg = config(LlmProvider::Openai, &server.url(), "gpt-4o");
    let storage = tempfile::tempdir().unwrap();

    let (vectors, _dims) = service
        .embed(
            EmbeddingBackend::ConfiguredProvider,
            storage.path(),
            &cfg,
            &["doc one".to_string(), "doc two".to_string()],
            "text-embedding-3-small",
            EmbeddingRole::Document,
        )
        .await
        .expect("cloud embed succeeds");

    mock.assert_async().await;
    // Vectors come back in input order (index-sorted by the provider client).
    assert_eq!(vectors, vec![vec![0.5, 0.5], vec![0.7, 0.7]]);
}

#[tokio::test]
async fn service_local_not_installed_returns_actionable_error() {
    let service =
        EmbeddingService::new(Arc::new(LlmOrchestrator::new(1, 0)), Arc::new(LocalEngine::new()));
    let cfg = config(LlmProvider::Openai, "http://unused", "gpt-4o");
    let storage = tempfile::tempdir().unwrap();

    let err = service
        .embed(
            EmbeddingBackend::BangoLocal,
            storage.path(),
            &cfg,
            &["sugar tax reduces obesity".to_string()],
            "builtin/embeddinggemma-300m-q4@r1",
            EmbeddingRole::Query,
        )
        .await
        .expect_err("local without installation must error");

    let msg = err.to_string();
    assert!(msg.contains("not installed"), "error must name the missing install: {msg}");
    assert!(msg.contains("Settings"), "error must point at Settings: {msg}");
}

#[tokio::test]
async fn service_local_installed_reports_pending_engine() {
    let service =
        EmbeddingService::new(Arc::new(LlmOrchestrator::new(1, 0)), Arc::new(LocalEngine::new()));
    let cfg = config(LlmProvider::Openai, "http://unused", "gpt-4o");
    let storage = tempfile::tempdir().unwrap();
    // Simulate an installed profile: `{storage_root}/model/<profile>/manifest.json`.
    let profile = storage.path().join("model").join("embeddinggemma-300m-q4");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join(INSTALL_MANIFEST_NAME), "{}").unwrap();

    let err = service
        .embed(
            EmbeddingBackend::BangoLocal,
            storage.path(),
            &cfg,
            &["sugar tax reduces obesity".to_string()],
            "builtin/embeddinggemma-300m-q4@r1",
            EmbeddingRole::Query,
        )
        .await
        .expect_err("an incomplete install must error via the engine's health gate");

    assert!(
        err.to_string().contains("not ready"),
        "error must name the engine's health gate: {err}"
    );
    assert!(err.to_string().contains("Settings"), "error must point at Settings: {err}");
}

#[tokio::test]
async fn service_local_missing_storage_root_reports_not_installed() {
    let service =
        EmbeddingService::new(Arc::new(LlmOrchestrator::new(1, 0)), Arc::new(LocalEngine::new()));
    let cfg = config(LlmProvider::Openai, "http://unused", "gpt-4o");

    // An empty storage root (side-effect-free read found no configured root)
    // must read as not-installed, never as a relative-path probe.
    let err = service
        .embed(
            EmbeddingBackend::BangoLocal,
            std::path::Path::new(""),
            &cfg,
            &["sugar tax reduces obesity".to_string()],
            "builtin/embeddinggemma-300m-q4@r1",
            EmbeddingRole::Query,
        )
        .await
        .expect_err("missing storage root must report not-installed");

    assert!(
        err.to_string().contains("not installed"),
        "error must name the missing install: {err}"
    );
}
