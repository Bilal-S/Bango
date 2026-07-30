//! Embedding provider client.
//!
//! Per-provider HTTP shapes for embedding generation. Routes through
//! `client::shared_client()` for HTTP keep-alive but does NOT go through
//! `send_chat_completion` (embeddings use a different endpoint + response
//! shape).
//!
//! Provider model resolution is automatic + transparent: try a provider-default
//! embedding model first, fall back to the configured chat model, disable with
//! a toast if both fail. See `.worktrees/embed-plan.md` §5.

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

// ── OpenAI-compatible request/response ──────────────────────────────────────

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    // `usage` is deserialized for future diagnostics (token accounting) but
    // not currently read. Allowed dead-code rather than removed so the field
    // stays available without a schema-shape change later.
    #[serde(default)]
    #[allow(dead_code)]
    usage: Option<EmbeddingUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingUsage {
    #[serde(default)]
    #[allow(dead_code)]
    total_tokens: Option<usize>,
}

// ── Ollama request/response ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

// ── Google request/response ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEmbeddingRequest {
    content: GoogleContent,
}

#[derive(Debug, Serialize)]
struct GoogleContent {
    parts: [GooglePart; 1],
}

#[derive(Debug, Serialize)]
struct GooglePart {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEmbeddingResponse {
    embedding: GoogleEmbeddingValues,
}

#[derive(Debug, Deserialize)]
struct GoogleEmbeddingValues {
    values: Vec<f32>,
}

// ── Model resolution ────────────────────────────────────────────────────────

/// The provider-default embedding model. Returns `None` when the provider has
/// no canonical default (the probe then uses the configured chat model).
#[must_use]
pub fn default_embedding_model(provider: &LlmProvider) -> Option<&'static str> {
    match provider {
        LlmProvider::Openai => Some("text-embedding-3-small"),
        LlmProvider::MistralAi => Some("mistral-embed"),
        LlmProvider::Google => Some("text-embedding-004"),
        // Local servers serve embeddings from the loaded model; no default.
        LlmProvider::Ollama | LlmProvider::LmStudio | LlmProvider::LlamaCpp => None,
        // Custom endpoints: try the configured chat model.
        LlmProvider::Custom => None,
        // Known unsupported.
        LlmProvider::Anthropic | LlmProvider::ZAi => None,
    }
}

/// Whether a provider has a known embedding API. Anthropic and Z.AI are
/// known-unsupported; all others are tried at runtime (404/405 handled as a
/// disabled outcome by the probe).
#[must_use]
pub fn check_embedding_support(provider: &LlmProvider) -> bool {
    !matches!(provider, LlmProvider::Anthropic | LlmProvider::ZAi)
}

// ── v2: per-provider batch + token limits ───────────────────────────────────

/// Per-provider limits that govern how [`split_text_by_token_budget`] +
/// `send_embedding_batch_parallel` chunk + dispatch embedding requests.
///
/// All three caps must be respected simultaneously: a sub-batch is closed when
/// ANY of the three would be exceeded by adding one more input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingLimits {
    /// Max input strings per single `/embeddings` request. OpenAI: 2048,
    /// Ollama/Google: 1 (no native batch endpoint).
    pub max_inputs_per_batch: usize,
    /// Max tokens per single input string. OpenAI 3-series: 8191,
    /// `mistral-embed`: 4096, small Ollama models (e.g. `mxbai-embed-large`):
    /// 512. This drives [`split_text_by_token_budget`].
    pub max_tokens_per_input: usize,
    /// Max total input tokens per single `/embeddings` request. OpenAI's
    /// effective cap is ~300_000; local servers often have much smaller caps
    /// (e.g. 16_384 for consumer GPUs).
    pub max_tokens_per_batch: usize,
}

/// Conservative defaults for unknown / Custom providers. Kept small enough
/// that any reasonable local server accepts the request; production users on
/// OpenAI/Mistral get the larger per-model overrides below.
const DEFAULT_LIMITS: EmbeddingLimits = EmbeddingLimits {
    max_inputs_per_batch: 32,
    max_tokens_per_input: 512,
    max_tokens_per_batch: 16_384,
};

/// OpenAI-compatible defaults (also used by Mistral / LM Studio / llama.cpp /
/// Z.AI when tried at runtime / Custom).
const OPENAI_COMPATIBLE_LIMITS: EmbeddingLimits = EmbeddingLimits {
    max_inputs_per_batch: 2048,
    max_tokens_per_input: 8191,
    max_tokens_per_batch: 300_000,
};

