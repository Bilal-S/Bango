use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::Duration;

use rand::RngExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

// ── OpenAI-compatible types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    total_tokens: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
    /// "stop" = normal completion; "length" = truncated by output token limit.
    /// Checked in `send_openai_compatible` to surface reasoning-model truncation in diagnostics.
    #[serde(default)]
    finish_reason: Option<String>,
}

// ── Google Generative Language API types ─────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSystemInstruction {
    parts: GoogleSystemPart,
}

#[derive(Debug, Serialize)]
struct GoogleSystemPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GoogleContent {
    role: String,
    parts: Vec<GooglePart>,
}

#[derive(Debug, Serialize)]
struct GooglePart {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleRequest {
    system_instruction: GoogleSystemInstruction,
    contents: Vec<GoogleContent>,
    generation_config: GoogleGenerationConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleResponse {
    candidates: Vec<GoogleCandidate>,
    usage_metadata: Option<GoogleUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleUsage {
    total_token_count: usize,
}

#[derive(Debug, Deserialize)]
struct GoogleCandidate {
    content: GoogleContentResponse,
}

#[derive(Debug, Deserialize)]
struct GoogleContentResponse {
    parts: Vec<GooglePartResponse>,
}

#[derive(Debug, Deserialize)]
struct GooglePartResponse {
    text: String,
}

// ── Model listing types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GoogleModelsResponse {
    models: Vec<GoogleModelEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModelEntry {
    name: String,
    supported_generation_methods: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
}

// ── Model filtering ──────────────────────────────────────────────────

/// Determine whether an OpenAI model ID is a chat-completion model.
///
/// Uses a deny-list approach so that new chat models are automatically
/// included without code changes.
fn is_chat_model(id: &str) -> bool {
    let id_lower = id.to_lowercase();

    // Embedding models
    if id_lower.starts_with("text-embedding-") {
        return false;
    }
    // TTS / speech synthesis
    if id_lower.starts_with("tts-") || id_lower.contains("-tts") {
        return false;
    }
    // Image generation
    if id_lower.starts_with("dall-e-")
        || id_lower.starts_with("gpt-image-")
        || id_lower.starts_with("chatgpt-image-")
    {
        return false;
    }
    // Video generation
    if id_lower.starts_with("sora-") {
        return false;
    }
    // Speech / transcription
    if id_lower.starts_with("whisper-") || id_lower.contains("transcribe") {
        return false;
    }
    // Realtime API models
    if id_lower.starts_with("gpt-realtime") || id_lower.contains("realtime-preview") {
        return false;
    }
    // Audio models
    if id_lower.contains("audio-preview") || id_lower.starts_with("gpt-audio") {
        return false;
    }
    // Search-specific endpoints
    if id_lower.contains("search-preview") || id_lower.contains("search-api") {
        return false;
    }
    // Codex / code execution
    if id_lower.contains("codex") {
        return false;
    }
    // Moderation
    if id_lower.starts_with("omni-moderation-") {
        return false;
    }
    // Legacy completion-only models
    if id_lower.starts_with("babbage-")
        || id_lower.starts_with("davinci-")
        || id_lower.contains("-instruct")
    {
        return false;
    }

    true
}

// ── Shared HTTP client + retry ───────────────────────────────────────

const LLM_MAX_RETRIES: u32 = 3;
const LLM_INITIAL_BACKOFF_MS: u64 = 1000;
const LLM_MAX_BACKOFF_MS: u64 = 10_000;

/// Side-channel metadata from [`send_chat_completion`].
///
/// When `temperature_was_rejected` is `true`, the call recovered from a provider
/// temperature-rejection 400 by re-issuing with `temperature` omitted. The
/// orchestrator inspects this to persist `skip_temperature = true`.
///
/// Normal success path returns `CallMeta::default()` (`false`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallMeta {
    /// `true` iff the call recovered from a temperature-rejection 400 by
    /// omitting the `temperature` parameter on retry.
    pub temperature_was_rejected: bool,
}

/// Lazily-built shared HTTP client. Reusing one `reqwest::Client` enables
/// HTTP keep-alive so repeated LLM calls reuse a single TLS session instead of
/// a fresh handshake per request. Matters on Windows (SChannel), where
/// per-request TLS is more failure-prone under concurrency.
///
/// Only connect/pool timeouts are set here; the per-request wall-clock cap is
/// owned by the orchestrator's `tokio::time::timeout`.
pub(crate) fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Normalize text for LLM payload: drop `\r`, coerce NBSP (`\u{00A0}`) to ASCII space.
/// Defense-in-depth hygiene — NBSP enters from PDF extraction, `\r` from Windows-edited
/// text. Fast path returns `Cow::Borrowed` when no change needed.
#[must_use]
fn normalize_llm_text<'a>(input: &'a str) -> Cow<'a, str> {
    if !input.contains('\r') && !input.contains('\u{00A0}') {
        return Cow::Borrowed(input);
    }
    let cleaned: String = input
        .chars()
        .filter(|&c| c != '\r')
        .map(|c| if c == '\u{00A0}' { ' ' } else { c })
        .collect();
    Cow::Owned(cleaned)
}

