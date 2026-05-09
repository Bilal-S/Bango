//! Abstraction over LLM HTTP calls, enabling integration tests with mock clients.

use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;

/// Trait abstracting a single chat-completion call.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a chat completion request. Returns `(response_text, total_tokens)`.
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError>;
}

// ---------------------------------------------------------------------------
// Real HTTP implementation (used in production)
// ---------------------------------------------------------------------------

/// Production client that forwards to the real LLM HTTP endpoint.
pub struct HttpLlmClient {
    pub config: LlmConfig,
}

#[async_trait::async_trait]
impl LlmClient for HttpLlmClient {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError> {
        client::send_chat_completion(&self.config, system, user).await
    }
}