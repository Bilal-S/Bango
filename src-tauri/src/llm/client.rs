use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<ChatTemplateKwargs>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ChatTemplateKwargs {
    enable_thinking: bool,
}

/// Per-request wire options. Only the local Bango AI provider consumes them;
/// cloud providers never receive these fields (`skip_temperature` is the
/// field-rejection precedent).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestOptions {
    /// Send `response_format: json_object` (llama-server grammar-backed).
    pub json_mode: bool,
    /// `Some(false)` sends `chat_template_kwargs.enable_thinking = false`;
    /// `None` leaves the server default (thinking on).
    pub enable_thinking: Option<bool>,
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
#[serde(rename_all = "camelCase")]
struct GoogleCandidate {
    content: GoogleContentResponse,
    /// `"STOP"` = natural stop; `"MAX_TOKENS"` = truncated by output budget.
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleContentResponse {
    parts: Vec<GooglePartResponse>,
}

#[derive(Debug, Deserialize)]
struct GooglePartResponse {
    text: String,
}

// ── Anthropic Messages API types ─────────────────────────────────────

/// Required on EVERY Anthropic API request (Messages + Models). `2023-06-01`
/// is the latest and only non-deprecated API version.
const ANTHROPIC_VERSION_HEADER: &str = "2023-06-01";

/// Default output budget REQUESTED from the Messages API (`max_tokens` is a
/// required field and cannot be omitted). Current Claude models cap output at
/// 64K-128K tokens, so 32_768 leaves generous headroom for long outputs
/// (summaries, wiki, translation) while bounding worst-case runaway cost.
/// `max_tokens` is a ceiling, not a target: short answers stop naturally and
/// are billed only for tokens actually generated, so the high default never
/// lengthens or surcharges them.
const ANTHROPIC_REQUESTED_MAX_TOKENS: i64 = 32_768;

/// Universal-safe fallback output budget: no Claude model rejects 4096 (the
/// tightest historical cap, claude-3-era models). Used when an over-cap 400
/// body does not carry a parseable model limit (proxy wording variance).
const ANTHROPIC_SAFE_MAX_TOKENS: i64 = 4096;

/// Marker sentence Anthropic includes in the 400 body when `max_tokens`
/// exceeds the model's output limit. Example message:
/// `max_tokens: 20000 > 4096, which is the maximum allowed number of output
/// tokens for claude-3-opus-20240229`
const ANTHROPIC_OVER_CAP_MARKER: &str = "which is the maximum allowed number of output tokens";

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: i64,
    messages: Vec<AnthropicMessage>,
    /// Top-level system prompt. The Messages API has NO "system" role -
    /// a system-role message inside `messages` is rejected with 400.
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    /// Content blocks; only `type: "text"` blocks carry a `text` field
    /// (e.g. `tool_use` blocks do not).
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    usage: Option<AnthropicUsage>,
    /// `"max_tokens"` = output truncated by the budget; `"end_turn"` = natural
    /// stop. Surfaced as a diagnostic log (parity with the OpenAI path's
    /// `finish_reason == "length"` handling).
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: usize,
    #[serde(default)]
    output_tokens: usize,
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

pub const LLM_MAX_RETRIES: u32 = 3;
pub const LLM_INITIAL_BACKOFF_MS: u64 = 1000;
pub const LLM_MAX_BACKOFF_MS: u64 = 10_000;

/// Side-channel metadata from [`send_chat_completion`].
///
/// When `temperature_was_rejected` is `true`, the call recovered from a provider
/// temperature-rejection 400 by re-issuing with `temperature` omitted. The
/// orchestrator inspects this to persist `skip_temperature = true`.
///
/// When `max_tokens_backed_down` is `Some(n)`, the Anthropic path recovered
/// from an over-cap `max_tokens` 400 by re-issuing with the model-reported
/// limit `n` (or the universal-safe fallback). Test Connection surfaces this
/// so users understand the adjusted budget.
///
/// Normal success path returns `CallMeta::default()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallMeta {
    /// `true` iff the call recovered from a temperature-rejection 400 by
    /// omitting the `temperature` parameter on retry.
    pub temperature_was_rejected: bool,
    /// `Some(n)` iff the call recovered from an over-cap `max_tokens` 400 by
    /// backing down to the model-reported limit `n`.
    pub max_tokens_backed_down: Option<i64>,
    /// Parsed stop/finish reason: `finish_reason` (OpenAI), `stop_reason`
    /// (Anthropic), `finishReason` (Google). `None` when the provider omitted
    /// it or the response shape had none.
    pub finish_reason: Option<String>,
}

