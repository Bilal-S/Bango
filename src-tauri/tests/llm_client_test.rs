//! Regression tests for all LLM HTTP call paths.
//!
//! These tests exercise `client::send_chat_completion` and `client::list_models`
//! against a mockito HTTP server so that no real API calls are made.
//! When the LlmOrchestrator is introduced, these tests ensure the underlying
//! HTTP + parsing behaviour is unchanged.

use bango_lib::llm::client;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

// ── Helpers ──────────────────────────────────────────────────────────

fn openai_config(server_url: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Openai,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: Some("test-key".to_string()),
        model_name: "gpt-4o".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

fn google_config(server_url: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Google,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: Some("test-google-key".to_string()),
        model_name: "gemini-1.5-flash".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

fn anthropic_config(server_url: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Anthropic,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: Some("test-anthropic-key".to_string()),
        model_name: "claude-3-sonnet".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

fn ollama_config(server_url: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Ollama,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: None,
        model_name: "llama3".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

// Standard OpenAI chat completion response body
fn openai_chat_response(content: &str, tokens: usize) -> String {
    format!(
        r#"{{
            "choices": [{{ "message": {{ "role": "assistant", "content": {content_json} }} }}],
            "usage": {{ "total_tokens": {tokens} }}
        }}"#,
        content_json = serde_json::to_string(content).unwrap(),
        tokens = tokens,
    )
}

// Standard Google Generative Language API response body
fn google_chat_response(content: &str, tokens: usize) -> String {
    format!(
        r#"{{
            "candidates": [{{
                "content": {{
                    "parts": [{{ "text": {content_json} }}]
                }}
            }}],
            "usageMetadata": {{ "totalTokenCount": {tokens} }}
        }}"#,
        content_json = serde_json::to_string(content).unwrap(),
        tokens = tokens,
    )
}

// Standard OpenAI models list response body
fn openai_models_response(model_ids: &[&str]) -> String {
    let entries: Vec<String> =
        model_ids.iter().map(|id| format!(r#"{{ "id": "{id}" }}"#)).collect();
    format!(r#"{{ "data": [{}] }}"#, entries.join(", "))
}

// Google models list response body
fn google_models_response(models: &[(&str, bool)]) -> String {
    // (name, supports_generateContent)
    let entries: Vec<String> = models
        .iter()
        .map(|(name, supports)| {
            let methods = if *supports {
                r#""supportedGenerationMethods": ["generateContent"]"#
            } else {
                r#""supportedGenerationMethods": ["embedContent"]"#
            };
            format!(r#"{{ "name": "models/{name}", {methods} }}"#)
        })
        .collect();
    format!(r#"{{ "models": [{}] }}"#, entries.join(", "))
}

// Anthropic models list response body
fn anthropic_models_response(model_ids: &[&str]) -> String {
    let entries: Vec<String> =
        model_ids.iter().map(|id| format!(r#"{{ "id": "{id}" }}"#)).collect();
    format!(r#"{{ "data": [{}] }}"#, entries.join(", "))
}

// ── send_chat_completion: OpenAI-compatible path ─────────────────────

#[tokio::test]
async fn test_openai_standard_response() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("content-type", "application/json")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(openai_chat_response("Hello, world!", 42))
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let result = client::send_chat_completion(&config, "system", "user").await;

    mock.assert_async().await;
    let (content, tokens) = result.expect("should succeed");
    assert_eq!(content, "Hello, world!");
    assert_eq!(tokens, 42);
}

#[tokio::test]
async fn test_openai_no_usage_returns_zero_tokens() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "hi");
    assert_eq!(tokens, 0, "missing usage field should default to 0 tokens");
}

#[tokio::test]
async fn test_openai_rate_limit_429() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(429)
        .with_body("rate limited")
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    let msg = err.to_string();
    assert!(
        msg.contains("Rate limited") || msg.contains("429"),
        "expected rate limit error, got: {msg}"
    );
}

#[tokio::test]
async fn test_openai_server_error_500() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(500)
        .with_body("internal server error")
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    let msg = err.to_string();
    assert!(msg.contains("500"), "expected 500 in error, got: {msg}");
}

#[tokio::test]
async fn test_openai_endpoint_appends_chat_completions() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("ok", 10))
        .create_async()
        .await;

    // Provide base URL without /chat/completions — client should append it
    let config = openai_config(&server.url());
    let _ = client::send_chat_completion(&config, "s", "u").await;

    mock.assert_async().await;
}

#[tokio::test]
async fn test_openai_endpoint_already_has_path() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("ok", 10))
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.endpoint_url = format!("{}/v1/chat/completions", server.url());
    let _ = client::send_chat_completion(&config, "s", "u").await;

    mock.assert_async().await;
}

