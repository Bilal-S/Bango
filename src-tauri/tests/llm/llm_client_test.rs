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

// Standard Anthropic Messages API response body (usage = input + output)
fn anthropic_chat_response(content: &str, input_tokens: usize, output_tokens: usize) -> String {
    format!(
        r#"{{"content":[{{"type":"text","text":{content_json}}}],"usage":{{"input_tokens":{input_tokens},"output_tokens":{output_tokens}}}}}"#,
        content_json = serde_json::to_string(content).unwrap(),
        input_tokens = input_tokens,
        output_tokens = output_tokens,
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
    let (content, tokens, _) = result.expect("should succeed");
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
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "hi");
    assert_eq!(tokens, 0, "missing usage field should default to 0 tokens");
}

#[tokio::test]
#[ignore = "slow"]
async fn test_openai_rate_limit_429() {
    // 429 is a transient status: the client retries up to 3 times (4 total
    // attempts) before surfacing the error. This matches the spec §4.3
    // "Exponential backoff handles rate-limiting (429 errors)" contract.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(429)
        .with_body("rate limited")
        .expect(4) // 1 initial + 3 retries
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
#[ignore = "slow"]
async fn test_openai_server_error_500() {
    // 500 is a transient server error: the client retries up to 3 times
    // (4 total attempts) before surfacing the error.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(500)
        .with_body("internal server error")
        .expect(4) // 1 initial + 3 retries
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    let msg = err.to_string();
    assert!(msg.contains("500"), "expected 500 in error, got: {msg}");
}

#[tokio::test]
#[ignore = "slow"]
async fn test_openai_insufficient_permissions_403_is_retried_then_succeeds() {
    // Regression test for the Windows-only intermittent "insufficient permissions"
    // gateway error. The body matches the empirically-observed transient exactly
    // (an `invalid_request_error` with "insufficient permissions for this
    // operation."). The client must retry this signature on 401/403 and succeed
    // once the gateway stabilizes, rather than surfacing the error immediately.
    let mut server = mockito::Server::new_async().await;
    let error_body = r#"{"error":{"message":"You have insufficient permissions for this operation.","type":"invalid_request_error","param":null,"code":null}}"#;

    // First two attempts: transient 403 with the gated body.
    let transient = server
        .mock("POST", "/chat/completions")
        .with_status(403)
        .with_header("x-request-id", "req_transient_abc")
        .with_header("cf-ray", "abc123-ATL")
        .with_body(error_body)
        .expect(2)
        .create_async()
        .await;

    // Third attempt: success.
    let success = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("recovered", 7))
        .expect(1)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    transient.assert_async().await;
    success.assert_async().await;
    assert_eq!(content, "recovered");
    assert_eq!(tokens, 7);
}

#[tokio::test]
async fn test_openai_real_auth_401_is_not_retried() {
    // A plain 401 WITHOUT the "insufficient permissions" body is a real auth
    // failure (wrong/revoked key). The client must NOT retry it (avoid burning
    // budget on a permanent error); it should surface the error after exactly
    // one attempt.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(401)
        .with_body(
            r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#,
        )
        .expect(1) // no retry
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("401"), "expected 401 error, got: {}", err);
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

    // Provide base URL without /chat/completions - client should append it
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

// ── send_chat_completion: Truncation (finish_reason=length) ──────────

#[tokio::test]
async fn test_openai_truncated_response_still_returns_content() {
    // When finish_reason="length" the server hit the output-token budget and
    // the content may be cut off mid-sentence. The client must NOT discard the
    // partial content (callers rely on the markdown-fallback retry elsewhere to
    // handle empty/garbage); it should return the truncated text and log a
    // diagnostic. This test pins the "return content" half of that contract.
    let mut server = mockito::Server::new_async().await;
    let body = r#"{
        "choices": [{
            "message": {"role": "assistant", "content": "truncated mid sent"},
            "finish_reason": "length"
        }],
        "usage": {"total_tokens": 128}
    }"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "truncated mid sent", "truncated content must still be returned");
    assert_eq!(tokens, 128);
}

#[tokio::test]
async fn test_openai_normal_finish_reason_returns_content() {
    // finish_reason="stop" is the normal-completion path; must return content
    // unchanged. Guards against a regression where the length-warning branch
    // accidentally interferes with the stop branch.
    let mut server = mockito::Server::new_async().await;
    let body = r#"{
        "choices": [{
            "message": {"role": "assistant", "content": "complete answer"},
            "finish_reason": "stop"
        }],
        "usage": {"total_tokens": 10}
    }"#;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "complete answer");
    assert_eq!(tokens, 10);
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
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

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
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

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
    let (content, tokens, _) = client::send_chat_completion(&config, "sys", "usr").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Google response");
    assert_eq!(tokens, 99);
}

