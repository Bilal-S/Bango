//! Centralized LLM request coordinator.
//!
//! All LLM calls MUST go through `LlmOrchestrator` (Tauri managed state).
//! The orchestrator enforces:
//! - Concurrency limits (`max_concurrent_requests`)
//! - Rate limiting (`request_delay_ms`)
//! - Unified error logging
//! - Post-call persistence of the `skip_temperature` flag when the client
//!   recovers from a temperature-rejection 400 (so the next call skips the
//!   wasteful first-attempt failure)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::LlmConfig;
use crate::utils::json_repair::prepare_llm_json;

/// Best-effort callback used by the orchestrator to persist the
/// `skip_temperature` flag when the client recovered from a temperature-rejection 400.
///
/// Implementations acquire the DB lock, run `llm_config_repo::set_skip_temperature`, and drop the
/// lock. The implementation MUST NOT be invoked while any caller holds the DB lock: every
/// orchestrator call site releases its DB lock before calling `orchestrator.send` (per spec §8.1
/// "lock-release-call-lock" worker pattern + the same discipline enforced across all command
/// handlers), so the post-call lock acquisition here is provably deadlock-free.
///
/// The trait exists so the orchestrator stays decoupled from `tauri::AppHandle` + `DbState`
/// (testable without the Tauri runtime) and so tests can inject a fake that records invocations
/// without a live SQLite connection.
pub trait TemperatureFlagPersister: Send + Sync {
    /// Persist `skip_temperature = skip`. Best-effort: implementations log and
    /// swallow errors so a DB failure never fails a successful LLM call.
    fn persist(&self, skip: bool);
}

/// Maximum time to wait for a single LLM response (default for all request
/// types except screening).
const LLM_TIMEOUT_SECS: u64 = 600;

/// Maximum time to wait for a single screening LLM response (stage-1 abstract
/// and stage-2 enhanced/two-stage). Screening is a high-volume, low-latency
/// operation compared to AI summaries / wiki ingest / chat, and abstract
/// screening prompts are small, so a tighter cap surfaces hung/slow calls as
/// errors within ~2 minutes instead of stalling the run for the 10-minute
/// default. The user can mitigate sustained slowness by lowering `batch_size`.
const SCREENING_TIMEOUT_SECS: u64 = 120;

/// Pick the per-call wall-clock timeout based on the request type.
///
/// Screening (both stage-1 `Screening` and stage-2 `EnhancedScreening`) uses
/// the tighter `SCREENING_TIMEOUT_SECS` cap. All other request types
/// (summaries, chat, wiki ingest, translation, gap analysis, etc.) use the
/// default `LLM_TIMEOUT_SECS`.
///
/// Pure function so it can be unit-tested in isolation without a live
/// orchestrator or network.
#[must_use]
pub fn timeout_for(request_type: &LlmRequestType) -> Duration {
    match request_type {
        LlmRequestType::Screening | LlmRequestType::EnhancedScreening => {
            Duration::from_secs(SCREENING_TIMEOUT_SECS)
        }
        _ => Duration::from_secs(LLM_TIMEOUT_SECS),
    }
}

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
    /// Research Gap Analysis: corpus-wide Markdown report over included
    /// articles (thematic coverage, identified gaps, methodological landscape,
    /// future directions). Distinguishes the call from `SummaryGeneration`
    /// (the literature review) in diagnostics so per-mode cost and failure
    /// rates are visible.
    GapAnalysis,
    /// Search Strategy Builder (spec §8.4): generates database-ready Boolean
    /// search strings for 8 academic databases from the research aims +
    /// criteria. One call per generation; the result is session-scoped (no
    /// persistence). Distinguished from `CriteriaGeneration` so diagnostics
    /// can separate search-strategy requests from criteria suggest/critique.
    SearchStrategy,
    /// OpenAlex Smart Search (Tier 2): generates an OpenAlex Boolean query
    /// from research aims + inclusion/exclusion criteria. The user reviews
    /// the query before executing it against the OpenAlex API.
    OpenAlexSmartSearch,
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
    /// Best-effort `skip_temperature` persister. `None` in tests (no-op); `Some`
    /// in production, wired in `init_background_state` after the orchestrator is
    /// constructed. Kept optional + set via a separate setter so the existing
    /// 2-param [`new`](Self::new) constructor stays unchanged - the ~40 test
    /// call sites need zero edits.
    temp_persister: RwLock<Option<Arc<dyn TemperatureFlagPersister>>>,
    /// In-session latch: once any call in this process recovers from a
    /// temperature-rejection 400, this flips to `true` so every subsequent call
    /// omits `temperature` from the start (no wasteful first-attempt 400 + retry).
    /// Lock-free (`AtomicBool`) because it is written once (the first rejection)
    /// and read on every call. Complements the DB persistence (which covers
    /// future process restarts but cannot reach the in-memory `LlmConfig`
    /// cached by long-running consumers like the screening engine).
    temperature_rejected_in_session: AtomicBool,
}