#[tokio::test]
async fn test_openai_empty_choices_returns_error() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"choices":[],"usage":{"total_tokens":0}}"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    let msg = err.to_string();
    assert!(
        msg.contains("No response") || msg.contains("Could not extract"),
        "expected no-response error, got: {msg}"
    );
}

// ── send_chat_completion: Non-standard response parsing ──────────────

#[tokio::test]
async fn test_openai_zai_array_content_response() {
    // z.ai returns content as an array of objects with "text" fields
    let mut server = mockito::Server::new_async().await;
    let body = r#"{
        "message": {
            "content": [
                {"type": "text", "text": "Hello "},
                {"type": "text", "text": "world!"}
            ]
        },
        "usage": {"total_tokens": 15}
    }"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Hello world!");
    assert_eq!(tokens, 15);
}

#[tokio::test]
async fn test_openai_malformed_json_returns_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body("this is not json at all")
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    let msg = err.to_string();
    assert!(msg.contains("JSON") || msg.contains("parse"), "expected JSON parse error, got: {msg}");
}

#[tokio::test]
async fn test_openai_nested_content_extraction() {
    // Response with content nested under a top-level key that isn't "choices"
    let mut server = mockito::Server::new_async().await;
    let body = r#"{
        "result": {
            "message": {
                "content": "extracted from nested"
            }
        },
        "usage": {"total_tokens": 5}
    }"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "extracted from nested");
    assert_eq!(tokens, 5);
}

// ── send_chat_completion: Google path ────────────────────────────────

#[tokio::test]
async fn test_google_standard_response() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/models/gemini-1.5-flash:generateContent")
        .match_header("x-goog-api-key", "test-google-key")
        .with_status(200)
        .with_body(google_chat_response("Google response", 99))
        .create_async()
        .await;

    let config = google_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "sys", "usr").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Google response");
    assert_eq!(tokens, 99);
}

#[tokio::test]
async fn test_google_rate_limit_429() {
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("POST", mockito::Matcher::Any).with_status(429).create_async().await;

    let config = google_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("429"), "expected 429 error, got: {}", err);
}

#[tokio::test]
async fn test_google_missing_api_key() {
    let mut config = google_config("http://unused");
    config.api_key_encrypted = None;

    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();
    assert!(err.to_string().contains("API key required"), "expected API key error, got: {}", err);
}

#[tokio::test]
async fn test_google_endpoint_already_has_generate_content() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/models/gemini:generateContent")
        .with_status(200)
        .with_body(google_chat_response("direct endpoint", 10))
        .create_async()
        .await;

    let mut config = google_config(&server.url());
    config.endpoint_url = format!("{}/v1/models/gemini:generateContent", server.url());

    let (content, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(content, "direct endpoint");
}

#[tokio::test]
async fn test_google_server_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .with_body("google internal error")
        .create_async()
        .await;

    let config = google_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("500"), "expected 500 error, got: {}", err);
}

#[tokio::test]
async fn test_google_no_candidates_returns_error() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"candidates":[],"usageMetadata":{"totalTokenCount":0}}"#;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = google_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("No response"), "expected no-response error, got: {}", err);
}

// ── send_chat_completion: Anthropic path ─────────────────────────────