/// Resolve the per-provider + per-model limits used by the v2 batch pipeline.
///
/// The model name is consulted for known OpenAI/Mistral variants with smaller
/// per-input caps (e.g. some older OpenAI models had 2046-token limits). For
/// Ollama we conservatively assume a small-context embedding model unless the
/// model name matches a known large-context family (`nomic-embed-text`:
/// 8192; `mxbai-embed-large`: 512).
///
/// Pure `#[must_use]` so it is unit-testable in isolation.
#[must_use]
pub fn embedding_limits(provider: &LlmProvider, model: &str) -> EmbeddingLimits {
    match provider {
        LlmProvider::Openai => {
            // All current OpenAI embedding models (3-small, 3-large, ada-002)
            // share the 8191 per-input cap. Override only if a future smaller
            // model is detected by name.
            EmbeddingLimits {
                max_inputs_per_batch: 2048,
                max_tokens_per_input: 8191,
                max_tokens_per_batch: 300_000,
            }
        }
        LlmProvider::MistralAi => EmbeddingLimits {
            max_inputs_per_batch: 2048,
            max_tokens_per_input: 4096, // mistral-embed
            max_tokens_per_batch: 300_000,
        },
        LlmProvider::Google => EmbeddingLimits {
            // Google's `embedContent` is one-text-per-call; `batchEmbedContents`
            // (deferred per plan §5.1) accepts up to 100 inputs. We use the
            // single-call shape, so max_inputs_per_batch = 1.
            max_inputs_per_batch: 1,
            max_tokens_per_input: 2048, // text-embedding-004
            max_tokens_per_batch: 2048,
        },
        LlmProvider::Ollama | LlmProvider::LmStudio | LlmProvider::LlamaCpp => {
            // Local servers: assume small per-input + small batch unless the
            // model name signals a known large-context family.
            let max_tokens_per_input = if model.contains("nomic") {
                8192
            } else if model.contains("mxbai") {
                512
            } else {
                2048 // conservative default for unknown local models
            };
            EmbeddingLimits {
                max_inputs_per_batch: 1, // Ollama's /api/embeddings is one-prompt-per-call
                max_tokens_per_input,
                max_tokens_per_batch: max_tokens_per_input, // single-text batches
            }
        }
        LlmProvider::Custom => OPENAI_COMPATIBLE_LIMITS,
        LlmProvider::Anthropic | LlmProvider::ZAi => {
            // Unsupported; the probe flips to Disabled before this is reached.
            // Return conservative defaults so a misconfigured call still splits
            // conservatively rather than sending a huge batch the provider
            // can't handle.
            DEFAULT_LIMITS
        }
    }
}

// ── Embedding call ──────────────────────────────────────────────────────────