/// No-op persister used when no DB is available (tests, or production before
/// `set_temperature_persister` is wired). Drops the flag silently; the next
/// call simply retries temperature-recovery again if the model still rejects
/// `temperature`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpTemperaturePersister;

impl TemperatureFlagPersister for NoOpTemperaturePersister {
    fn persist(&self, _skip: bool) {
        // Intentionally no-op.
    }
}

impl LlmOrchestrator {
    /// Create a new orchestrator with the given concurrency limit and delay.
    ///
    /// The temperature-flag persister starts as `None`; production wires it via
    /// [`set_temperature_persister`](Self::set_temperature_persister) after the
    /// orchestrator is registered as managed state. Keeping it out of the
    /// constructor signature means the ~40 existing test call sites (`new(...)`)
    /// need zero edits.
    pub fn new(max_concurrent: usize, request_delay_ms: u64) -> Self {
        Self {
            semaphore: RwLock::new(Arc::new(Semaphore::new(max_concurrent.max(1)))),
            last_request: Arc::new(tokio::sync::Mutex::new(
                Instant::now() - Duration::from_millis(request_delay_ms.max(1)),
            )),
            request_delay_ms: Arc::new(tokio::sync::Mutex::new(request_delay_ms)),
            temp_persister: RwLock::new(None),
            temperature_rejected_in_session: AtomicBool::new(false),
        }
    }

    /// Wire the best-effort `skip_temperature` persister (production only).
    ///
    /// Must be called once during startup (`init_background_state`) after the
    /// `DbState` is available. When unset, temperature-recovery still WORKS
    /// (the client retries without `temperature`), but the flag is not
    /// persisted, so the next call repeats the wasteful first-attempt failure.
    pub fn set_temperature_persister(&self, persister: Arc<dyn TemperatureFlagPersister>) {
        *self.temp_persister.write().unwrap_or_else(|p| p.into_inner()) = Some(persister);
    }