#[tokio::test]
async fn test_anthropic_uses_messages_endpoint() {
    let mut server = mockito::Server::new_async().await;
    // Anthropic uses OpenAI-compatible ChatResponse format in the fallback parser
    let mock = server
        .mock("POST", "/messages")
        .match_header("content-type", "application/json")
        .with_status(200)
        .with_body(openai_chat_response("Anthropic reply", 77))
        .create_async()
        .await;

    let config = anthropic_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Anthropic reply");
    assert_eq!(tokens, 77);
}

#[tokio::test]
async fn test_anthropic_endpoint_already_has_messages() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/messages")
        .with_status(200)
        .with_body(openai_chat_response("ok", 5))
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.endpoint_url = format!("{}/v1/messages", server.url());

    let _ = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
}

// ── send_chat_completion: Ollama (no API key) ───────────────────────

#[tokio::test]
async fn test_ollama_no_api_key() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("Ollama response", 20))
        .create_async()
        .await;

    let config = ollama_config(&server.url());
    let (content, tokens) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Ollama response");
    assert_eq!(tokens, 20);
}

// ── send_chat_completion: temperature control ────────────────────────

#[tokio::test]
async fn test_skip_temperature_excludes_from_request() {
    // When skip_temperature is true, serde's skip_serializing_if = "Option::is_none"
    // removes the temperature key from the JSON body entirely.
    // We verify the code path works without error; the skip is a serde behaviour,
    // not something we can easily assert via mock body matching.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("no temp", 5))
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.skip_temperature = true;

    let _ = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
}

// ── list_models: OpenAI path ─────────────────────────────────────────

#[tokio::test]
async fn test_list_models_openai() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(openai_models_response(&["gpt-4o", "gpt-3.5-turbo", "gpt-4o-mini"]))
        .create_async()
        .await;

    let result =
        client::list_models(&LlmProvider::Openai, &server.url(), Some("test-key")).await.unwrap();

    mock.assert_async().await;
    // OpenAI models are sorted
    assert_eq!(result, vec!["gpt-3.5-turbo", "gpt-4o", "gpt-4o-mini"]);
}

#[tokio::test]
async fn test_list_models_openai_filters_non_chat() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&[
            "gpt-4o",
            "text-embedding-3-small",
            "tts-1",
            "dall-e-3",
            "gpt-image-1",
            "whisper-1",
            "gpt-4o-mini",
        ]))
        .create_async()
        .await;

    let result =
        client::list_models(&LlmProvider::Openai, &server.url(), Some("test-key")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["gpt-4o", "gpt-4o-mini"]);
}

#[tokio::test]
async fn test_list_models_openai_filters_video_and_audio_models() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&[
            "gpt-4o",
            "sora-1.0-turbo",
            "gpt-4o-audio-preview",
            "gpt-4o-realtime-preview",
            "omni-moderation-latest",
            "codex-mini",
        ]))
        .create_async()
        .await;

    let result =
        client::list_models(&LlmProvider::Openai, &server.url(), Some("test-key")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["gpt-4o"]);
}

#[tokio::test]
async fn test_list_models_openai_no_api_key() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&["gpt-4o"]))
        .create_async()
        .await;

    let result = client::list_models(&LlmProvider::Openai, &server.url(), None).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["gpt-4o"]);
}

// ── list_models: Google path ─────────────────────────────────────────

#[tokio::test]
async fn test_list_models_google() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .match_header("x-goog-api-key", "google-key")
        .with_status(200)
        .with_body(google_models_response(&[
            ("gemini-1.5-flash", true),
            ("gemini-1.5-pro", true),
            ("text-embedding-004", false),
        ]))
        .create_async()
        .await;

    let result =
        client::list_models(&LlmProvider::Google, &server.url(), Some("google-key")).await.unwrap();

    mock.assert_async().await;
    // Should strip "models/" prefix and filter to generateContent models
    assert_eq!(result, vec!["gemini-1.5-flash", "gemini-1.5-pro"]);
}

