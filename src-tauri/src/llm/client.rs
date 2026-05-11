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

// ── Public API ───────────────────────────────────────────────────────

pub async fn list_models(
    provider: &LlmProvider,
    endpoint_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let client = Client::new();
    let base_url = endpoint_url.trim_end_matches('/');

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

pub async fn send_chat_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, usize), AppError> {
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
) -> Result<(String, usize), AppError> {
    let client = Client::new();
    let temp = if config.skip_temperature { None } else { Some(config.temperature) };
    let request = GoogleRequest {
        system_instruction: GoogleSystemInstruction {
            parts: GoogleSystemPart { text: system_prompt.to_string() },
        },
        contents: vec![GoogleContent {
            role: "user".to_string(),
            parts: vec![GooglePart { text: user_prompt.to_string() }],
        }],
        generation_config: GoogleGenerationConfig { temperature: temp },
    };

    let api_key = config
        .api_key_encrypted
        .as_deref()
        .ok_or_else(|| AppError::Import("API key required for Google".to_string()))?;

    let base_url = config.endpoint_url.trim_end_matches('/');
    let endpoint = if base_url.contains(":generateContent") {
        base_url.to_string()
    } else {
        format!("{}/models/{}:generateContent", base_url, config.model_name)
    };

    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("X-goog-api-key", api_key)
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("LLM request failed: {e}")))?;

    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::Import("Rate limited (429)".to_string()));
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Import(format!("LLM request failed ({status}): {body}")));
    }

    let google_response: GoogleResponse = response
        .json()
        .await
        .map_err(|e| AppError::Import(format!("Failed to parse LLM response: {e}")))?;

    let content = google_response
        .candidates
        .first()
        .and_then(|c| c.content.parts.first())
        .map(|p| p.text.clone())
        .ok_or_else(|| AppError::Import("No response from LLM".to_string()))?;

    let total_tokens = google_response.usage_metadata.map(|u| u.total_token_count).unwrap_or(0);
    Ok((content, total_tokens))
}

// ── OpenAI-compatible path ───────────────────────────────────────────

async fn send_openai_compatible(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<(String, usize), AppError> {
    let client = Client::new();
    let temp = if config.skip_temperature { None } else { Some(config.temperature) };
    let request = ChatRequest {
        model: config.model_name.clone(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
            ChatMessage { role: "user".to_string(), content: user_prompt.to_string() },
        ],
        temperature: temp,
    };

    let api_key = config.api_key_encrypted.as_deref().unwrap_or("");

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

    let response = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("LLM request failed: {e}")))?;

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::Import("Rate limited (429)".to_string()));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Import(format!("LLM request failed ({status}): {body}")));
    }

    // Read raw body text so we can try multiple deserialization strategies
    let body_text = response
        .text()
        .await
        .map_err(|e| AppError::Import(format!("Failed to read LLM response body: {e}")))?;

    // Strategy 1: Try standard ChatResponse (OpenAI format)
    if let Ok(chat_response) = serde_json::from_str::<ChatResponse>(&body_text) {
        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| AppError::Import("No response from LLM".to_string()))?;
        let total_tokens = chat_response.usage.and_then(|u| u.total_tokens).unwrap_or(0);
        return Ok((content, total_tokens));
    }

    // Strategy 2: Fallback - extract content from arbitrary JSON envelope
    let value: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| AppError::Import(format!("Failed to parse LLM response as JSON: {e}")))?;

    let content = extract_content_from_response(&value).ok_or_else(|| {
        AppError::Import(format!(
            "Could not extract content from LLM response. Raw body (first 500 chars): {}",
            &body_text[..body_text.len().min(500)]
        ))
    })?;

    // Try to get token count from the envelope
    let total_tokens = extract_total_tokens(&value);
    Ok((content, total_tokens))
}

/// Attempt to extract text content from an arbitrary LLM response JSON value.
///
/// Handles:
/// - Standard OpenAI: `choices[0].message.content` (string)
/// - z.ai / non-standard: `message.content` (string or array of text objects)
/// - Any provider with a `content` key at the first two levels
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
