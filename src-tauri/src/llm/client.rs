use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::llm_config::{LlmConfig, LlmProvider};

// ── OpenAI-compatible types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
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
    temperature: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleRequest {
    system_instruction: GoogleSystemInstruction,
    contents: Vec<GoogleContent>,
    generation_config: GoogleGenerationConfig,
}

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    candidates: Vec<GoogleCandidate>,
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

// ── Public API ───────────────────────────────────────────────────────

pub async fn send_chat_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, AppError> {
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
) -> Result<String, AppError> {
    let client = Client::new();
    let request = GoogleRequest {
        system_instruction: GoogleSystemInstruction {
            parts: GoogleSystemPart { text: system_prompt.to_string() },
        },
        contents: vec![GoogleContent {
            role: "user".to_string(),
            parts: vec![GooglePart { text: user_prompt.to_string() }],
        }],
        generation_config: GoogleGenerationConfig { temperature: config.temperature },
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

    google_response
        .candidates
        .first()
        .and_then(|c| c.content.parts.first())
        .map(|p| p.text.clone())
        .ok_or_else(|| AppError::Import("No response from LLM".to_string()))
}

// ── OpenAI-compatible path ───────────────────────────────────────────

async fn send_openai_compatible(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, AppError> {
    let client = Client::new();
    let request = ChatRequest {
        model: config.model_name.clone(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
            ChatMessage { role: "user".to_string(), content: user_prompt.to_string() },
        ],
        temperature: config.temperature,
    };

    let api_key = config.api_key_encrypted.as_deref().unwrap_or("");

    let base_url = config.endpoint_url.trim_end_matches('/');
    let endpoint = match config.provider {
        crate::models::llm_config::LlmProvider::LlamaCpp
        | crate::models::llm_config::LlmProvider::Ollama
        | crate::models::llm_config::LlmProvider::LmStudio
        | crate::models::llm_config::LlmProvider::MistralAi
        | crate::models::llm_config::LlmProvider::ZAi => {
            if base_url.ends_with("/chat/completions") {
                base_url.to_string()
            } else {
                format!("{base_url}/chat/completions")
            }
        }
        crate::models::llm_config::LlmProvider::Anthropic => {
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

    let chat_response: ChatResponse = response
        .json()
        .await
        .map_err(|e| AppError::Import(format!("Failed to parse LLM response: {e}")))?;

    chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| AppError::Import("No response from LLM".to_string()))
}
