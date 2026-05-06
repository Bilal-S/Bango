use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::llm_config::LlmConfig;

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

pub async fn send_chat_completion(
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
        crate::models::llm_config::LlmProvider::Openai
        | crate::models::llm_config::LlmProvider::MistralAi
        | crate::models::llm_config::LlmProvider::ZAi
        | crate::models::llm_config::LlmProvider::LlamaCpp
        | crate::models::llm_config::LlmProvider::Ollama
        | crate::models::llm_config::LlmProvider::LmStudio => {
            if base_url.ends_with("/chat/completions") {
                base_url.to_string()
            } else {
                format!("{}/chat/completions", base_url)
            }
        }
        crate::models::llm_config::LlmProvider::Google => {
            if base_url.contains(":generateContent") {
                base_url.to_string()
            } else {
                format!("{}/models/{}:generateContent", base_url, config.model_name)
            }
        }
        crate::models::llm_config::LlmProvider::Anthropic => {
            if base_url.ends_with("/messages") {
                base_url.to_string()
            } else {
                format!("{}/messages", base_url)
            }
        }
        crate::models::llm_config::LlmProvider::Custom => base_url.to_string(),
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
