//! Integration tests for the embedding provider client (`llm::embedding`).
//!
//! Covers `default_embedding_model` per provider, `check_embedding_support`,
//! the OpenAI-batch request/response parse, the Ollama single-prompt shape,
//! the Google `embedContent` shape, and the `probe_embedding_support` outcome
//! for each scenario (default works, default-404-chat-works, both-fail-disabled,
//! Anthropic-disabled).
//!
//! The HTTP layer is exercised against a mockito server so no real provider
//! is contacted.

#![cfg(test)]

use bango_lib::llm::embedding::{
    check_embedding_support, default_embedding_model, embed_texts, probe_embedding_support,
};
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

/// Build a minimal `LlmConfig` pointing at `endpoint` for `provider`.
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

// ── Pure model resolution ───────────────────────────────────────────────────

#[test]
fn default_embedding_model_openai() {
    assert_eq!(default_embedding_model(&LlmProvider::Openai), Some("text-embedding-3-small"));
}

#[test]
fn default_embedding_model_mistral() {
    assert_eq!(default_embedding_model(&LlmProvider::MistralAi), Some("mistral-embed"));
}

#[test]
fn default_embedding_model_google() {
    assert_eq!(default_embedding_model(&LlmProvider::Google), Some("text-embedding-004"));
}

#[test]
fn default_embedding_model_local_servers_none() {
    assert_eq!(default_embedding_model(&LlmProvider::Ollama), None);
    assert_eq!(default_embedding_model(&LlmProvider::LmStudio), None);
    assert_eq!(default_embedding_model(&LlmProvider::LlamaCpp), None);
}

#[test]
fn default_embedding_model_custom_none() {
    assert_eq!(default_embedding_model(&LlmProvider::Custom), None);
}

#[test]
fn default_embedding_model_unsupported_none() {
    assert_eq!(default_embedding_model(&LlmProvider::Anthropic), None);
    assert_eq!(default_embedding_model(&LlmProvider::ZAi), None);
}

// ── check_embedding_support ─────────────────────────────────────────────────

#[test]
fn check_support_anthropic_disabled() {
    assert!(!check_embedding_support(&LlmProvider::Anthropic));
}

#[test]
fn check_support_zai_disabled() {
    assert!(!check_embedding_support(&LlmProvider::ZAi));
}

#[test]
fn check_support_openai_enabled() {
    assert!(check_embedding_support(&LlmProvider::Openai));
}

#[test]
fn check_support_ollama_enabled() {
    assert!(check_embedding_support(&LlmProvider::Ollama));
}

// ── embed_texts (OpenAI-compatible batch) ───────────────────────────────────

#[tokio::test]
async fn embed_texts_openai_batch_returns_vectors_in_order() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"data":[{"embedding":[0.1,0.2],"index":0},{"embedding":[0.3,0.4],"index":1}]}"#;
    let _m = server
        .mock("POST", "/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let cfg = config(LlmProvider::Openai, &server.url(), "text-embedding-3-small");
    let texts = vec!["hello".to_string(), "world".to_string()];
    let (vectors, dims) = embed_texts(&cfg, &texts, "text-embedding-3-small").await.unwrap();
    assert_eq!(vectors.len(), 2);
    assert_eq!(dims, 2);
    assert_eq!(vectors[0], vec![0.1, 0.2]);
    assert_eq!(vectors[1], vec![0.3, 0.4]);
}

#[tokio::test]
async fn embed_texts_empty_input_rejected() {
    let cfg = config(LlmProvider::Openai, "http://unused", "text-embedding-3-small");
    let result = embed_texts(&cfg, &[], "text-embedding-3-small").await;
    assert!(result.is_err(), "empty input must be rejected before any HTTP call");
}

// ── embed_texts (Google) ────────────────────────────────────────────────────

#[tokio::test]
async fn embed_texts_google_single_text() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"embedding":{"values":[0.7,0.8,0.9]}}"#;
    let _m = server
        .mock("POST", "/models/text-embedding-004:embedContent")
        .match_header("X-goog-api-key", "test-key")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let cfg = config(LlmProvider::Google, &server.url(), "text-embedding-004");
    let (vectors, dims) =
        embed_texts(&cfg, &["probe".to_string()], "text-embedding-004").await.unwrap();
    assert_eq!(vectors.len(), 1);
    assert_eq!(dims, 3);
    assert_eq!(vectors[0], vec![0.7, 0.8, 0.9]);
}

// ── embed_texts (Ollama) ────────────────────────────────────────────────────

#[tokio::test]
async fn embed_texts_ollama_single_prompt() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"embedding":[0.1,0.2,0.3,0.4]}"#;
    let _m = server
        .mock("POST", "/api/embeddings")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let base = format!("{}/v1", server.url());
    let cfg = config(LlmProvider::Ollama, &base, "nomic-embed-text");
    let (vectors, dims) =
        embed_texts(&cfg, &["probe".to_string()], "nomic-embed-text").await.unwrap();
    assert_eq!(vectors.len(), 1);
    assert_eq!(dims, 4);
}

// ── probe_embedding_support ─────────────────────────────────────────────────

#[tokio::test]
async fn probe_anthropic_disabled_immediately() {
    let cfg = config(LlmProvider::Anthropic, "http://unused", "claude-3");
    let outcome = probe_embedding_support(&cfg).await;
    assert_eq!(outcome.status, "disabled");
}

#[tokio::test]
async fn probe_zai_disabled_immediately() {
    let cfg = config(LlmProvider::ZAi, "http://unused", "glm-4");
    let outcome = probe_embedding_support(&cfg).await;
    assert_eq!(outcome.status, "disabled");
}

#[tokio::test]
async fn probe_openai_default_model_enabled() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"data":[{"embedding":[0.1,0.2,0.3],"index":0}]}"#;
    let _m =
        server.mock("POST", "/embeddings").with_status(200).with_body(body).create_async().await;

    let cfg = config(LlmProvider::Openai, &server.url(), "gpt-4o");
    let outcome = probe_embedding_support(&cfg).await;
    assert_eq!(outcome.status, "enabled", "reason: {}", outcome.reason);
    assert_eq!(outcome.model, "text-embedding-3-small");
    assert_eq!(outcome.dimensions, 3);
}

#[tokio::test]
async fn probe_openai_default_404_falls_back_to_chat_model() {
    let mut server = mockito::Server::new_async().await;
    let _m1 = server
        .mock("POST", "/embeddings")
        .with_status(404)
        .with_body(r#"{"error":{"message":"model not found"}}"#)
        .create_async()
        .await;
    let body = r#"{"data":[{"embedding":[0.5,0.6],"index":0}]}"#;
    let _m2 =
        server.mock("POST", "/embeddings").with_status(200).with_body(body).create_async().await;

    let cfg = config(LlmProvider::Openai, &server.url(), "gpt-4o");
    let outcome = probe_embedding_support(&cfg).await;
    assert_eq!(outcome.status, "enabled", "reason: {}", outcome.reason);
    assert_eq!(outcome.model, "gpt-4o", "fell back to the configured chat model");
}

#[tokio::test]
async fn probe_both_models_fail_returns_disabled() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/embeddings")
        .with_status(404)
        .with_body(r#"{"error":{"message":"model not found"}}"#)
        .create_async()
        .await;

    let cfg = config(LlmProvider::Openai, &server.url(), "gpt-4o");
    let outcome = probe_embedding_support(&cfg).await;
    assert_eq!(outcome.status, "disabled", "reason: {}", outcome.reason);
}
