//! Abstraction over LLM HTTP calls, enabling integration tests with mock clients.

use std::sync::Arc;

use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;

/// Trait abstracting a single chat-completion call.
///
/// The default `send` categorizes the request as `Screening` (stage-1 abstract
/// screening). Tier 3 stage-2 calls (enhanced / two-stage) use `send_with_type`
/// with `LlmRequestType::EnhancedScreening` so diagnostics can distinguish them.
/// The default impl of `send_with_type` delegates to `send`, so existing test
/// mocks are unaffected and only the production `HttpLlmClient` overrides it.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a chat completion request categorized as stage-1 `Screening`.
    /// Returns `(response_text, total_tokens)`.
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError>;

    /// Send a chat completion request with an explicit `LlmRequestType`.
    /// Default impl delegates to `send` (preserving backward compat for mocks
    /// that don't care about the request type). Production overrides to route
    /// the type through the orchestrator.
    async fn send_with_type(
        &self,
        system: &str,
        user: &str,
        _request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        self.send(system, user).await
    }
}

// ---------------------------------------------------------------------------
// Real HTTP implementation (used in production)
// ---------------------------------------------------------------------------

/// Production client that routes through the `LlmOrchestrator`.
pub struct HttpLlmClient {
    pub config: LlmConfig,
    pub orchestrator: Arc<LlmOrchestrator>,
}

#[async_trait::async_trait]
impl LlmClient for HttpLlmClient {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError> {
        self.orchestrator.send(&self.config, system, user, LlmRequestType::Screening).await
    }

    async fn send_with_type(
        &self,
        system: &str,
        user: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        self.orchestrator.send(&self.config, system, user, request_type).await
    }
}