#[tokio::test]
#[ignore = "slow"]
async fn test_google_rate_limit_429() {
    // 429 is transient: the Google path now retries (parity with the OpenAI
    // path), so the mock receives 4 requests (1 initial + 3 retries).
    let mut server = mockito::Server::new_async().await;
    let mock =
        server.mock("POST", mockito::Matcher::Any).with_status(429).expect(4).create_async().await;

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

    let (content, _, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(content, "direct endpoint");
}

#[tokio::test]
#[ignore = "slow"]
async fn test_google_server_error() {
    // 500 is transient: the Google path retries (parity with OpenAI),
    // so the mock receives 4 requests (1 initial + 3 retries).
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .with_body("google internal error")
        .expect(4)
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

// ── send_chat_completion: Anthropic path (native Messages API) ────────

#[tokio::test]
async fn test_anthropic_sends_required_headers() {
    // Every Anthropic request must carry `anthropic-version` (missing header
    // => 400 "anthropic-version: header is required") and the `x-api-key`
    // auth header, matching the `list_models` path.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/messages")
        .match_header("content-type", "application/json")
        .match_header("x-api-key", "test-anthropic-key")
        .match_header("anthropic-version", "2023-06-01")
        .with_status(200)
        .with_body(anthropic_chat_response("Anthropic reply", 30, 47))
        .create_async()
        .await;

    let config = anthropic_config(&server.url());
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "Anthropic reply");
    assert_eq!(tokens, 77, "usage total = input_tokens + output_tokens");
}

#[tokio::test]
async fn test_anthropic_native_request_shape() {
    // Native Messages API body: system prompt is a TOP-LEVEL field (the API
    // has no "system" role), a single user message, and the required
    // `max_tokens` budget. Temperature is carried when not skipped.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "model": "claude-3-sonnet",
            "max_tokens": 32768,
            "system": "be brief",
            "temperature": 0.2,
            "messages": [{ "role": "user", "content": "hello" }]
        })))
        .with_status(200)
        .with_body(anthropic_chat_response("ok", 1, 2))
        .create_async()
        .await;

    let config = anthropic_config(&server.url());
    let _ = client::send_chat_completion(&config, "be brief", "hello").await.unwrap();
    mock.assert_async().await;
}

#[tokio::test]
async fn test_anthropic_joins_multiple_text_blocks() {
    // Multi-block responses interleave text with non-text blocks (e.g.
    // `tool_use`) that carry no `text` field; only text blocks concatenate.
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"content":[{"type":"text","text":"part one "},{"type":"tool_use","id":"toolu_1"},{"type":"text","text":"part two"}],"usage":{"input_tokens":5,"output_tokens":7}}"#;
    let mock =
        server.mock("POST", "/messages").with_status(200).with_body(body).create_async().await;

    let config = anthropic_config(&server.url());
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "part one part two");
    assert_eq!(tokens, 12);
}

#[tokio::test]
async fn test_anthropic_missing_api_key() {
    let mut config = anthropic_config("http://unused");
    config.api_key_encrypted = None;

    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();
    assert!(err.to_string().contains("API key required"), "expected API key error, got: {err}");
}

#[tokio::test]
async fn test_anthropic_endpoint_already_has_messages() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/v1/messages")
        .with_status(200)
        .with_body(anthropic_chat_response("ok", 1, 4))
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.endpoint_url = format!("{}/v1/messages", server.url());

    let _ = client::send_chat_completion(&config, "s", "u").await.unwrap();
    mock.assert_async().await;
}

#[tokio::test]
async fn test_anthropic_empty_content_returns_error() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"content":[],"usage":{"input_tokens":3,"output_tokens":0}}"#;
    let mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let config = anthropic_config(&server.url());
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("No response"), "expected no-response error, got: {err}");
}

#[tokio::test]
async fn test_anthropic_temperature_400_retries_without_temperature() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"temperature does not support 0.2 with this model. Only the default (1) value is supported."}}"#;

    // First attempt: 400 with a temperature-rejection body (request carries
    // temperature because skip_temperature=false in anthropic_config).
    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({"temperature": 0.2})))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    // Second attempt: 200. The rebuilt body must carry the native shape
    // WITHOUT temperature (serde skips None).
    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "claude-3-sonnet",
                "max_tokens": 32768,
                "system": "s",
                "messages": [{ "role": "user", "content": "u" }]
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(anthropic_chat_response("recovered", 2, 3))
        .expect(1)
        .create_async()
        .await;

    let config = anthropic_config(&server.url());
    let (_, tokens, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(tokens, 5);
    assert!(meta.temperature_was_rejected, "recovery must be surfaced in CallMeta");
}