impl CallMeta {
    /// `true` iff the provider stopped generation at the output-token budget
    /// (`finish_reason = "length"`, `stop_reason = "max_tokens"`, Google
    /// `finishReason = "MAX_TOKENS"`). Consumed by the wiki ingest
    /// truncation-detection + continuation machinery (wikifix-final Change 4).
    #[must_use]
    pub fn truncated_by_output_budget(&self) -> bool {
        self.finish_reason
            .as_deref()
            .is_some_and(|r| matches!(r.to_ascii_lowercase().as_str(), "length" | "max_tokens"))
    }
}

/// Planning-only estimate of the provider's effective output budget (tokens).
/// NOTHING here is sent to the provider (user ruling: no generic output-cap
/// restriction; the OpenAI-compatible and Google paths send no output field).
/// Used solely for two-sided wiki batch sizing; the truncation detection +
/// bounded continuation machinery covers mis-estimates.
#[must_use]
pub fn estimated_output_budget_tokens(config: &LlmConfig) -> usize {
    let model = config.model_name.to_ascii_lowercase();
    let name = model.as_str();
    if name.contains("gpt-5")
        || name.contains("gpt-4.1")
        || name.contains("o3")
        || name.contains("o4")
    {
        return 128_000;
    }
    if name.contains("claude-3-5-haiku") {
        return 8_192;
    }
    if name.contains("claude-3") {
        return 4_096;
    }
    if name.contains("claude") {
        // Honest planning number: the requested ceiling, not the model max
        // (ANTHROPIC_REQUESTED_MAX_TOKENS; back-down can only lower it).
        return 32_768;
    }
    if name.contains("gemini-1.5-flash") || name.contains("gemini-2.0") {
        return 8_192;
    }
    if name.contains("gemini") {
        return 65_536;
    }
    match config.provider {
        // Local endpoints usually default to small output budgets; Bango AI
        // runs a 9B CPU model and is included with the local providers.
        LlmProvider::Ollama
        | LlmProvider::LlamaCpp
        | LlmProvider::LmStudio
        | LlmProvider::BangoAi => 8_192,
        // Hosted OpenAI-compatible providers default to the model max, which
        // is >= 32K for current-generation models; 32K is the conservative
        // planning number.
        _ => 32_768,
    }
}

/// Session-scoped cache of per-model discovered Anthropic output caps.
/// Populated by the over-cap back-down path in `send_anthropic`; keyed by
/// model name so a mid-session model switch re-probes instead of reusing a
/// stale cap. Poisoned-lock failures degrade to the uncached default budget
/// (never crash, mirroring `shared_client`'s degrade philosophy).
static ANTHROPIC_CAP_CACHE: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

/// Look up the latched output cap for a model, if a previous call already
/// probed it this session.
fn anthropic_cached_cap(model: &str) -> Option<i64> {
    let cache = ANTHROPIC_CAP_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().ok()?.get(model).copied()
}

/// Latch a discovered output cap for a model (best-effort; lock failure is a
/// no-op, the next call simply re-probes).
fn anthropic_latch_cap(model: &str, cap: i64) {
    let cache = ANTHROPIC_CAP_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = cache.lock() {
        map.insert(model.to_string(), cap);
    }
}

/// `true` iff an LLM error string is an Anthropic over-cap `max_tokens` 400.
/// Two-marker gate (`max_tokens` + the marker sentence from
/// [`ANTHROPIC_OVER_CAP_MARKER`]) so unrelated 400s (temperature, body shape,
/// auth) never trigger a back-down retry.
#[must_use]
pub fn is_over_cap_error(msg: &str) -> bool {
    msg.contains("max_tokens") && msg.contains(ANTHROPIC_OVER_CAP_MARKER)
}

