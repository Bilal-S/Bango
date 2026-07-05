//! Centralized LLM request coordinator.
//!
//! All LLM calls MUST go through `LlmOrchestrator` (Tauri managed state).
//! The orchestrator enforces:
//! - Concurrency limits (`max_concurrent_requests`)
//! - Rate limiting (`request_delay_ms`)
//! - Unified error logging

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;

/// Maximum time to wait for a single LLM response.
const LLM_TIMEOUT_SECS: u64 = 600;

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
    /// T1.3: per-section AI summary (Methods/Results/Discussion) generated
    /// alongside the whole-paper summary.
    SectionSummary,
    /// Tier 2 Phase 4: batched LLM description of figure/table captions.
    FigureDescription,
    /// Tier 3: enhanced / two-stage screening stage 2 (abstract + retrieved
    /// full-text chunks). Stage 1 stays `Screening`. Categorized separately so
    /// diagnostics can distinguish abstract-only from full-text-aware calls.
    EnhancedScreening,
    /// Tier 4.2: unified AI summary - the synthesis call that combines
    /// per-section summaries + figure/table descriptions into one upgraded
    /// `summary_150_250_words` digest. Distinguishes the unified path from the
    /// legacy monolithic `ArticleSummary` and the section-only `SectionSummary`.
    UnifiedSummary,
    /// Multilingual translation: per-chunk or per-metadata translation of a
    /// non-English article to English. Categorized separately so diagnostics
    /// can distinguish translation calls (Plan-A permanent rewrite) from
    /// screening/summary calls. The translation engine owns a thin
    /// `TranslationLlmClient` that logs `job_id`/`part_id` before delegating
    /// to the orchestrator via `send_with_type(LlmRequestType::Translation)`.
    Translation,
}

/// Centralized LLM request coordinator.
///
/// Registered as Tauri managed state. All LLM calls flow through here
/// to enforce concurrency limits and rate limiting from `LlmConfig`.
///
/// The concurrency limit is enforced by a `tokio::sync::Semaphore` wrapped in
/// a `std::sync::RwLock<Arc<Semaphore>>`. `tokio::Semaphore` only supports
/// growing (`add_permits`), not shrinking, so `update_settings` swaps in a
/// fresh semaphore of the exact new size instead of mutating the old one.
/// `send()` clones the `Arc<Semaphore>` under a brief read lock and drops the
/// guard before awaiting permit acquisition, so settings updates are never
/// blocked by in-flight requests and no lock guard is held across an `.await`.
pub struct LlmOrchestrator {
    semaphore: RwLock<Arc<Semaphore>>,
    last_request: Arc<tokio::sync::Mutex<Instant>>,
    request_delay_ms: Arc<tokio::sync::Mutex<u64>>,
}

impl LlmOrchestrator {
    /// Create a new orchestrator with the given concurrency limit and delay.
    pub fn new(max_concurrent: usize, request_delay_ms: u64) -> Self {
        Self {
            semaphore: RwLock::new(Arc::new(Semaphore::new(max_concurrent.max(1)))),
            last_request: Arc::new(tokio::sync::Mutex::new(
                Instant::now() - Duration::from_millis(request_delay_ms.max(1)),
            )),
            request_delay_ms: Arc::new(tokio::sync::Mutex::new(request_delay_ms)),
        }
    }

    /// Update concurrency/delay settings when LLM config changes.
    ///
    /// Replaces the semaphore with a fresh one of the exact requested size.
    /// This correctly handles both growing and shrinking the limit (the
    /// previous `add_permits` approach could only grow, and used
    /// `available_permits` instead of capacity, causing unbounded growth when
    /// requests were in-flight during a save).
    ///
    /// In-flight requests holding permits on the old semaphore are unaffected;
    /// they keep the old `Arc<Semaphore>` alive until their permit drops. This
    /// means a lower limit takes effect for *new* acquisitions only - there is
    /// a brief transient window where (old in-flight + new capacity) may exceed
    /// the new limit. This is acceptable for an advisory LLM rate limiter and
    /// avoids preemptively cancelling running requests.
    pub async fn update_settings(&self, max_concurrent: usize, request_delay_ms: u64) {
        let new_sem = Arc::new(Semaphore::new(max_concurrent.max(1)));
        // Recover from poison (a panicked lock holder) rather than unwinding:
        // an `Arc<Semaphore>` is never left structurally inconsistent by a panic.
        *self.semaphore.write().unwrap_or_else(|p| p.into_inner()) = new_sem;
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
        // 1. Acquire semaphore permit (waits if at concurrency limit).
        // Clone the Arc under a brief read lock, then drop the guard before
        // awaiting so update_settings is never blocked by an active request.
        let sem = self.semaphore.read().unwrap_or_else(|p| p.into_inner()).clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| AppError::Import("LLM orchestrator closed".to_string()))?;

        // 2. Rate limiting: ensure minimum delay between requests
        self.enforce_rate_limit().await;

        // 3. Make the actual LLM call with a 5-minute timeout
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
    ///
    /// Reflects the *current* semaphore only. Immediately after
    /// `update_settings`, permits held on the previous semaphore are not
    /// counted, so this may briefly under-report real in-flight work during
    /// the swap transient window.
    pub fn available_permits(&self) -> usize {
        self.semaphore.read().unwrap_or_else(|p| p.into_inner()).available_permits()
    }
}