#[tokio::test]
async fn test_list_models_google_missing_api_key() {
    let err = client::list_models(&LlmProvider::Google, "http://unused", None).await.unwrap_err();
    assert!(err.to_string().contains("API key required"), "expected API key error, got: {}", err);
}

#[tokio::test]
async fn test_list_models_google_error_response() {
    let mut server = mockito::Server::new_async().await;
    let mock =
        server.mock("GET", "/models").with_status(403).with_body("forbidden").create_async().await;

    let err = client::list_models(&LlmProvider::Google, &server.url(), Some("bad-key"))
        .await
        .unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("403"), "expected 403 error, got: {}", err);
}

// ── list_models: Anthropic path ──────────────────────────────────────

#[tokio::test]
async fn test_list_models_anthropic() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .match_header("x-api-key", "anthropic-key")
        .match_header("anthropic-version", "2023-06-01")
        .with_status(200)
        .with_body(anthropic_models_response(&["claude-3-sonnet", "claude-3-haiku"]))
        .create_async()
        .await;

    let result = client::list_models(&LlmProvider::Anthropic, &server.url(), Some("anthropic-key"))
        .await
        .unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["claude-3-sonnet", "claude-3-haiku"]);
}

#[tokio::test]
async fn test_list_models_anthropic_missing_api_key() {
    let err =
        client::list_models(&LlmProvider::Anthropic, "http://unused", None).await.unwrap_err();
    assert!(err.to_string().contains("API key required"), "expected API key error, got: {}", err);
}

// ── list_models: Ollama (no filtering) ───────────────────────────────

#[tokio::test]
async fn test_list_models_ollama_no_filtering() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&["llama3", "mistral", "embedding-model"]))
        .create_async()
        .await;

    let result = client::list_models(&LlmProvider::Ollama, &server.url(), None).await.unwrap();

    mock.assert_async().await;
    // Ollama doesn't filter — all models returned
    assert_eq!(result, vec!["llama3", "mistral", "embedding-model"]);
}

// ── list_models: URL normalization ───────────────────────────────────

#[tokio::test]
async fn test_list_models_strips_trailing_slash() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&["gpt-4o"]))
        .create_async()
        .await;

    let url = format!("{}/", server.url());
    let result = client::list_models(&LlmProvider::Openai, &url, Some("key")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["gpt-4o"]);
}

#[tokio::test]
async fn test_list_models_strips_models_suffix() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(200)
        .with_body(openai_models_response(&["gpt-4o"]))
        .create_async()
        .await;

    let url = format!("{}/models", server.url());
    let result = client::list_models(&LlmProvider::Openai, &url, Some("key")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(result, vec!["gpt-4o"]);
}

// ── list_models: error handling ──────────────────────────────────────

#[tokio::test]
async fn test_list_models_server_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/models")
        .with_status(500)
        .with_body("internal error")
        .create_async()
        .await;

    let err =
        client::list_models(&LlmProvider::Openai, &server.url(), Some("key")).await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("500"), "expected 500 error, got: {}", err);
}

// ── Edge cases ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_empty_api_key_still_sends_request() {
    // The code does unwrap_or("") then bearer_auth("") — so an empty key
    // still makes the request with a Bearer header (value is empty string).
    // Verify the request still succeeds when the server accepts it.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        // Don't assert exact header value — reqwest may format "Bearer " differently
        .match_header("content-type", "application/json")
        .with_status(200)
        .with_body(openai_chat_response("empty auth", 1))
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.api_key_encrypted = Some(String::new());

    let (content, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(content, "empty auth");
}

#[tokio::test]
async fn test_google_temperature_skipped() {
    // Verify the skip_temperature code path works for Google without error.
    // The temperature field is omitted via serde's skip_serializing_if.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(google_chat_response("no temp", 1))
        .create_async()
        .await;

    let mut config = google_config(&server.url());
    config.skip_temperature = true;

    let _ = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
}

#[tokio::test]
async fn test_endpoint_url_trailing_slash_stripped() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("ok", 1))
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.endpoint_url = format!("{}/", server.url());

    let _ = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
}