/// Parse the model-reported output limit out of an over-cap error message:
/// the number after the `>` in `max_tokens: 32768 > 4096, ...`. Returns
/// `None` when no positive limit can be extracted (proxy wording variance);
/// callers then fall back to [`ANTHROPIC_SAFE_MAX_TOKENS`].
#[must_use]
pub fn parse_model_cap(msg: &str) -> Option<i64> {
    let start = msg.find("max_tokens:")?;
    let gt = msg[start..].find('>')? + start;
    let tail = msg[gt + 1..].trim_start();
    let end = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
    let cap: i64 = tail[..end].parse().ok()?;
    (cap > 0).then_some(cap)
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
pub fn normalize_llm_text<'a>(input: &'a str) -> Cow<'a, str> {
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
pub fn is_retryable_response(status: reqwest::StatusCode, body: &str) -> bool {
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
pub fn calculate_backoff(attempt: u32) -> u64 {
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

/// Char-boundary-safe truncation (byte-slicing a multi-byte UTF-8 body panics;
/// aifixes1 F6). Returns the leading `max_chars` characters.
#[must_use]
pub fn truncate_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// Parse `Retry-After` header (delta-seconds) as milliseconds.
/// Capped at `LLM_MAX_BACKOFF_MS` so a misconfigured server can't stall indefinitely.
fn parse_retry_after_ms(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(retry_after_ms_from_secs)
}

/// Saturating seconds-to-milliseconds conversion (a hostile `Retry-After`
/// near `u64::MAX` must not overflow; aifixes1 F6).
pub fn retry_after_ms_from_secs(secs: u64) -> u64 {
    secs.saturating_mul(1000).min(LLM_MAX_BACKOFF_MS)
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
    send_chat_completion_with_options(config, system_prompt, user_prompt, RequestOptions::default())
        .await
}

/// Option-carrying variant used by the orchestrator (local engine options:
/// JSON intent and thinking control).
pub async fn send_chat_completion_with_options(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
    options: RequestOptions,
) -> Result<(String, usize, CallMeta), AppError> {
    match config.provider {
        LlmProvider::Google => send_google(config, system_prompt, user_prompt).await,
        LlmProvider::Anthropic => send_anthropic(config, system_prompt, user_prompt).await,
        _ => {
            send_openai_compatible(config, system_prompt, user_prompt, options).await.map_err(|e| {
                if config.provider == LlmProvider::BangoAi {
                    map_bango_ai_error(e)
                } else {
                    e
                }
            })
        }
    }
}

/// Whether an error message is a llama-server context-overflow 400
/// (aifixes1 F3): wording varies across builds, so match the stable fragments.
fn is_local_context_overflow(message: &str) -> bool {
    message.contains("(400")
        && (message.contains("exceeds the available context size")
            || message.contains("context size")
            || message.contains("n_ctx"))
}

/// Map a Bango AI failure to an actionable message where possible; everything
/// else passes through untouched (aifixes1 F3: no raw `400` for local overflow).
pub fn map_bango_ai_error(err: AppError) -> AppError {
    let message = err.to_string();
    if is_local_context_overflow(&message) {
        return AppError::Import(
            "Bango AI could not fit this prompt in the selected context. Reduce the Context \
             setting or shorten the text."
                .to_string(),
        );
    }
    err
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
            let candidate = google_response.candidates.first();
            let finish_reason = candidate.and_then(|c| c.finish_reason.clone());
            let content = candidate
                .and_then(|c| c.content.parts.first())
                .map(|p| p.text.clone())
                .ok_or_else(|| AppError::Import("No response from LLM".to_string()))?;
            let total_tokens =
                google_response.usage_metadata.map(|u| u.total_token_count).unwrap_or(0);
            Ok((content, total_tokens, finish_reason))
        }
    })
    .await
}

// ── Anthropic path (native Messages API) ─────────────────────────────

