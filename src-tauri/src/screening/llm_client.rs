//! LLM client abstraction (trait + production impl). Enables mock clients in integration tests.

use std::sync::Arc;

use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;

/// Abstraction over a single chat-completion call. Default `send` is stage-1 `Screening`;
/// `send_with_type` with `EnhancedScreening` for stage-2. Default impl delegates to `send`
/// so mocks are unaffected.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Send chat completion as stage-1 `Screening`. Returns `(response_text, total_tokens)`.
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError>;

    /// Send with explicit `LlmRequestType`. Default delegates to `send`.
    async fn send_with_type(
        &self,
        system: &str,
        user: &str,
        _request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        self.send(system, user).await
    }
}

// ── Production HTTP client ──────────────────────────────────────────────────

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
