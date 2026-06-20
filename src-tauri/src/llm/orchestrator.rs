//! Centralized LLM request coordinator.
//!
//! All LLM calls MUST go through `LlmOrchestrator` (Tauri managed state).
//! The orchestrator enforces:
//! - Concurrency limits (`max_concurrent_requests`)
//! - Rate limiting (`request_delay_ms`)
//! - Unified error logging

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;

/// Maximum time to wait for a single LLM response.
const LLM_TIMEOUT_SECS: u64 = 120;

/// Label for categorizing LLM request sources.
#[derive(Debug, Clone)]
pub enum LlmRequestType {
    Screening,
    AiSummary,
    ArticleSummary,
    TagGeneration,
    LabelGeneration,
    CriteriaGeneration,
    SummaryGeneration,
    TestConnection,
    Chat,
    WikiIngest,
    WikiChat,
}

/// Centralized LLM request coordinator.
///
/// Registered as Tauri managed state. All LLM calls flow through here
/// to enforce concurrency limits and rate limiting from `LlmConfig`.
pub struct LlmOrchestrator {
    semaphore: Arc<Semaphore>,
    last_request: Arc<tokio::sync::Mutex<Instant>>,
    request_delay_ms: Arc<tokio::sync::Mutex<u64>>,
}

impl LlmOrchestrator {
    /// Create a new orchestrator with the given concurrency limit and delay.
    pub fn new(max_concurrent: usize, request_delay_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            last_request: Arc::new(tokio::sync::Mutex::new(
                Instant::now() - Duration::from_millis(request_delay_ms.max(1)),
            )),
            request_delay_ms: Arc::new(tokio::sync::Mutex::new(request_delay_ms)),
        }
    }

    /// Update concurrency/delay settings when LLM config changes.
    pub async fn update_settings(&self, max_concurrent: usize, request_delay_ms: u64) {
        // Grow semaphore if needed (shrinking is not supported by tokio::Semaphore)
        let current = self.semaphore.available_permits();
        if max_concurrent > current {
            self.semaphore.add_permits(max_concurrent - current);
        }
        *self.request_delay_ms.lock().await = request_delay_ms;
    }

    /// Send a chat completion request through the orchestrator.
    ///
    /// Enforces concurrency limits and rate limiting.
    pub async fn send(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        // 1. Acquire semaphore permit (waits if at concurrency limit)
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::Import("LLM orchestrator closed".to_string()))?;

        // 2. Rate limiting: ensure minimum delay between requests
        self.enforce_rate_limit().await;

        // 3. Make the actual LLM call with a 2-minute timeout
        let result = tokio::time::timeout(
            Duration::from_secs(LLM_TIMEOUT_SECS),
            client::send_chat_completion(config, system_prompt, user_prompt),
        )
        .await
        .map_err(|_| {
            AppError::Import(format!("LLM request timed out after {} seconds", LLM_TIMEOUT_SECS))
        })?;

        // 4. Log errors centrally
        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] {:?} request failed: {}", request_type, e);
        }

        result
    }

    /// Send a chat completion without acquiring the semaphore.
    ///
    /// Used for test-connection requests where the user expects immediate feedback.
    /// Rate limiting is still enforced.
    pub async fn send_unthrottled(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        // Only enforce rate limiting, no semaphore
        self.enforce_rate_limit().await;

        let result = tokio::time::timeout(
            Duration::from_secs(LLM_TIMEOUT_SECS),
            client::send_chat_completion(config, system_prompt, user_prompt),
        )
        .await
        .map_err(|_| {
            AppError::Import(format!("LLM request timed out after {} seconds", LLM_TIMEOUT_SECS))
        })?;

        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] {:?} request failed: {}", request_type, e);
        }

        result
    }

    /// Enforce minimum delay between consecutive requests.
    async fn enforce_rate_limit(&self) {
        let delay_ms = *self.request_delay_ms.lock().await;
        if delay_ms == 0 {
            return;
        }
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < Duration::from_millis(delay_ms) {
            let wait = Duration::from_millis(delay_ms) - elapsed;
            tokio::time::sleep(wait).await;
        }
        *last = Instant::now();
    }

    /// Test an LLM connection using a simple "hello" prompt.
    /// Does NOT go through the semaphore (not a real request).
    pub async fn test_connection(&self, config: &LlmConfig) -> Result<(String, usize), AppError> {
        tokio::time::timeout(
            Duration::from_secs(30), // shorter timeout for connection test
            crate::llm::client::send_chat_completion(config, "You are a test.", "Say hello."),
        )
        .await
        .map_err(|_| {
            AppError::Import("LLM connection test timed out after 30 seconds".to_string())
        })?
    }

    /// Get number of available semaphore permits (for diagnostics).
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}