/// Native Anthropic Messages API path (`POST {base}/messages`).
///
/// Anthropic is NOT OpenAI-compatible, so this builder mirrors `send_google`
/// with a provider-native shape:
/// - Every request must carry the `anthropic-version` header; omitting it is
///   rejected with 400 `anthropic-version: header is required`.
/// - Auth uses `x-api-key` (Bearer is also accepted by the API; this matches
///   the `list_models` path).
/// - `max_tokens` is a required body field; the system prompt is a TOP-LEVEL
///   `system` field because the Messages API has no "system" role.
///
/// Output-cap capability probe: the first request for a model asks for
/// [`ANTHROPIC_REQUESTED_MAX_TOKENS`]. Models with a lower cap reject it with
/// a 400 whose message states the model's true limit; the path then backs
/// down to the parsed limit ([`ANTHROPIC_SAFE_MAX_TOKENS`] when unparseable),
/// latches it per model for the rest of the session, and retries once. The
/// probe therefore costs a single failed request once per model per session.
async fn send_anthropic(
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
        .ok_or_else(|| AppError::Import("API key required for Anthropic".to_string()))?;

    let base_url = config.endpoint_url.trim_end_matches('/');
    let endpoint = if base_url.ends_with("/messages") {
        base_url.to_string()
    } else {
        format!("{base_url}/messages")
    };

    let model_name = config.model_name.clone();
    /* Owned `String`s so the retry envelopes (`Fn`, up to 2 calls each) can
    clone them cheaply into each `async move` block without moving out of the
    captured environment. */
    let system_text = system_prompt.into_owned();
    let user_text = user_prompt.into_owned();

    /* First budget: the latched per-model cap when a previous call already
    probed this model this session, otherwise the generous default request. */
    let initial_budget =
        anthropic_cached_cap(&model_name).unwrap_or(ANTHROPIC_REQUESTED_MAX_TOKENS);

    match anthropic_attempt(
        client,
        endpoint.clone(),
        api_key.clone(),
        model_name.clone(),
        system_text.clone(),
        user_text.clone(),
        initial_budget,
        config.skip_temperature,
        config.temperature,
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(e) => {
            let err_text = format!("{e}");
            /* Only back down on a genuine over-cap 400, and never when the
            budget already sits at the safe floor (a 4096 rejection means a
            broken proxy: fail loudly instead of spinning). */
            if !is_over_cap_error(&err_text) || initial_budget <= ANTHROPIC_SAFE_MAX_TOKENS {
                return Err(e);
            }
            /* The 400 message states the model's true limit
            (`max_tokens: 32768 > 4096, which is the maximum allowed ...`).
            Parse it, fall back to the universal-safe floor when unparseable,
            and latch per model so later calls skip the probe entirely. */
            let backed_down = parse_model_cap(&err_text)
                .map_or(ANTHROPIC_SAFE_MAX_TOKENS, |cap| cap.min(initial_budget));
            anthropic_latch_cap(&config.model_name, backed_down);
            eprintln!(
                "[LlmClient] Anthropic max_tokens {} above model cap; backing down to {backed_down} for this session",
                initial_budget
            );
            /* Exactly one back-down per call: a second over-cap surfaces as-is
            (loop guard). Merge CallMeta so a temperature recovery inside the
            second envelope is still visible to the orchestrator. */
            let (content, total_tokens, meta) = anthropic_attempt(
                client,
                endpoint,
                api_key,
                model_name,
                system_text,
                user_text,
                backed_down,
                config.skip_temperature,
                config.temperature,
            )
            .await?;
            Ok((
                content,
                total_tokens,
                CallMeta {
                    temperature_was_rejected: meta.temperature_was_rejected,
                    max_tokens_backed_down: Some(backed_down),
                    finish_reason: meta.finish_reason,
                },
            ))
        }
    }
}