/// Decide whether a non-success response should be retried.
///
/// Classic transients (429, 408, 5xx) always retry. Additionally, 401/403 with
/// body `"...insufficient permissions for this operation."` retries — this is an
/// empirically-observed OpenAI/Cloudflare project-scope transient on Windows that
/// succeeds on resubmit. Gated on the body string so real auth failures (wrong/
/// revoked key, wrong org) fail fast.
#[must_use]
fn is_retryable_response(status: reqwest::StatusCode, body: &str) -> bool {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
    {
        return true;
    }
    if (status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN)
        && body.contains("insufficient permissions for this operation")
    {
        return true;
    }
    false
}

/// Detect provider rejection of a non-default `temperature` value.
///
/// Models supporting only the default temperature (typically `1`) return HTTP 400
/// mentioning `temperature` + `unsupported`/`does not support`/`not supported`.
///
/// Mirrors the inline check in `test_llm_connection` (`commands/llm_config.rs`);
/// extracted as a pure helper so both paths share one definition.
///
/// Matched: `"Unsupported value: 'temperature' does not support..."` (OpenAI),
/// `"temperature does not support ... not supported"` (Google).
/// Rejected: `"Invalid model"` (no `temperature` token), `"max_tokens is not supported"`
/// (wrong field), `"temperature parameter is invalid"` (out-of-range — retrying
/// without temperature would mask a genuine parameter error).
#[must_use]
pub fn is_temperature_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    if !lower.contains("temperature") {
        return false;
    }
    lower.contains("unsupported")
        || lower.contains("does not support")
        || lower.contains("not supported")
}

/// Exponential backoff: 1s, 2s, 4s (capped at 10s) + 0-500ms jitter.
fn calculate_backoff(attempt: u32) -> u64 {
    if let Some(ms) = test_backoff_override_ms() {
        return ms;
    }
    let base = LLM_INITIAL_BACKOFF_MS * (1u64 << attempt);
    let capped = base.min(LLM_MAX_BACKOFF_MS);
    let mut rng = rand::rng();
    let jitter = rng.random_range(0..=500);
    capped + jitter
}

/*
 * TEST-ONLY override for the retry backoff delay.
 * Debug builds honor `BANGO_TEST_BACKOFF_MS` so retry-path tests (transport
 * errors, 429/5xx mock servers) run in milliseconds instead of sleeping
 * 1+2+4s per call. Release builds are compiled without this branch - the
 * override can never change production retry timing.
 */