#[tokio::test]
async fn test_anthropic_over_cap_backs_down_to_reported_limit() {
    // Exact production body: the 400 message states the model's true output
    // cap; the retry must carry that parsed cap as max_tokens and surface it
    // via CallMeta. Unique model name so the session cap latch never collides
    // with other tests in this binary.
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 32768 > 4096, which is the maximum allowed number of output tokens for claude-3-opus-20240229-probe-a"}}"#;

    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 32768 })))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 4096 })))
        .with_status(200)
        .with_body(anthropic_chat_response("backed down", 4, 5))
        .expect(1)
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.model_name = "claude-3-opus-20240229-probe-a".to_string();
    let (content, tokens, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(content, "backed down");
    assert_eq!(tokens, 9);
    assert_eq!(meta.max_tokens_backed_down, Some(4096));
}

#[tokio::test]
async fn test_anthropic_over_cap_unparseable_falls_back_to_4096() {
    // A proxy may reword the message while keeping the marker sentence;
    // without a parseable `> N` limit the back-down targets the
    // universal-safe floor.
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: value above the limit, which is the maximum allowed number of output tokens for this model"}}"#;

    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 32768 })))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 4096 })))
        .with_status(200)
        .with_body(anthropic_chat_response("safe floor", 1, 1))
        .expect(1)
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.model_name = "claude-proxy-model-probe-b".to_string();
    let (_, _, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(meta.max_tokens_backed_down, Some(4096));
}

#[tokio::test]
async fn test_anthropic_over_cap_uses_reported_8192_not_floor() {
    // claude-3.5-era models report 8192; the parsed limit wins over the
    // conservative 4096 floor (probe the capability, not a fixed budget).
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 32768 > 8192, which is the maximum allowed number of output tokens for claude-3-5-sonnet-probe-c"}}"#;

    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 32768 })))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 8192 })))
        .with_status(200)
        .with_body(anthropic_chat_response("8k model", 2, 2))
        .expect(1)
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.model_name = "claude-3-5-sonnet-probe-c".to_string();
    let (_, _, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(meta.max_tokens_backed_down, Some(8192));
}