/// One full Anthropic send envelope at a given output `budget`: build + send,
/// then recover from temperature-rejection 400 by retrying with `temp = None`
/// (same envelope as `send_google`; the closure captures the Anthropic-native
/// request shape). Split out of `send_anthropic` so the over-cap back-down can
/// re-run the whole envelope with a lower budget.
//
// Nine parameters keep every input owned/copyable; bundling them into a struct
// would add ceremony without reuse elsewhere.
#[allow(clippy::too_many_arguments)]
async fn anthropic_attempt(
    client: &'static reqwest::Client,
    endpoint: String,
    api_key: String,
    model_name: String,
    system_text: String,
    user_text: String,
    budget: i64,
    skip_temperature: bool,
    temperature: f64,
) -> Result<(String, usize, CallMeta), AppError> {
    send_with_temperature_recovery(skip_temperature, temperature, move |temp| {
        // Clone per-call: the outer closure is `Fn` (up to 2 calls), so each
        // invocation must produce its own owned strings.
        let model_name = model_name.clone();
        let system_text = system_text.clone();
        let user_text = user_text.clone();
        let api_key = api_key.clone();
        let endpoint = endpoint.clone();
        async move {
            let request = AnthropicRequest {
                model: model_name,
                max_tokens: budget,
                messages: vec![AnthropicMessage { role: "user", content: user_text }],
                system: if system_text.is_empty() { None } else { Some(system_text) },
                temperature: temp,
            };
            let builder = client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .header("x-api-key", api_key)
                .header("anthropic-version", ANTHROPIC_VERSION_HEADER)
                .json(&request);
            let body_text = send_with_retry(&builder, "Anthropic").await?;
            let anthropic_response: AnthropicResponse = serde_json::from_str(&body_text)
                .map_err(|e| AppError::Import(format!("Failed to parse LLM response: {e}")))?;
            /* Surface budget truncation: `max_tokens` means the server hit
            the output-token budget before the model finished (parity with
            the OpenAI path's `finish_reason == "length"` log). */
            if anthropic_response.stop_reason.as_deref() == Some("max_tokens") {
                eprintln!(
                    "[LlmClient] Anthropic response truncated by output-token budget (stop_reason=max_tokens); content may be incomplete"
                );
            }
            /* Join every text block: multi-block responses interleave text
            with non-text blocks (`tool_use`, `thinking`) that carry no
            `text` field and are skipped. */
            let content = anthropic_response
                .content
                .iter()
                .filter_map(|block| block.text.as_deref())
                .collect::<Vec<&str>>()
                .join("");
            if content.is_empty() {
                return Err(AppError::Import("No response from LLM".to_string()));
            }
            let total_tokens =
                anthropic_response.usage.map_or(0, |u| u.input_tokens + u.output_tokens);
            Ok((content, total_tokens, anthropic_response.stop_reason.clone()))
        }
    })
    .await
}

// ── OpenAI-compatible path ───────────────────────────────────────────

async fn send_openai_compatible(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
    options: RequestOptions,
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
        | LlmProvider::BangoAi
        | LlmProvider::Custom => {
            if base_url.ends_with("/chat/completions") {
                base_url.to_string()
            } else {
                format!("{base_url}/chat/completions")
            }
        }
        /* Anthropic is absent by design: it routes to the native
        `send_anthropic` path in `send_chat_completion` and never reaches
        this OpenAI-compatible builder (Google likewise routes to
        `send_google`). */
        _ => base_url.to_string(),
    };

    let model_name = config.model_name.clone();
    let system_text = system_prompt.to_string();
    let user_text = user_prompt.to_string();
    let local = config.provider == LlmProvider::BangoAi;
    let response_format =
        (local && options.json_mode).then_some(ResponseFormat { kind: "json_object" });
    let chat_template_kwargs = if local {
        options.enable_thinking.map(|enable_thinking| ChatTemplateKwargs { enable_thinking })
    } else {
        None
    };

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
                response_format,
                chat_template_kwargs,
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
                let finish_reason = choice.finish_reason.clone();
                return Ok((choice.message.content, total_tokens, finish_reason));
            }

            // Strategy 2: Fallback - extract content from arbitrary JSON envelope
            let value: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
                AppError::Import(format!("Failed to parse LLM response as JSON: {e}"))
            })?;

            let content = extract_content_from_response(&value).ok_or_else(|| {
                AppError::Import(format!(
                    "Could not extract content from LLM response. Raw body (first 500 chars): {}",
                    truncate_chars(&body_text, 500)
                ))
            })?;

            let total_tokens = extract_total_tokens(&value);
            Ok((content, total_tokens, None))
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
    Fut: std::future::Future<Output = Result<(String, usize, Option<String>), AppError>>,
{
    let first_temp = if skip_temperature { None } else { Some(temperature) };

    match make_request(first_temp).await {
        Ok(tuple) => {
            Ok((tuple.0, tuple.1, CallMeta { finish_reason: tuple.2, ..CallMeta::default() }))
        }
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
                Ok(tuple) => Ok((
                    tuple.0,
                    tuple.1,
                    CallMeta {
                        temperature_was_rejected: true,
                        finish_reason: tuple.2,
                        ..CallMeta::default()
                    },
                )),
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