/// Embed one or more texts via the provider's embedding endpoint.
///
/// Providers with batch support (OpenAI-compatible) send `input: [...]` in one
/// request and return `Vec<Vec<f32>>` in input order. Ollama and Google lack
/// batch endpoints, so this function issues one request per text (still
/// bounded by the orchestrator's semaphore when called through `send_embedding`).
///
/// Returns `(vectors, dimensions)`. The dimensions are taken from the first
/// vector; the caller validates the rest match.
pub async fn embed_texts(
    config: &LlmConfig,
    texts: &[String],
    model: &str,
) -> Result<(Vec<Vec<f32>>, i32), AppError> {
    if texts.is_empty() {
        return Err(AppError::Validation("No texts to embed".to_string()));
    }

    let client = crate::llm::client::shared_client();

    match config.provider {
        LlmProvider::Google => {
            let api_key = config
                .api_key_encrypted
                .clone()
                .ok_or_else(|| AppError::Import("API key required for Google".to_string()))?;
            let base_url = config.endpoint_url.trim_end_matches('/');
            let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
            for text in texts {
                let endpoint = format!("{base_url}/models/{model}:embedContent");
                let request = GoogleEmbeddingRequest {
                    content: GoogleContent { parts: [GooglePart { text: text.clone() }] },
                };
                let builder = client
                    .post(&endpoint)
                    .header("Content-Type", "application/json")
                    .header("X-goog-api-key", &api_key)
                    .json(&request);
                let body_text = crate::llm::client::send_with_retry(&builder, "Google").await?;
                let response: GoogleEmbeddingResponse =
                    serde_json::from_str(&body_text).map_err(|e| {
                        AppError::Import(format!("Failed to parse Google embedding response: {e}"))
                    })?;
                out.push(response.embedding.values);
            }
            let dims = out.first().map(|v| v.len() as i32).unwrap_or(0);
            Ok((out, dims))
        }
        LlmProvider::Ollama => {
            let base_url = config.endpoint_url.trim_end_matches('/');
            // Ollama uses a dedicated endpoint, not /v1/embeddings.
            let base = base_url.trim_end_matches("/v1").trim_end_matches("/api");
            let endpoint = format!("{base}/api/embeddings");
            let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
            for text in texts {
                let request =
                    OllamaEmbeddingRequest { model: model.to_string(), prompt: text.clone() };
                let builder = client
                    .post(&endpoint)
                    .header("Content-Type", "application/json")
                    .json(&request);
                let body_text = crate::llm::client::send_with_retry(&builder, "Ollama").await?;
                let response: OllamaEmbeddingResponse =
                    serde_json::from_str(&body_text).map_err(|e| {
                        AppError::Import(format!("Failed to parse Ollama embedding response: {e}"))
                    })?;
                out.push(response.embedding);
            }
            let dims = out.first().map(|v| v.len() as i32).unwrap_or(0);
            Ok((out, dims))
        }
        // OpenAI-compatible: OpenAI, Mistral, LM Studio, llama.cpp, Z.AI (tried at runtime), Custom
        _ => {
            let base_url = config.endpoint_url.trim_end_matches('/');
            // Strip a trailing /embeddings or /chat/completions so we control the path.
            let base =
                base_url.trim_end_matches("/embeddings").trim_end_matches("/chat/completions");
            let endpoint = format!("{base}/embeddings");
            let api_key = config.api_key_encrypted.as_deref().unwrap_or("");

            let request = EmbeddingRequest { model: model.to_string(), input: texts.to_vec() };
            let builder = client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .bearer_auth(api_key)
                .json(&request);
            let body_text =
                crate::llm::client::send_with_retry(&builder, "OpenAI-compatible").await?;
            let response: EmbeddingResponse = serde_json::from_str(&body_text).map_err(|e| {
                AppError::Import(format!("Failed to parse embedding response: {e}"))
            })?;
            // Sort by index (if present) so vectors match input order.
            let mut data = response.data;
            data.sort_by_key(|d| d.index.unwrap_or(0));
            let out: Vec<Vec<f32>> = data.into_iter().map(|d| d.embedding).collect();
            let dims = out.first().map(|v| v.len() as i32).unwrap_or(0);
            Ok((out, dims))
        }
    }
}

// ── Capability probe ────────────────────────────────────────────────────────

/// The outcome of probing a provider for embedding support.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    /// `"enabled"` | `"disabled"`.
    pub status: String,
    pub model: String,
    pub dimensions: i32,
    /// Human-readable reason (for the toast message).
    pub reason: String,
}

/// Probe a provider for embedding capability. Resolution order:
/// 1. Anthropic -> `disabled` immediately.
/// 2. Try provider-default embedding model with a tiny probe text.
/// 3. On failure, retry with the configured chat model.
/// 4. Both fail -> `disabled`.
pub async fn probe_embedding_support(config: &LlmConfig) -> ProbeOutcome {
    if !check_embedding_support(&config.provider) {
        return ProbeOutcome {
            status: "disabled".to_string(),
            model: String::new(),
            dimensions: 0,
            reason: format!("{:?} does not support embeddings", config.provider),
        };
    }

    // Try the provider-default model first (if one exists).
    if let Some(default_model) = default_embedding_model(&config.provider) {
        match embed_texts(config, &["probe".to_string()], default_model).await {
            Ok((vectors, dims)) if !vectors.is_empty() => {
                return ProbeOutcome {
                    status: "enabled".to_string(),
                    model: default_model.to_string(),
                    dimensions: dims,
                    reason: "Embeddings enabled".to_string(),
                };
            }
            Ok(_) => {
                eprintln!("[embedding] default model {default_model} returned no vectors");
            }
            Err(e) => {
                eprintln!(
                    "[embedding] default model {default_model} failed: {e}; trying chat model"
                );
            }
        }
    }

    // Fall back to the configured chat model.
    let chat_model = &config.model_name;
    match embed_texts(config, &["probe".to_string()], chat_model).await {
        Ok((vectors, dims)) if !vectors.is_empty() => {
            return ProbeOutcome {
                status: "enabled".to_string(),
                model: chat_model.clone(),
                dimensions: dims,
                reason: "Embeddings enabled using chat model".to_string(),
            };
        }
        Ok(_) => {
            eprintln!("[embedding] chat model {chat_model} returned no vectors");
        }
        Err(e) => {
            eprintln!("[embedding] chat model {chat_model} failed: {e}");
        }
    }

    ProbeOutcome {
        status: "disabled".to_string(),
        model: String::new(),
        dimensions: 0,
        reason: "No embedding-capable model found".to_string(),
    }
}