    /// Best-effort: if the client set `CallMeta.temperature_was_rejected`,
    /// spawn a detached task to persist `skip_temperature = true`. Failures are
    /// logged and swallowed so a DB hiccup never fails a successful LLM call.
    ///
    /// INVARIANT: this MUST run AFTER the LLM call returns, never before or
    /// during. Every caller of [`send`](Self::send) releases its DB lock before
    /// invoking the orchestrator (spec §8.1 "lock-release-call-lock" worker
    /// pattern + the same discipline enforced across all command handlers), so
    /// the persister's lock acquisition here cannot deadlock with a caller.
    fn maybe_persist_skip_temperature(&self, meta: client::CallMeta) {
        if !meta.temperature_was_rejected {
            return;
        }
        // Latch the in-session flag FIRST so concurrent + immediately-following
        // calls skip temperature without waiting for the detached DB write. This
        // is the fix for the "every batch retries temperature" bug: long-running
        // consumers (screening engine) cache `LlmConfig` and never re-read the
        // DB row, so the DB persistence alone cannot reach them mid-run.
        self.temperature_rejected_in_session.store(true, Ordering::Relaxed);
        let persister = self.temp_persister.read().unwrap_or_else(|p| p.into_inner()).clone();
        let Some(persister) = persister else {
            // No persister wired (tests, or pre-`set_temperature_persister`):
            // log and move on. The next call will retry recovery.
            eprintln!(
                "[LlmOrchestrator] temperature rejected by model but no persister wired; \
                 skip_temperature NOT persisted"
            );
            return;
        };
        // Detached: persistence must not block the response hand-off to the
        // caller (screening batches, chat UX, etc.). The persister impl is
        // expected to be cheap (one `UPDATE` under a short-lived DB lock).
        // `spawn_blocking` runs the synchronous SQLite write on the blocking
        // thread pool (not the async executor) and is itself detached, so no
        // outer `tokio::task::spawn` wrapper is needed.
        tokio::task::spawn_blocking(move || persister.persist(true));
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

        // 3. If a prior call in this session already discovered the model
        //    rejects `temperature`, clone the caller's config with
        //    `skip_temperature = true` so this call omits the parameter from
        //    the start (no wasteful first-attempt 400 + retry). The session
        //    latch complements DB persistence: the DB flag covers future
        //    process restarts, but cannot reach the in-memory `LlmConfig`
        //    cached by long-running consumers (e.g. the screening engine's
        //    `HttpLlmClient.config`).
        let effective_config: LlmConfig;
        let config_ref = if self.temperature_rejected_in_session.load(Ordering::Relaxed)
            && !config.skip_temperature
        {
            effective_config = {
                let mut c = config.clone();
                c.skip_temperature = true;
                c
            };
            &effective_config
        } else {
            config
        };

        // 4. Make the actual LLM call with a per-request-type timeout.
        let timeout = timeout_for(&request_type);
        let timeout_secs = timeout.as_secs();
        eprintln!(
            "[screening:diag] orchestrator: LLM call START type={request_type:?} timeout={timeout_secs}s"
        );
        let call_start = std::time::Instant::now();
        let result = tokio::time::timeout(
            timeout,
            client::send_chat_completion(config_ref, system_prompt, user_prompt),
        )
        .await
        .map_err(|_| {
            eprintln!(
                "[screening:diag] orchestrator: TIMEOUT after {timeout_secs}s (type={request_type:?})"
            );
            AppError::Import(format!(
                "LLM request timed out after {timeout_secs} seconds. This is often caused by                  sustained rate limiting (429), server overload (5xx), or a slow model.                  Try reducing batch_size or increasing request_delay_ms in LLM settings."
            ))
        })?;
        eprintln!(
            "[screening:diag] orchestrator: LLM call END type={request_type:?} elapsed={}ms",
            call_start.elapsed().as_millis()
        );

        // 4. Unpack the 3-tuple (content, tokens, CallMeta), persist the
        //    temperature flag if the client recovered from a rejection, and
        //    log errors centrally. Persistence is best-effort + detached; see
        //    `maybe_persist_skip_temperature`.
        let result = result.map(|(content, tokens, meta)| {
            self.maybe_persist_skip_temperature(meta);
            (content, tokens)
        });

        // 5. Log errors centrally
        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] {:?} request failed: {}", request_type, e);
        }

        result
    }

    /// Send a chat completion whose response is expected to be JSON, and run the
    /// shared JSON pre-parser on the result.
    ///
    /// This is the recommended entry point for any caller that will feed the
    /// LLM response into `serde_json::from_str`. It chains:
    /// 1. [`send`](Self::send) (concurrency + rate limit + timeout).
    /// 2. [`utils::json_repair::prepare_llm_json`] — strips markdown code fences
    ///    and escapes raw control characters (`0x00`–`0x1F`) that the LLM may
    ///    have placed inside JSON string values, so `serde_json` accepts the
    ///    payload. Non-destructive: a clean JSON response passes through
    ///    byte-identical.
    ///
    /// Callers that expect Markdown / plain text (chat, wiki chat, literature
    /// review, wiki ingest, markdown-fallback retry) MUST use [`send`](Self::send)
    /// instead — running the JSON pre-parser on prose would corrupt quoted spans.
    ///
    /// The returned `String` is the prepared JSON (ready for `serde_json::from_str`).
    /// The `usize` is the prompt+completion token total (unchanged from `send`).
    pub async fn send_json(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        let (raw, tokens) = self.send(config, system_prompt, user_prompt, request_type).await?;
        Ok((prepare_llm_json(&raw), tokens))
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

        let timeout = timeout_for(&request_type);
        let timeout_secs = timeout.as_secs();
        let result = tokio::time::timeout(
            timeout,
            client::send_chat_completion(config, system_prompt, user_prompt),
        )
        .await
        .map_err(|_| {
            AppError::Import(format!(
                "LLM request timed out after {timeout_secs} seconds. This is often caused by                  sustained rate limiting (429), server overload (5xx), or a slow model."
            ))
        })?;

        // Unpack the 3-tuple; the unthrottled path intentionally does NOT
        // persist the temperature flag - the sole unthrottled caller is
        // `test_connection`, which is driven by `test_llm_connection` and that
        // command already owns its own temperature-recovery + persistence
        // (`commands/llm_config.rs`). Persisting here would double-handle.
        let result = result.map(|(content, tokens, _meta)| (content, tokens));

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
    ///
    /// Temperature-recovery still applies (the client retries without
    /// `temperature` on a rejection 400), but the flag is NOT persisted here -
    /// the caller (`test_llm_connection` command) owns its own persistence so it
    /// can show the user the "temperature not supported - auto-adjusted" toast.
    pub async fn test_connection(
        &self,
        config: &LlmConfig,
    ) -> Result<(String, usize, client::CallMeta), AppError> {
        let (content, tokens, meta) = tokio::time::timeout(
            Duration::from_secs(30), // shorter timeout for connection test
            crate::llm::client::send_chat_completion(config, "You are a test.", "Say hello."),
        )
        .await
        .map_err(|_| {
            AppError::Import("LLM connection test timed out after 30 seconds".to_string())
        })??;
        // Latch the in-session flag when Test Connection discovers a temperature
        // rejection, so the very first screening/chat/summary call in the same
        // session skips temperature from the start. Without this, the client
        // recovery would silently swallow the 400 (returning Ok), and
        // `test_llm_connection` would report success without persisting the
        // flag - leaving screening batch 1 to rediscover the rejection.
        // The DB persistence is owned by `test_llm_connection` (it saves the
        // full config row so it can show the auto-adjusted toast); here we only
        // flip the orchestrator's in-memory latch.
        if meta.temperature_was_rejected {
            self.temperature_rejected_in_session.store(true, Ordering::Relaxed);
        }
        Ok((content, tokens, meta))
    }

    /// List available models for a provider's discovery endpoint.
    ///
    /// Routes the metadata/discovery call through the orchestrator so it
    /// participates in concurrency limiting (`max_concurrent_requests`) and
    /// rate limiting (`request_delay_ms`) just like chat-completion calls.
    /// This matters because some providers (e.g. OpenAI) count `/models`
    /// requests against the same rate-limit budget as completions.
    pub async fn list_models(
        &self,
        provider: &crate::models::llm_config::LlmProvider,
        endpoint_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, AppError> {
        // Acquire semaphore permit (waits if at concurrency limit).
        let sem = self.semaphore.read().unwrap_or_else(|p| p.into_inner()).clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| AppError::Import("LLM orchestrator closed".to_string()))?;

        // Rate limiting: ensure minimum delay between requests.
        self.enforce_rate_limit().await;

        let result = tokio::time::timeout(
            Duration::from_secs(30),
            client::list_models(provider, endpoint_url, api_key),
        )
        .await
        .map_err(|_| {
            AppError::Import("LLM list-models request timed out after 30 seconds".to_string())
        })?;

        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] list-models request failed: {e}");
        }

        result
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