#[tokio::test]
async fn test_anthropic_cap_latch_skips_probe_on_second_call() {
    // After a back-down, the discovered cap is latched per model for the
    // session: the next call goes straight out at the latched budget with no
    // failed-request round trip (meta carries no back-down flag).
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 32768 > 4096, which is the maximum allowed number of output tokens for claude-3-opus-probe-d"}}"#;

    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 32768 })))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    // Hit twice: once by the back-down retry, once by the direct second call.
    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 4096 })))
        .with_status(200)
        .with_body(anthropic_chat_response("latched", 3, 3))
        .expect(2)
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.model_name = "claude-3-opus-probe-d".to_string();

    let (_, _, first_meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    assert_eq!(first_meta.max_tokens_backed_down, Some(4096));

    let (_, _, second_meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    assert_eq!(second_meta.max_tokens_backed_down, None, "second call must skip the probe");

    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn test_anthropic_persistent_over_cap_surfaces_error() {
    // Loop guard: exactly one back-down per call. A server rejecting even the
    // backed-down budget (broken proxy) surfaces the error instead of looping.
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 32768 > 4096, which is the maximum allowed number of output tokens for claude-broken-proxy-probe-e"}}"#;

    let mock = server
        .mock("POST", "/messages")
        .with_status(400)
        .with_body(error_body)
        // Exactly 2 requests: the initial ask + the single backed-down retry.
        .expect(2)
        .create_async()
        .await;

    let mut config = anthropic_config(&server.url());
    config.model_name = "claude-broken-proxy-probe-e".to_string();
    let err = client::send_chat_completion(&config, "s", "u").await.unwrap_err();

    mock.assert_async().await;
    assert!(err.to_string().contains("max_tokens"), "expected over-cap error, got: {err}");
}

#[tokio::test]
async fn test_anthropic_stop_reason_max_tokens_returns_content() {
    // stop_reason=max_tokens means the output was truncated by the budget;
    // the content still parses and returns (the diagnostic goes to stderr).
    let mut server = mockito::Server::new_async().await;

    let body = r#"{"content":[{"type":"text","text":"partial answer"}],"stop_reason":"max_tokens","usage":{"input_tokens":10,"output_tokens":4096}}"#;
    let mock =
        server.mock("POST", "/messages").with_status(200).with_body(body).create_async().await;

    let config = anthropic_config(&server.url());
    let (content, tokens, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();

    mock.assert_async().await;
    assert_eq!(content, "partial answer");
    assert_eq!(tokens, 4106);
    // Change 4: the stop reason now surfaces in CallMeta as truncation.
    assert_eq!(meta.finish_reason.as_deref(), Some("max_tokens"));
    assert!(meta.truncated_by_output_budget());
    assert!(!meta.temperature_was_rejected);
    assert_eq!(meta.max_tokens_backed_down, None);
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
    let (content, tokens, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();

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
    // Ollama doesn't filter - all models returned
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
#[ignore = "slow"]
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
    // The code does unwrap_or("") then bearer_auth("") - so an empty key
    // still makes the request with a Bearer header (value is empty string).
    // Verify the request still succeeds when the server accepts it.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        // Don't assert exact header value - reqwest may format "Bearer " differently
        .match_header("content-type", "application/json")
        .with_status(200)
        .with_body(openai_chat_response("empty auth", 1))
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.api_key_encrypted = Some(String::new());

    let (content, _, _) = client::send_chat_completion(&config, "s", "u").await.unwrap();
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

// ─── Temperature-rejection recovery (client-level retry) ───────────────
//
// When a model rejects a non-default `temperature` (HTTP 400 with a body
// matching `is_temperature_error`), the client rebuilds the request with
// `temperature` omitted and retries once. `CallMeta.temperature_was_rejected`
// is set to `true` on the recovery path so the orchestrator can persist
// `skip_temperature = true`. Recovery is skipped when `skip_temperature` is
// already `true` (nothing to recover from).

use bango_lib::llm::client::CallMeta;

#[tokio::test]
#[ignore = "slow"]
async fn test_openai_temperature_400_retries_without_temperature() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model. Only the default (1) value is supported.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;

    // First attempt: 400 with the temperature-rejection body. The request
    // body MUST contain "temperature" (the config has skip_temperature=false
    // + temperature=0.2 from openai_config).
    let first = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({"temperature": 0.2})))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    // Second attempt: 200. The request body MUST NOT contain "temperature"
    // (the recovery rebuilds with temp=None, which serde skips).
    let second = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "system", "content": "s"},
                    {"role": "user", "content": "u"},
                ],
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(openai_chat_response("recovered", 7))
        .expect(1)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let result = client::send_chat_completion(&config, "s", "u").await;

    first.assert_async().await;
    second.assert_async().await;
    let (content, _tokens, meta) = result.expect("should recover and succeed");
    assert_eq!(content, "recovered");
    assert!(
        meta.temperature_was_rejected,
        "CallMeta.temperature_was_rejected must be true after a recovery"
    );
}

#[tokio::test]
async fn test_openai_temperature_400_with_skip_temperature_true_does_not_retry() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;

    // Only one request expected: skip_temperature=true means temperature is
    // already omitted, so a 400 cannot be a temperature rejection and there
    // is nothing to retry.
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;

    let mut config = openai_config(&server.url());
    config.skip_temperature = true;
    let result = client::send_chat_completion(&config, "s", "u").await;

    mock.assert_async().await;
    assert!(result.is_err(), "should fail without retry when skip_temperature is already true");
}

#[tokio::test]
async fn test_openai_nontemperature_400_does_not_retry() {
    let mut server = mockito::Server::new_async().await;

    // 400 with a body that does NOT match the temperature pattern. The client
    // must surface the error immediately (no second attempt).
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(400)
        .with_body(r#"{"error":{"message":"Invalid model","type":"invalid_request_error"}}"#)
        .expect(1)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let result = client::send_chat_completion(&config, "s", "u").await;

    mock.assert_async().await;
    assert!(result.is_err(), "non-temperature 400 must surface immediately");
}

#[tokio::test]
async fn test_openai_success_returns_default_callmeta() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("ok", 3))
        .expect(1)
        .create_async()
        .await;

    let config = openai_config(&server.url());
    let (content, _tokens, meta) =
        client::send_chat_completion(&config, "s", "u").await.expect("should succeed");

    mock.assert_async().await;
    assert_eq!(content, "ok");
    assert_eq!(
        meta,
        CallMeta::default(),
        "normal success must return CallMeta::default (no rejection)"
    );
}