fn test_backoff_override_ms() -> Option<u64> {
    #[cfg(debug_assertions)]
    {
        static OVERRIDE: OnceLock<Option<u64>> = OnceLock::new();
        *OVERRIDE.get_or_init(|| {
            std::env::var("BANGO_TEST_BACKOFF_MS").ok().and_then(|v| v.parse().ok())
        })
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// Parse `Retry-After` header (delta-seconds) as milliseconds.
/// Capped at `LLM_MAX_BACKOFF_MS` so a misconfigured server can't stall indefinitely.
fn parse_retry_after_ms(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|secs| (secs * 1000).min(LLM_MAX_BACKOFF_MS))
}

/// Extract OpenAI/Cloudflare trace identifiers (`x-request-id`, `CF-Ray`) for
/// diagnostics. Returns a bracketed annotation (e.g. ` [req=..., cf-ray=...]`)
/// or empty when neither is present.
fn extract_trace_ids(resp: &reqwest::Response) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = resp.headers().get("x-request-id").and_then(|h| h.to_str().ok()) {
        parts.push(format!("req={v}"));
    }
    if let Some(v) = resp.headers().get("cf-ray").and_then(|h| h.to_str().ok()) {
        parts.push(format!("cf-ray={v}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join(", "))
    }
}

/// Send a `RequestBuilder` with bounded retry on transient failures
/// (`is_retryable_response`) and transport errors. Returns raw response body on
/// success. Each retry logs trace IDs so users can confirm the fix is engaging
/// and paste `req_...` / `cf-ray=...` into an OpenAI support ticket.
pub(crate) async fn send_with_retry(
    builder: &reqwest::RequestBuilder,
    label: &str,
) -> Result<String, AppError> {
    /* `RequestBuilder::send` takes ownership but this helper receives `&RequestBuilder`.
    `try_clone()` is `Some` because our builders always carry serializable `.json()`
    bodies. Each retry re-issues an identical request. If a builder can't be cloned,
    fail fast with a clear error. */
    if builder.try_clone().is_none() {
        return Err(AppError::Import(
            "LLM request body is not retryable (non-cloneable RequestBuilder)".to_string(),
        ));
    }

    let mut last_error: Option<String> = None;
    for attempt in 0..=LLM_MAX_RETRIES {
        // `try_clone()` is `Some` (guarded above). `else` returns an error
        // instead of panicking, satisfying the no-`unwrap`/`expect` lint.
        let Some(request_builder) = builder.try_clone() else {
            return Err(AppError::Import(
                "LLM request body is not retryable (non-cloneable RequestBuilder)".to_string(),
            ));
        };
        let response = match request_builder.send().await {
            Ok(r) => r,
            Err(e) => {
                // Transport-level failure (connection reset, TLS handshake
                // error, etc.) - definitionally transient; retry up to the cap.
                if attempt < LLM_MAX_RETRIES {
                    let backoff = calculate_backoff(attempt);
                    eprintln!(
                        "[LlmClient] {label} attempt {}/{} transport error: {e}; retrying in {backoff}ms",
                        attempt + 1,
                        LLM_MAX_RETRIES + 1
                    );
                    last_error = Some(format!("LLM request failed: {e}"));
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    continue;
                }
                return Err(AppError::Import(format!(
                    "LLM request failed after {LLM_MAX_RETRIES} retries: {e}"
                )));
            }
        };

        let status = response.status();
        if status.is_success() {
            return response
                .text()
                .await
                .map_err(|e| AppError::Import(format!("Failed to read LLM response body: {e}")));
        }

        // Non-success: capture trace IDs + Retry-After before consuming the body.
        let trace = extract_trace_ids(&response);
        let retry_after = parse_retry_after_ms(&response);
        let body = response.text().await.unwrap_or_default();

        if attempt < LLM_MAX_RETRIES && is_retryable_response(status, &body) {
            let backoff = retry_after.unwrap_or_else(|| calculate_backoff(attempt));
            eprintln!(
                "[LlmClient] {label} attempt {}/{} failed ({status}){trace}; retrying in {backoff}ms",
                attempt + 1,
                LLM_MAX_RETRIES + 1
            );
            last_error = Some(format!("LLM request failed ({status}){trace}: {body}"));
            tokio::time::sleep(Duration::from_millis(backoff)).await;
            continue;
        }

        return Err(AppError::Import(format!("LLM request failed ({status}){trace}: {body}")));
    }
    Err(AppError::Import(
        last_error.unwrap_or_else(|| format!("LLM request failed after {LLM_MAX_RETRIES} retries")),
    ))
}

// ── Public API ───────────────────────────────────────────────────────

pub async fn list_models(
    provider: &LlmProvider,
    endpoint_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let client = Client::new();
    let base_url = endpoint_url.trim_end_matches('/').trim_end_matches("/models");

    match provider {
        LlmProvider::Google => {
            let url = format!("{base_url}/models");
            let key = api_key
                .ok_or_else(|| AppError::Import("API key required for Google".to_string()))?;
            let resp = client
                .get(&url)
                .header("Content-Type", "application/json")
                .header("X-goog-api-key", key)
                .send()
                .await
                .map_err(|e| AppError::Import(format!("Failed to fetch models: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Import(format!("Failed to fetch models ({status}): {body}")));
            }

            let models: GoogleModelsResponse = resp
                .json()
                .await
                .map_err(|e| AppError::Import(format!("Failed to parse models response: {e}")))?;

            let ids: Vec<String> = models
                .models
                .into_iter()
                .filter(|m| {
                    m.supported_generation_methods
                        .as_ref()
                        .is_none_or(|methods| methods.contains(&"generateContent".to_string()))
                })
                .map(|m| m.name.strip_prefix("models/").map(|s| s.to_string()).unwrap_or(m.name))
                .collect();
            Ok(ids)
        }
        LlmProvider::Anthropic => {
            let url = format!("{base_url}/models");
            let key = api_key
                .ok_or_else(|| AppError::Import("API key required for Anthropic".to_string()))?;
            let resp = client
                .get(&url)
                .header("Content-Type", "application/json")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| AppError::Import(format!("Failed to fetch models: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Import(format!("Failed to fetch models ({status}): {body}")));
            }

            let models: OpenAiModelsResponse = resp
                .json()
                .await
                .map_err(|e| AppError::Import(format!("Failed to parse models response: {e}")))?;
            Ok(models.data.into_iter().map(|m| m.id).collect())
        }
        _ => {
            // OpenAI-compatible: OpenAI, Mistral, z_ai, Ollama, LM Studio, llama.cpp, Custom
            let url = format!("{base_url}/models");
            let mut req = client.get(&url).header("Content-Type", "application/json");
            if let Some(key) = api_key {
                if !key.is_empty() {
                    req = req.bearer_auth(key);
                }
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AppError::Import(format!("Failed to fetch models: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Import(format!("Failed to fetch models ({status}): {body}")));
            }

            let models: OpenAiModelsResponse = resp
                .json()
                .await
                .map_err(|e| AppError::Import(format!("Failed to parse models response: {e}")))?;

            let should_filter = matches!(provider, LlmProvider::Openai);
            let mut ids: Vec<String> = models
                .data
                .into_iter()
                .map(|m| m.id)
                .filter(|id| !should_filter || is_chat_model(id))
                .collect();
            if should_filter {
                ids.sort();
            }
            Ok(ids)
        }
    }
}

/// Send a chat completion request. Returns `(response_text, token_total, CallMeta)`.
///
/// Single entry point used by `LlmOrchestrator::send`. The orchestrator inspects
/// `CallMeta.temperature_was_rejected` to persist `skip_temperature = true`.
///
/// Temperature-rejection recovery runs INSIDE this function (one extra
/// `send_with_retry` call), bounded by the orchestrator's single outer
/// `tokio::time::timeout` — no doubling of the timeout budget.
pub async fn send_chat_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, usize, CallMeta), AppError> {
    match config.provider {
        LlmProvider::Google => send_google(config, system_prompt, user_prompt).await,
        _ => send_openai_compatible(config, system_prompt, user_prompt).await,
    }
}

// ── Google path ──────────────────────────────────────────────────────

async fn send_google(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, usize, CallMeta), AppError> {
    let system_prompt = normalize_llm_text(system_prompt);
    let user_prompt = normalize_llm_text(user_prompt);
    let client = shared_client();

    // Owned `String` so the retry closure (`Fn`, up to 2 calls) can clone it
    // into each attempt without moving out of the captured environment.
    let api_key = config
        .api_key_encrypted
        .clone()
        .ok_or_else(|| AppError::Import("API key required for Google".to_string()))?;

    let base_url = config.endpoint_url.trim_end_matches('/');
    let endpoint = if base_url.contains(":generateContent") {
        base_url.to_string()
    } else {
        format!("{}/models/{}:generateContent", base_url, config.model_name)
    };

    /* Convert normalized prompts to owned `String`s so the retry closure
    (an `Fn`, up to 2 calls) can clone them cheaply into each `async move`
    block without moving out of captured environment. */
    let system_text = system_prompt.into_owned();
    let user_text = user_prompt.into_owned();

    /* Build + send, then recover from temperature-rejection 400 by retrying with
    `temp = None`. When `config.skip_temperature` is already `true`, the first
    attempt omits temperature and there's nothing to recover from. */
    send_with_temperature_recovery(config.skip_temperature, config.temperature, move |temp| {
        // Clone per-call: the outer closure is `Fn` (up to 2 calls), so each
        // invocation must produce its own owned prompt strings.
        let system_text = system_text.clone();
        let user_text = user_text.clone();
        let api_key = api_key.clone();
        let endpoint = endpoint.clone();
        async move {
            let request = GoogleRequest {
                system_instruction: GoogleSystemInstruction {
                    parts: GoogleSystemPart { text: system_text },
                },
                contents: vec![GoogleContent {
                    role: "user".to_string(),
                    parts: vec![GooglePart { text: user_text }],
                }],
                generation_config: GoogleGenerationConfig { temperature: temp },
            };
            let builder = client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .header("X-goog-api-key", api_key)
                .json(&request);
            let body_text = send_with_retry(&builder, "Google").await?;
            let google_response: GoogleResponse = serde_json::from_str(&body_text)
                .map_err(|e| AppError::Import(format!("Failed to parse LLM response: {e}")))?;
            let content = google_response
                .candidates
                .first()
                .and_then(|c| c.content.parts.first())
                .map(|p| p.text.clone())
                .ok_or_else(|| AppError::Import("No response from LLM".to_string()))?;
            let total_tokens =
                google_response.usage_metadata.map(|u| u.total_token_count).unwrap_or(0);
            Ok((content, total_tokens))
        }
    })
    .await
}

// ── OpenAI-compatible path ───────────────────────────────────────────

async fn send_openai_compatible(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, usize, CallMeta), AppError> {
    let system_prompt = normalize_llm_text(system_prompt);
    let user_prompt = normalize_llm_text(user_prompt);
    let client = shared_client();
    /* `max_tokens` is intentionally NOT sent. Some newer OpenAI-compatible models
    (e.g. o-series reasoning) reject it with 400 and need `max_completion_tokens`
    instead. Sending neither is provider-portable: the server applies its own
    output budget. The summary markdown-fallback retry handles the empty-content
    failure mode reasoning models can produce. */
    let api_key = config.api_key_encrypted.as_deref().unwrap_or("").to_string();

    let base_url = config.endpoint_url.trim_end_matches('/');
    let endpoint = match config.provider {
        LlmProvider::Openai
        | LlmProvider::LlamaCpp
        | LlmProvider::Ollama
        | LlmProvider::LmStudio
        | LlmProvider::MistralAi
        | LlmProvider::ZAi
        | LlmProvider::Custom => {
            if base_url.ends_with("/chat/completions") {
                base_url.to_string()
            } else {
                format!("{base_url}/chat/completions")
            }
        }
        LlmProvider::Anthropic => {
            if base_url.ends_with("/messages") {
                base_url.to_string()
            } else {
                format!("{base_url}/messages")
            }
        }
        _ => base_url.to_string(),
    };

    let model_name = config.model_name.clone();
    let system_text = system_prompt.to_string();
    let user_text = user_prompt.to_string();

    /* Build + send, then recover from temperature-rejection 400. Same envelope
    as `send_google`; the closure captures the OpenAI-compatible request shape. */
    send_with_temperature_recovery(config.skip_temperature, config.temperature, move |temp| {
        // Clone per-call: the outer closure is `Fn` (up to 2 calls), so each
        // invocation must produce its own owned prompt + api_key strings.
        let model_name = model_name.clone();
        let system_text = system_text.clone();
        let user_text = user_text.clone();
        let api_key = api_key.clone();
        let endpoint = endpoint.clone();
        async move {
            let request = ChatRequest {
                model: model_name,
                messages: vec![
                    ChatMessage { role: "system".to_string(), content: system_text },
                    ChatMessage { role: "user".to_string(), content: user_text },
                ],
                temperature: temp,
            };
            let builder = client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .bearer_auth(&api_key)
                .json(&request);
            /* `send_with_retry` owns transient retry (429/408/5xx + transport +
            OpenAI "insufficient permissions" 401/403 transient) and captures
            `x-request-id` / `CF-Ray` into the error string for diagnostics. */
            let body_text = send_with_retry(&builder, "OpenAI-compatible").await?;

            // Strategy 1: Try standard ChatResponse (OpenAI format)
            if let Ok(chat_response) = serde_json::from_str::<ChatResponse>(&body_text) {
                let choice = chat_response
                    .choices
                    .into_iter()
                    .next()
                    .ok_or_else(|| AppError::Import("No response from LLM".to_string()))?;
                /* Surface reasoning-model truncation: "length" means the server hit
                its output-token budget before the model finished. */
                if choice.finish_reason.as_deref() == Some("length") {
                    eprintln!(
                    "[LlmClient] response truncated by output-token limit (finish_reason=length); \
                     content may be incomplete"
                );
                }
                let total_tokens = chat_response.usage.and_then(|u| u.total_tokens).unwrap_or(0);
                return Ok((choice.message.content, total_tokens));
            }

            // Strategy 2: Fallback - extract content from arbitrary JSON envelope
            let value: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
                AppError::Import(format!("Failed to parse LLM response as JSON: {e}"))
            })?;

            let content = extract_content_from_response(&value).ok_or_else(|| {
                AppError::Import(format!(
                    "Could not extract content from LLM response. Raw body (first 500 chars): {}",
                    &body_text[..body_text.len().min(500)]
                ))
            })?;

            let total_tokens = extract_total_tokens(&value);
            Ok((content, total_tokens))
        }
    })
    .await
}

/// Run `make_request(temp)` once. On temperature-rejection 400, retry with
/// `temp = None`, returning `temperature_was_rejected = true` in [`CallMeta`].
///
/// Skipped when `skip_temperature` is already `true` (first attempt already omits
/// `temperature`). On second-attempt failure, the ORIGINAL first-attempt error is
/// returned (the actionable temperature message, not a fluky second error).
///
/// Recovery happens INSIDE this function, sharing the orchestrator's single outer
/// `tokio::time::timeout` envelope.
async fn send_with_temperature_recovery<F, Fut>(
    skip_temperature: bool,
    temperature: f64,
    make_request: F,
) -> Result<(String, usize, CallMeta), AppError>
where
    F: Fn(Option<f64>) -> Fut,
    Fut: std::future::Future<Output = Result<(String, usize), AppError>>,
{
    let first_temp = if skip_temperature { None } else { Some(temperature) };

    match make_request(first_temp).await {
        Ok(tuple) => Ok((tuple.0, tuple.1, CallMeta::default())),
        Err(e) => {
            /* Only retry if temperature was actually sent. If skip_temperature
            was already true, surface the error immediately. */
            if skip_temperature || !is_temperature_error(&format!("{e}")) {
                return Err(e);
            }
            eprintln!(
                "[LlmClient] temperature rejected by model; retrying without temperature parameter"
            );
            match make_request(None).await {
                Ok(tuple) => Ok((tuple.0, tuple.1, CallMeta { temperature_was_rejected: true })),
                // Surface the ORIGINAL (temperature) error: it carries the
                // actionable diagnostic. The second failure is likely unrelated.
                Err(_) => Err(e),
            }
        }
    }
}

/// Extract text content from an arbitrary LLM response JSON.
///
/// Handles: standard OpenAI `choices[0].message.content`, z.ai `message.content`
/// (string or array of text objects), and any provider with a `content` key at
/// the first two levels.
fn extract_content_from_response(value: &serde_json::Value) -> Option<String> {
    // Level 0: is value itself a string?
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    // Level 1: standard OpenAI path
    if let Some(s) = value["choices"][0]["message"]["content"].as_str() {
        return Some(s.to_string());
    }

    // Scan all top-level keys for a "content" field
    if let Some(obj) = value.as_object() {
        for (_, v) in obj {
            // Direct string content
            if let Some(s) = v["content"].as_str() {
                return Some(s.to_string());
            }
            // Array content (e.g., z.ai returns content as array of objects)
            if let Some(arr) = v["content"].as_array() {
                let collected: String = arr
                    .iter()
                    .filter_map(|item| {
                        // Objects with "text" field; skip objects with "reasoning" field
                        item["text"]
                            .as_str()
                            .map(String::from)
                            .or_else(|| item.as_str().map(String::from))
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !collected.is_empty() {
                    return Some(collected);
                }
            }

            // Level 2: check nested object keys for content
            if let Some(nested) = v.as_object() {
                for (_, inner) in nested {
                    if let Some(s) = inner["content"].as_str() {
                        return Some(s.to_string());
                    }
                    if let Some(arr) = inner["content"].as_array() {
                        let collected: String = arr
                            .iter()
                            .filter_map(|item| {
                                item["text"]
                                    .as_str()
                                    .map(String::from)
                                    .or_else(|| item.as_str().map(String::from))
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        if !collected.is_empty() {
                            return Some(collected);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Try to extract total_tokens from an arbitrary response JSON.
fn extract_total_tokens(value: &serde_json::Value) -> usize {
    // Standard OpenAI path
    value["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    // ── normalize_llm_text ───────────────────────────────────────────

    #[test]
    fn normalize_llm_text_identity_fast_path_returns_borrowed() {
        // Clean input must return Cow::Borrowed (no allocation).
        match normalize_llm_text("clean ascii text with\nnewlines and\ttabs") {
            Cow::Borrowed(s) => assert_eq!(s, "clean ascii text with\nnewlines and\ttabs"),
            Cow::Owned(_) => panic!("expected Cow::Borrowed for clean input"),
        }
    }

    #[test]
    fn normalize_llm_text_strips_carriage_returns() {
        // `.as_ref()` compares the deref'd &str value, sidestepping the Cow
        // type-parameter inference that constructing Cow::Owned on the RHS triggers.
        assert_eq!(normalize_llm_text("line1\r\nline2\rmore").as_ref(), "line1\nline2more");
        // Lone \r also dropped.
        assert_eq!(normalize_llm_text("a\rb").as_ref(), "ab");
    }

    #[test]
    fn normalize_llm_text_coerces_nbsp_to_ascii_space() {
        let input = "word\u{00A0}word";
        assert_eq!(normalize_llm_text(input).as_ref(), "word word");
    }

    #[test]
    fn normalize_llm_text_handles_mixed_crlf_and_nbsp() {
        let input = "title\r\nbody\u{00A0}with nbsp\r";
        assert_eq!(normalize_llm_text(input).as_ref(), "title\nbody with nbsp");
    }

    #[test]
    fn normalize_llm_text_preserves_unicode() {
        // Non-NBSP unicode (CJK, accented) passes through unchanged.
        let input = "日本語 résumé café";
        match normalize_llm_text(input) {
            Cow::Borrowed(s) => assert_eq!(s, "日本語 résumé café"),
            Cow::Owned(_) => panic!("expected borrowed for non-NBSP unicode"),
        }
    }

    // ── is_retryable_response ────────────────────────────────────────

    #[test]
    fn is_retryable_classic_transient_statuses_retry() {
        assert!(is_retryable_response(StatusCode::TOO_MANY_REQUESTS, ""));
        assert!(is_retryable_response(StatusCode::REQUEST_TIMEOUT, ""));
        assert!(is_retryable_response(StatusCode::INTERNAL_SERVER_ERROR, ""));
        assert!(is_retryable_response(StatusCode::BAD_GATEWAY, ""));
        assert!(is_retryable_response(StatusCode::SERVICE_UNAVAILABLE, ""));
        assert!(is_retryable_response(StatusCode::GATEWAY_TIMEOUT, ""));
    }

    #[test]
    fn is_retryable_permanent_client_errors_do_not_retry() {
        assert!(!is_retryable_response(StatusCode::BAD_REQUEST, "malformed json"));
        assert!(!is_retryable_response(StatusCode::NOT_FOUND, "model not found"));
        // Plain 401/403 without the specific transient body must NOT retry
        // (these are real auth failures: wrong/revoked key, wrong org).
        assert!(!is_retryable_response(StatusCode::UNAUTHORIZED, "Incorrect API key provided"));
        assert!(!is_retryable_response(StatusCode::FORBIDDEN, "access denied"));
    }

    #[test]
    fn is_retryable_insufficient_permissions_body_retries_on_401_and_403() {
        let body = r#"{"error":{"message":"You have insufficient permissions for this operation.","type":"invalid_request_error","param":null,"code":null}}"#;
        assert!(is_retryable_response(StatusCode::UNAUTHORIZED, body));
        assert!(is_retryable_response(StatusCode::FORBIDDEN, body));
    }

    #[test]
    fn is_retryable_insufficient_permissions_requires_both_status_and_body() {
        // The body alone does not make a 400 retryable.
        let body = "insufficient permissions for this operation";
        assert!(!is_retryable_response(StatusCode::BAD_REQUEST, body));
        // A similar but non-matching body on 403 does not retry.
        assert!(!is_retryable_response(StatusCode::FORBIDDEN, "insufficient permission"));
    }

    // ── calculate_backoff ────────────────────────────────────────────

    #[test]
    fn calculate_backoff_stays_within_jitter_band() {
        for attempt in 0..LLM_MAX_RETRIES {
            let base = (LLM_INITIAL_BACKOFF_MS * (1u64 << attempt)).min(LLM_MAX_BACKOFF_MS);
            for _ in 0..50 {
                let backoff = calculate_backoff(attempt);
                assert!(
                    (base..=base + 500).contains(&backoff),
                    "attempt {attempt}: backoff {backoff} outside [{base}, {}]",
                    base + 500
                );
            }
        }
    }

    #[test]
    fn calculate_backoff_caps_at_max_backoff() {
        // Large attempt must still be capped at LLM_MAX_BACKOFF_MS + jitter.
        let backoff = calculate_backoff(20);
        assert!((LLM_MAX_BACKOFF_MS..=LLM_MAX_BACKOFF_MS + 500).contains(&backoff));
    }

    // ── is_temperature_error ─────────────────────────────────────────

    #[test]
    fn is_temperature_error_matches_openai_unsupported_value_body() {
        // The exact body from the bug report.
        let msg = "LLM request failed (400 Bad Request): {\"error\":{\"message\":\
                   \"Unsupported value: 'temperature' does not support 0.2 with this model. \
                   Only the default (1) value is supported.\",\"type\":\"invalid_request_error\",\
                   \"param\":\"temperature\",\"code\":\"unsupported_value\"}}";
        assert!(is_temperature_error(msg));
    }

    #[test]
    fn is_temperature_error_matches_google_does_not_support_body() {
        let msg = "LLM request failed (400 Bad Request): temperature does not support 0.2 with \
                   this model. Only the default is supported.";
        assert!(is_temperature_error(msg));
    }

    #[test]
    fn is_temperature_error_matches_not_supported_phrasing() {
        assert!(is_temperature_error("Invalid temperature: not supported by this model"));
        assert!(is_temperature_error("temperature not supported"));
    }

    #[test]
    fn is_temperature_error_rejects_non_temperature_errors() {
        // No `temperature` token => never a temperature error.
        assert!(!is_temperature_error("Invalid model"));
        assert!(!is_temperature_error("max_tokens is not supported"));
        assert!(!is_temperature_error("model not found"));
        assert!(!is_temperature_error(""));
    }

    #[test]
    fn is_temperature_error_rejects_temperature_without_unsupported_marker() {
        // Mentions temperature but not as an unsupported-value error.
        assert!(!is_temperature_error("temperature set to 0.2"));
        assert!(!is_temperature_error("warning: temperature is low"));
        // Out-of-range / invalid-value errors must NOT trigger retry-without-
        // temperature: doing so would mask a genuine parameter error. These
        // are distinct from "unsupported feature" errors (which the helper
        // does match).
        assert!(!is_temperature_error("temperature parameter is invalid"));
        assert!(!is_temperature_error("Invalid temperature value"));
    }
}