// ---- Binding inventory implementations (`docs/test-plans/wiki-output-budget-tests.md`,
// `.worktrees/wikifix-final.md` Change 4 output-cap policy). ----

/// `CallMeta` surfaces truncation for `finish_reason=length`,
/// `stop_reason=max_tokens`, and the Google `finishReason=MAX_TOKENS`.
#[tokio::test]
async fn call_meta_flags_truncation_from_finish_and_stop_reasons() {
    // OpenAI: finish_reason = "length".
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(
            r#"{"choices":[{"message":{"role":"assistant","content":"partial"},
                 "finish_reason":"length"}],"usage":{"total_tokens":10}}"#,
        )
        .create_async()
        .await;
    let (_c, _t, meta) =
        client::send_chat_completion(&openai_config(&server.url()), "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(meta.finish_reason.as_deref(), Some("length"));
    assert!(meta.truncated_by_output_budget(), "{meta:?}");

    // Anthropic: stop_reason = "max_tokens".
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/messages")
        .with_status(200)
        .with_body(
            r#"{"content":[{"type":"text","text":"partial"}],
                "usage":{"input_tokens":1,"output_tokens":2},"stop_reason":"max_tokens"}"#,
        )
        .create_async()
        .await;
    let (_c, _t, meta) =
        client::send_chat_completion(&anthropic_config(&server.url()), "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(meta.finish_reason.as_deref(), Some("max_tokens"));
    assert!(meta.truncated_by_output_budget(), "{meta:?}");

    // Google: finishReason = "MAX_TOKENS".
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/models/gemini-1.5-flash:generateContent")
        .with_status(200)
        .with_body(
            r#"{"candidates":[{"content":{"parts":[{"text":"partial"}]},
                "finishReason":"MAX_TOKENS"}]}"#,
        )
        .create_async()
        .await;
    let (_c, _t, meta) =
        client::send_chat_completion(&google_config(&server.url()), "s", "u").await.unwrap();
    mock.assert_async().await;
    assert_eq!(meta.finish_reason.as_deref(), Some("MAX_TOKENS"));
    assert!(meta.truncated_by_output_budget(), "{meta:?}");

    // Default (no reason reported) is not truncated.
    assert!(!bango_lib::llm::client::CallMeta::default().truncated_by_output_budget());
}

/// Anthropic keeps the ceiling-not-target `max_tokens` plus over-cap
/// back-down; NO output-cap field is ever added to the OpenAI-compatible or
/// Google request bodies (user ruling: no generic output restriction).
#[tokio::test]
async fn anthropic_request_keeps_max_tokens_ceiling_and_backdown() {
    // OpenAI-compatible body: exact-shape match with NO output-cap field.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    { "role": "system", "content": "s" },
                    { "role": "user", "content": "u" }
                ],
                "temperature": 0.2
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(openai_chat_response("ok", 1))
        .expect(1)
        .create_async()
        .await;
    client::send_chat_completion(&openai_config(&server.url()), "s", "u").await.unwrap();
    mock.assert_async().await;

    // Google body: exact-shape match with NO maxOutputTokens field.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/models/gemini-1.5-flash:generateContent")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "systemInstruction": { "parts": { "text": "s" } },
                "contents": [ { "role": "user", "parts": [ { "text": "u" } ] } ],
                "generationConfig": { "temperature": 0.2 }
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]}}]}"#)
        .expect(1)
        .create_async()
        .await;
    client::send_chat_completion(&google_config(&server.url()), "s", "u").await.unwrap();
    mock.assert_async().await;

    // Anthropic: ceiling present, over-cap 400 backs down to the parsed limit.
    // Unique model name isolates the session-scoped cap cache from other tests.
    let mut server = mockito::Server::new_async().await;
    let first = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 32768 })))
        .with_status(400)
        .with_body(
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 32768 > 4096, which is the maximum allowed number of output tokens for claude-3-opus-20240229"}}"#,
        )
        .expect(1)
        .create_async()
        .await;
    let second = server
        .mock("POST", "/messages")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({ "max_tokens": 4096 })))
        .with_status(200)
        .with_body(anthropic_chat_response("ok", 1, 1))
        .expect(1)
        .create_async()
        .await;
    let config = LlmConfig {
        model_name: "claude-3-opus-20240229".to_string(),
        ..anthropic_config(&server.url())
    };
    let (_c, _t, meta) = client::send_chat_completion(&config, "s", "u").await.unwrap();
    first.assert_async().await;
    second.assert_async().await;
    assert_eq!(meta.max_tokens_backed_down, Some(4096), "{meta:?}");
}
