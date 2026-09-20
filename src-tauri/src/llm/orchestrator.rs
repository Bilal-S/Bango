//! Centralized LLM request coordinator.
//!
//! All LLM calls MUST go through `LlmOrchestrator` (Tauri managed state).
//! Enforces: concurrency limits, rate limiting, unified error logging, and
//! post-call `skip_temperature` persistence after temperature-rejection recovery.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::embedding::batching::group_into_embedding_batches;
use crate::embedding::text::{pool_vectors, split_text_by_token_budget, TextPiece};
use crate::error::AppError;
use crate::llm::client;
use crate::llm::embedding::embedding_limits;
use crate::models::llm_config::LlmConfig;
use crate::utils::json_repair::prepare_llm_json;

/// Best-effort callback to persist `skip_temperature` after client temperature-rejection recovery.
///
/// Implementations run `llm_config_repo::set_skip_temperature` under a DB lock.
/// INVARIANT: never invoked while any caller holds the DB lock (every orchestrator
/// call site releases its lock before calling `orchestrator.send` per spec §8.1
/// "lock-release-call-lock"), so post-call lock acquisition is provably deadlock-free.
///
/// Trait decouples orchestrator from `tauri::AppHandle` + `DbState` for testability.
pub trait TemperatureFlagPersister: Send + Sync {
    /// Persist `skip_temperature = skip`. Best-effort: implementations log and
    /// swallow errors so a DB failure never fails a successful LLM call.
    fn persist(&self, skip: bool);
}

/// Live local config provider (Bango AI). Production wraps the engine and
/// starts it on demand; tests inject stubs. Returns `None` when the local
/// backend is not active.
pub trait LocalConfigProvider: Send + Sync {
    fn effective_local_config<'a>(
        &'a self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<LlmConfig>, AppError>> + Send + 'a>,
    >;

    /// Whether the prose "extended reasoning" toggle is on.
    fn reasoning_enabled(&self) -> bool {
        false
    }

    /// In-flight accounting hook (engine busy state + reset drain).
    fn note_request_start(&self) {}

    /// In-flight accounting hook; must balance every `note_request_start`.
    fn note_request_end(&self) {}
}

/// Local requests get a much longer wall-clock budget: many CPUs are slow, and
/// the pinned on-device model can exceed every cloud per-type cap (screening
/// 120 s, split 60 s). 60 minutes; per-call `max_tokens` caps bound the actual
/// generation, and engine teardown uses a separate shorter bound.
pub const LOCAL_TIMEOUT_SECS: u64 = 3600;

/// Wall-clock timeout for one call: the single local override or the cloud
/// per-request-type value.
#[must_use]
pub fn resolve_timeout(request_type: &LlmRequestType, local: bool) -> Duration {
    if local {
        Duration::from_secs(LOCAL_TIMEOUT_SECS)
    } else {
        timeout_for(request_type)
    }
}

/// Wire options for one call: structured calls force thinking off; prose
/// calls honor the "extended reasoning" toggle (off = force thinking off,
/// on = leave the server default).
#[must_use]
pub fn local_request_options(json_mode: bool, reasoning_enabled: bool) -> client::RequestOptions {
    client::RequestOptions {
        json_mode,
        enable_thinking: if json_mode || !reasoning_enabled { Some(false) } else { None },
        max_tokens: None,
    }
}

/// Output cap for local (Bango AI) calls, per request type. Bounds runaway
/// no-EOS generations on the pinned CPU model; a hit is surfaced through
/// `CallMeta::truncated_by_output_budget()` so callers can handle truncation
/// (the wiki ingest drops the partial trailing page and re-dispatches).
/// `None` = server default (no cap). Cloud paths never see this field.
///
/// WikiIngest gets the largest cap so 2-3 sources share one prompt prefix
/// (`wiki_batch_output_budget` in `wiki/ingest/batching.rs` sizes batches for
/// it); chat gets 4096 because it is interactive; classification calls that
/// emit bounded per-item payloads keep 2048.
#[must_use]
pub fn local_max_tokens_for(request_type: &LlmRequestType) -> Option<u32> {
    match request_type {
        // Large page-set output; sized for ~2 sources per batch.
        LlmRequestType::WikiIngest => Some(8192),
        // Corpus-wide / multi-item reports and classifications.
        LlmRequestType::GapAnalysis | LlmRequestType::CitationFinder => Some(4096),
        // Interactive prose: the user is present and can stop the request.
        LlmRequestType::Chat | LlmRequestType::WikiChat => Some(4096),
        // Multi-section JSON summaries are the next largest outputs.
        LlmRequestType::AiSummary
        | LlmRequestType::ArticleSummary
        | LlmRequestType::SummaryGeneration
        | LlmRequestType::SectionSummary
        | LlmRequestType::UnifiedSummary
        | LlmRequestType::ClusterThematicAnalysis => Some(3072),
        // Bounded structured/classification/extraction calls.
        LlmRequestType::Screening
        | LlmRequestType::EnhancedScreening
        | LlmRequestType::TagGeneration
        | LlmRequestType::LabelGeneration
        | LlmRequestType::CriteriaGeneration
        | LlmRequestType::TestConnection
        | LlmRequestType::FigureDescription
        | LlmRequestType::Translation
        | LlmRequestType::SearchStrategy
        | LlmRequestType::OpenAlexSmartSearch
        | LlmRequestType::CitationFinderSplit => Some(2048),
        // Embeddings never use the chat path; no cap applies.
        LlmRequestType::Embedding => None,
    }
}

/// Maximum time to wait for a single LLM response (default for all request
/// types except screening).
const LLM_TIMEOUT_SECS: u64 = 600;

/// Timeout for screening LLM calls (stage-1 abstract + stage-2 enhanced/two-stage).
/// Tighter than `LLM_TIMEOUT_SECS` because screening prompts are small and high-volume;
/// surfaces hung calls in ~2 min. Mitigate slowness by lowering `batch_size`.
const SCREENING_TIMEOUT_SECS: u64 = 120;

/// Timeout for embedding requests (generation or recall). 30s cap surfaces a
/// hung provider promptly. Inner `client::send_with_retry` handles transients.
const EMBEDDING_TIMEOUT_SECS: u64 = 30;

/// Timeout for Citation Finder main classification (`citation_finder/AGENTS.md`).
/// 120s surfaces a hung provider while leaving room for slow local models.
const CITATION_FINDER_TIMEOUT_SECS: u64 = 120;

/// Timeout for Citation Finder claim-split (per-statement mode). 60s allows for
/// slow local model first-call (model load).
const CITATION_FINDER_SPLIT_TIMEOUT_SECS: u64 = 60;

/// Per-`LlmRequestType` wall-clock timeout. Pure for unit-test isolation.
#[must_use]
pub fn timeout_for(request_type: &LlmRequestType) -> Duration {
    match request_type {
        LlmRequestType::Screening | LlmRequestType::EnhancedScreening => {
            Duration::from_secs(SCREENING_TIMEOUT_SECS)
        }
        LlmRequestType::Embedding => Duration::from_secs(EMBEDDING_TIMEOUT_SECS),
        LlmRequestType::CitationFinder => Duration::from_secs(CITATION_FINDER_TIMEOUT_SECS),
        LlmRequestType::CitationFinderSplit => {
            Duration::from_secs(CITATION_FINDER_SPLIT_TIMEOUT_SECS)
        }
        _ => Duration::from_secs(LLM_TIMEOUT_SECS),
    }
}

/// Labels for categorizing LLM request sources. Used by [`timeout_for`] for per-type
/// timeouts and by diagnostics for per-call-type traffic/cost tracking.
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
    /// T1.3: per-section Methods/Results/Discussion summary alongside whole-paper summary.
    SectionSummary,
    /// Tier 2 Phase 4: batched LLM description of figure/table captions.
    FigureDescription,
    /// Tier 3: stage-2 enhanced/two-stage screening (abstract + full-text chunks).
    EnhancedScreening,
    /// Tier 4.2: unified AI summary combining per-section + figure descriptions.
    UnifiedSummary,
    /// Multilingual translation: per-chunk/metadata English rewrite.
    Translation,
    /// Research Gap Analysis: corpus-wide Markdown report over included articles.
    GapAnalysis,
    /// Search Strategy Builder: generates database-ready Boolean queries from aims + criteria.
    SearchStrategy,
    /// OpenAlex Smart Search: generates OpenAlex Boolean query from aims + criteria.
    OpenAlexSmartSearch,
    /// Embedding generation / recall. Does NOT participate in `skip_temperature` machinery.
    Embedding,
    /// Citation Finder main classification: rank + classify + explain.
    CitationFinder,
    /// Citation Finder claim-split (per-statement mode): ≤5 distinct claims.
    CitationFinderSplit,
    /// Cluster thematic analysis: shared themes of one bibliometric cluster.
    ClusterThematicAnalysis,
}

/// Centralized LLM request coordinator (Tauri managed state).
///
/// Concurrency: `tokio::sync::Semaphore` in `RwLock<Arc<Semaphore>>`. Since tokio's
/// Semaphore only grows, `update_settings` swaps in a fresh one. `send()` clones
/// the Arc under a brief read lock and drops it before `.await`, so settings
/// updates are never blocked.
pub struct LlmOrchestrator {
    semaphore: RwLock<Arc<Semaphore>>,
    last_request: Arc<tokio::sync::Mutex<Instant>>,
    request_delay_ms: Arc<tokio::sync::Mutex<u64>>,
    /// Best-effort `skip_temperature` persister. `None` in tests; `Some` in production.
    temp_persister: RwLock<Option<Arc<dyn TemperatureFlagPersister>>>,
    /// In-session latch: flips to `true` on first temperature-rejection recovery
    /// so subsequent calls omit `temperature` from the start. Lock-free; complements
    /// DB persistence for in-memory config caches.
    temperature_rejected_in_session: AtomicBool,
    /// Active generation backend (T6). Local requests route through
    /// `local_provider` and never persist cloud temperature flags.
    backend: RwLock<crate::llm::backend::LlmBackend>,
    /// Live local config provider, wired at startup (`Arc<BangoAiEngine>`).
    local_provider: RwLock<Option<Arc<dyn LocalConfigProvider>>>,
}

/// Releases the local in-flight slot on drop so every exit path (success,
/// error, timeout) balances `note_request_start`.
struct InFlightGuard {
    provider: Option<Arc<dyn LocalConfigProvider>>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.provider {
            provider.note_request_end();
        }
    }
}

/// No-op persister used when no DB is available (tests, or pre-wiring).
/// Drops the flag silently; next call retries temperature-recovery if the model still rejects.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpTemperaturePersister;

impl TemperatureFlagPersister for NoOpTemperaturePersister {
    fn persist(&self, _skip: bool) {
        // Intentionally no-op.
    }
}

impl LlmOrchestrator {
    /// Create a new orchestrator. Temperature persister starts as `None`;
    /// production wires it via [`set_temperature_persister`](Self::set_temperature_persister).
    pub fn new(max_concurrent: usize, request_delay_ms: u64) -> Self {
        Self {
            semaphore: RwLock::new(Arc::new(Semaphore::new(max_concurrent.max(1)))),
            last_request: Arc::new(tokio::sync::Mutex::new(
                Instant::now() - Duration::from_millis(request_delay_ms.max(1)),
            )),
            request_delay_ms: Arc::new(tokio::sync::Mutex::new(request_delay_ms)),
            temp_persister: RwLock::new(None),
            temperature_rejected_in_session: AtomicBool::new(false),
            backend: RwLock::new(crate::llm::backend::LlmBackend::default()),
            local_provider: RwLock::new(None),
        }
    }

    /// The active generation backend.
    #[must_use]
    pub fn backend(&self) -> crate::llm::backend::LlmBackend {
        *self.backend.read().unwrap_or_else(|p| p.into_inner())
    }

    /// Wire the live local config provider (production: engine-backed).
    pub fn set_local_config_provider(&self, provider: Arc<dyn LocalConfigProvider>) {
        *self.local_provider.write().unwrap_or_else(|p| p.into_inner()) = Some(provider);
    }

    /// Startup-only synchronous backend set: no requests are in flight yet
    /// and the semaphore was sized for this backend at construction.
    pub fn set_backend_initial(&self, backend: crate::llm::backend::LlmBackend) {
        *self.backend.write().unwrap_or_else(|p| p.into_inner()) = backend;
    }

    /// Switch the generation backend and reconfigure the semaphore/delay:
    /// local calls run serialized (concurrency 1, no pacing); switching back
    /// restores the stored cloud limits.
    pub async fn set_backend(
        &self,
        backend: crate::llm::backend::LlmBackend,
        stored: Option<&LlmConfig>,
    ) {
        *self.backend.write().unwrap_or_else(|p| p.into_inner()) = backend;
        match backend {
            crate::llm::backend::LlmBackend::BangoAi => self.update_settings(1, 0).await,
            crate::llm::backend::LlmBackend::ConfiguredProvider => {
                let max = stored.map_or(3, |c| c.max_concurrent_requests.max(1) as usize);
                let delay = stored.map_or(500, |c| c.request_delay_ms.max(0) as u64);
                self.update_settings(max, delay).await;
            }
        }
    }

    /// Wire the best-effort `skip_temperature` persister (production only).
    /// Called once during startup after `DbState` is available.
    pub fn set_temperature_persister(&self, persister: Arc<dyn TemperatureFlagPersister>) {
        *self.temp_persister.write().unwrap_or_else(|p| p.into_inner()) = Some(persister);
    }

    /// If `meta.temperature_was_rejected`, latch in-session flag + spawn detached
    /// task to persist `skip_temperature = true`. Best-effort; DB failures logged.
    ///
    /// INVARIANT: runs AFTER the LLM call returns. Every caller releases its DB lock
    /// before invoking the orchestrator, so the persister's lock acquisition is
    /// provably deadlock-free.
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
    /// Swaps in a fresh semaphore of the exact requested size (handles both grow
    /// and shrink). In-flight requests holding the old semaphore are unaffected.
    /// A lower limit takes effect for new acquisitions only - brief transient
    /// acceptable for an advisory rate limiter.
    pub async fn update_settings(&self, max_concurrent: usize, request_delay_ms: u64) {
        let new_sem = Arc::new(Semaphore::new(max_concurrent.max(1)));
        // Recover from poison (a panicked lock holder) rather than unwinding:
        // an `Arc<Semaphore>` is never left structurally inconsistent by a panic.
        *self.semaphore.write().unwrap_or_else(|p| p.into_inner()) = new_sem;
        *self.request_delay_ms.lock().await = request_delay_ms;
    }

    /// Send a chat completion request through the orchestrator with explicit
    /// wire options (`json_mode`). Enforces concurrency limits + rate limiting
    /// + per-request-type timeout.
    pub async fn send_with_meta_opts(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
        json_mode: bool,
    ) -> Result<(String, usize, client::CallMeta), AppError> {
        let local = self.backend() == crate::llm::backend::LlmBackend::BangoAi;
        let provider = if local {
            self.local_provider.read().unwrap_or_else(|p| p.into_inner()).clone()
        } else {
            None
        };
        // Local in-flight accounting (engine busy state + reset drain): the
        // request counts from before the local config resolution AND the
        // permit wait, so a cold start is visible to `reset_off_thread` too
        // (queued and starting requests keep the server alive) until the guard
        // drops on any exit (aifixes1 F7).
        let _in_flight_guard = InFlightGuard { provider: provider.clone() };
        if let Some(in_flight) = &_in_flight_guard.provider {
            in_flight.note_request_start();
        }
        let local_config: Option<LlmConfig> = if local {
            let Some(provider) = &provider else {
                return Err(AppError::Import(
                    "Bango AI is selected but the local engine is not available. Restart the app \
                     or reinstall Bango AI."
                        .to_string(),
                ));
            };
            let Some(config) = provider.effective_local_config().await? else {
                return Err(AppError::Import(
                    "Bango AI is selected but its components are not ready. Set up Bango AI in \
                     Settings or switch back to the configured provider."
                        .to_string(),
                ));
            };
            Some(config)
        } else {
            None
        };
        let reasoning_enabled = provider.as_ref().is_some_and(|p| p.reasoning_enabled());
        let options = if local {
            let mut options = local_request_options(json_mode, reasoning_enabled);
            options.max_tokens = local_max_tokens_for(&request_type);
            /* Thinking tokens count against `max_tokens`. Structured calls
            force thinking off; prose with the reasoning toggle ON leaves
            thinking enabled, so double the headroom there. */
            if options.enable_thinking.is_none() {
                options.max_tokens = options.max_tokens.map(|cap| cap.saturating_mul(2));
            }
            options
        } else {
            client::RequestOptions::default()
        };
        let config_ref = local_config.as_ref().unwrap_or(config);

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

        /* If a prior CLOUD call in this session discovered temperature
        rejection, clone config with skip_temperature = true so this call omits
        the parameter from the start. Local calls neither consult nor set the
        latch: it belongs to the cloud row (decision: no local persistence). */
        let effective_config: LlmConfig;
        let config_ref = if !local
            && self.temperature_rejected_in_session.load(Ordering::Relaxed)
            && !config_ref.skip_temperature
        {
            effective_config = {
                let mut c = config_ref.clone();
                c.skip_temperature = true;
                c
            };
            &effective_config
        } else {
            config_ref
        };

        // 4. Make the actual LLM call with a per-request-type timeout (local
        // requests use the single 3600 s local override).
        let timeout = resolve_timeout(&request_type, local);
        let timeout_secs = timeout.as_secs();
        eprintln!(
            "[screening:diag] orchestrator: LLM call START type={request_type:?} timeout={timeout_secs}s"
        );
        let call_start = std::time::Instant::now();
        let result = tokio::time::timeout(
            timeout,
            client::send_chat_completion_with_options(config_ref, system_prompt, user_prompt, options),
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

        /* Unpack the (content, tokens, CallMeta) 3-tuple, persist the cloud
        temperature flag on recovery (never for local calls), and centralize
        error logging. */
        let result = result.map(|(content, tokens, meta)| {
            if !local {
                self.maybe_persist_skip_temperature(meta.clone());
            }
            (content, tokens, meta)
        });

        // 5. Log errors centrally
        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] {:?} request failed: {}", request_type, e);
        }

        result
    }

    /// Default-options wrapper (cloud-compatible behavior).
    pub async fn send_with_meta(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize, client::CallMeta), AppError> {
        self.send_with_meta_opts(config, system_prompt, user_prompt, request_type, false).await
    }

    /// Prose-call wrapper around [`Self::send_with_meta`] that drops the
    /// `CallMeta` side-channel (pre-existing caller contract).
    pub async fn send(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        self.send_opts(config, system_prompt, user_prompt, request_type, false).await
    }

    /// Option-carrying wrapper that drops the `CallMeta` side-channel. The
    /// screening client uses this with `json_mode = true` and runs its own
    /// array-aware `extract_json` (no double `prepare_llm_json` pass).
    pub async fn send_opts(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
        json_mode: bool,
    ) -> Result<(String, usize), AppError> {
        let (content, tokens, _meta) = self
            .send_with_meta_opts(config, system_prompt, user_prompt, request_type, json_mode)
            .await?;
        Ok((content, tokens))
    }

    /// Send a chat completion whose response is expected to be JSON. Chains
    /// the call with `prepare_llm_json` (strips code fences, escapes
    /// raw control chars in strings). Returned `String` is ready for `serde_json::from_str`.
    ///
    /// JSON-returning callers (summaries, criteria, smart search, etc.) MUST use
    /// this. Prose callers (chat, wiki, translation, etc.) MUST use [`send`](Self::send).
    pub async fn send_json(
        &self,
        config: &LlmConfig,
        system_prompt: &str,
        user_prompt: &str,
        request_type: LlmRequestType,
    ) -> Result<(String, usize), AppError> {
        let (raw, tokens) =
            self.send_opts(config, system_prompt, user_prompt, request_type, true).await?;
        Ok((prepare_llm_json(&raw), tokens))
    }

    /// Send an embedding request through the orchestrator.
    ///
    /// Enforces concurrency + rate limiting + 30s `Embedding` timeout. Delegates
    /// to `llm::embedding::embed_texts`. Does NOT participate in `skip_temperature`
    /// machinery. Callers MUST release DB lock before invoking (same lock-release-
    /// call-lock discipline as `send`).
    pub async fn send_embedding(
        &self,
        config: &LlmConfig,
        texts: &[String],
        model: &str,
    ) -> Result<(Vec<Vec<f32>>, i32), AppError> {
        let sem = self.semaphore.read().unwrap_or_else(|p| p.into_inner()).clone();
        let _permit = sem
            .acquire()
            .await
            .map_err(|_| AppError::Import("LLM orchestrator closed".to_string()))?;

        self.enforce_rate_limit().await;

        let timeout = timeout_for(&LlmRequestType::Embedding);
        let result =
            tokio::time::timeout(timeout, crate::llm::embedding::embed_texts(config, texts, model))
                .await
                .map_err(|_| {
                    AppError::Import(format!(
                        "Embedding request timed out after {} seconds.",
                        timeout.as_secs()
                    ))
                })?;

        if let Err(ref e) = result {
            eprintln!("[LlmOrchestrator] Embedding request failed: {e}");
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

    /// Test an LLM connection with a simple "hello" prompt. Does NOT go through
    /// the semaphore. Temperature-recovery still applies; in-session flag is latched
    /// on recovery. DB persistence is owned by the caller (`test_llm_connection`).
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
        // flip the orchestrator's in-memory latch. Local (Bango AI) configs
        // never latch: the flag belongs to the stored cloud row (aifixes1 F14).
        if meta.temperature_was_rejected
            && config.provider != crate::models::llm_config::LlmProvider::BangoAi
        {
            self.temperature_rejected_in_session.store(true, Ordering::Relaxed);
        }
        Ok((content, tokens, meta))
    }

    /// List available models via the provider's discovery endpoint. Routes through
    /// the semaphore + rate limit since some providers count `/models` against the
    /// same budget as completions.
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

    /// Current semaphore permits (for diagnostics). May briefly under-report in-flight
    /// work during the `update_settings` swap transient window.
    pub fn available_permits(&self) -> usize {
        self.semaphore.read().unwrap_or_else(|p| p.into_inner()).available_permits()
    }

    /* `send_batch_parallel` and `send_embedding_batch_parallel` are FREE
    functions (not `&self` methods) because `JoinSet::spawn` requires
    `'static` futures. Embedding variant takes `&Arc<Self>` so the inner
    closure can `Arc::clone` the orchestrator into each spawned task. */
}

// ── v2 free functions: generic parallel dispatch ────────────────────────────

/// Dispatch independent work units in parallel via `JoinSet`, reassembling in
/// INPUT ORDER. Generic over batch/result/closure/future types.
///
/// Contracts: order-preserving (result `i` = `batches[i]`), per-batch failures
/// tolerated, panic isolation (slot filled with `Err`, not abort-all), no
/// cancellation at this layer, no semaphore (caller's `work_fn` acquires its own).
/// Used by [`send_embedding_batch_parallel`] and available for future callers.
pub async fn send_batch_parallel<B, R, F, Fut>(
    batches: Vec<B>,
    work_fn: F,
) -> Vec<Result<R, AppError>>
where
    B: Send + 'static,
    R: Send + 'static,
    F: Fn(B) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<R, AppError>> + Send,
{
    let total = batches.len();
    let mut slots: Vec<Option<Result<R, AppError>>> = (0..total).map(|_| None).collect();
    let work_fn = Arc::new(work_fn);

    let mut set: JoinSet<(usize, Result<R, AppError>)> = JoinSet::new();
    for (idx, batch) in batches.into_iter().enumerate() {
        let work_fn = Arc::clone(&work_fn);
        set.spawn(async move {
            let result = work_fn(batch).await;
            (idx, result)
        });
    }

    // Wait for every task; scatter each result into its indexed slot.
    // We poll until the JoinSet is empty so panics do not leave slots
    // unfilled (a panicked task's `join_next` yields `Result::Err`).
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((idx, result)) => {
                slots[idx] = Some(result);
            }
            Err(join_err) => {
                /* Task panicked or cancelled. Scan for first unfilled slot and mark
                as Err so the result vec stays full-length + ordered. */
                if let Some(slot) = slots.iter_mut().find(|s| s.is_none()) {
                    *slot = Some(Err(AppError::Import(format!("task panicked: {join_err}"))));
                }
            }
        }
    }

    // Every slot should be Some by now (we spawned one task per slot and
    // waited for all). Defense-in-depth: fill any stragglers.
    slots
        .into_iter()
        .map(|s| s.unwrap_or_else(|| Err(AppError::Import("task result missing".to_string()))))
        .collect()
}

// ── v2 free functions: embedding-specific batch parallel dispatch ───────────

/// Embed input texts, returning one vector per input in input order. Arbitrary-length
/// texts handled losslessly via per-text splitting + mean-pooling.
///
/// Takes `&Arc<LlmOrchestrator>` so inner closure can `Arc::clone` into spawned tasks.
///
/// Pipeline:
/// 1. Resolve `embedding_limits` → 2. Split each text by `max_tokens_per_input` →
/// 3. Flatten into `(input_idx, TextPiece)` → 4. Group into sub-batches respecting
///    BOTH caps → 5-6. Parallel HTTP via `send_batch_parallel` → 7. Scatter + pool →
///    8. Return `(ordered Vec<Vec<f32>>, effective_dim)`.
///
/// `effective_dim` taken from first non-empty vector; caller validates rest match.
pub async fn send_embedding_batch_parallel(
    orchestrator: &Arc<LlmOrchestrator>,
    config: &LlmConfig,
    texts: &[String],
    model: &str,
) -> Result<(Vec<Vec<f32>>, i32), AppError> {
    if texts.is_empty() {
        return Err(AppError::Validation("No texts to embed".to_string()));
    }

    let limits = embedding_limits(&config.provider, model);

    /* Split each text into pieces fitting `max_tokens_per_input`. Record per-piece
    token counts for pooling. */
    let split: Vec<Vec<TextPiece>> =
        texts.iter().map(|t| split_text_by_token_budget(t, limits.max_tokens_per_input)).collect();

    // 3. Flatten into (input_idx, TextPiece) pairs.
    let mut flat: Vec<(usize, TextPiece)> = Vec::new();
    for (input_idx, pieces) in split.into_iter().enumerate() {
        for piece in pieces {
            flat.push((input_idx, piece));
        }
    }

    // 4. Group into sub-batches respecting both caps.
    let sub_batches = group_into_embedding_batches(flat, &limits);

    /* Strip indices for HTTP dispatch; remember them for reassembly. Clone the
    orchestrator Arc so each spawned task is `'static` + self-contained. */
    let mut dispatch_batches: Vec<(Vec<usize>, Vec<String>, Vec<usize>)> = Vec::new();
    for sub in sub_batches {
        let (indices, pieces): (Vec<usize>, Vec<TextPiece>) = sub.into_iter().unzip();
        let token_counts: Vec<usize> = pieces.iter().map(|p| p.token_count).collect();
        let strings: Vec<String> = pieces.into_iter().map(|p| p.text).collect();
        dispatch_batches.push((indices, strings, token_counts));
    }

    // Clone orchestrator Arc so the `'static` closure captures it by value.
    let orch_owned = Arc::clone(orchestrator);
    let config_arc = Arc::new(config.clone());
    let model_arc = Arc::new(model.to_string());

    let results = send_batch_parallel(dispatch_batches, move |(indices, strings, _tokens)| {
        let orch = Arc::clone(&orch_owned);
        let config_arc = Arc::clone(&config_arc);
        let model_arc = Arc::clone(&model_arc);
        async move {
            let (vectors, _dim) = orch.send_embedding(&config_arc, &strings, &model_arc).await?;
            Ok::<_, AppError>((indices, vectors))
        }
    })
    .await;

    /* Scatter per-batch vectors into per-input slots. Collect per-piece weights
    for `pool_vectors` over split inputs. */
    let mut slots: Vec<Vec<Vec<f32>>> = vec![Vec::new(); texts.len()];
    let mut slot_weights: Vec<Vec<usize>> = vec![Vec::new(); texts.len()];
    let mut effective_dim: i32 = 0;
    for result in results {
        let (indices, vectors) = result?;
        for (slot_idx, vec) in indices.into_iter().zip(vectors) {
            if effective_dim == 0 && !vec.is_empty() {
                effective_dim = vec.len() as i32;
            }
            if slot_idx < slots.len() {
                slots[slot_idx].push(vec);
                /* Uniform weight fallback: per-piece token counts aren't carried
                through HTTP. Uniform weighting produces a stable L2-normalized
                direction for cosine recall. Token-weighted means require
                threading counts through the sub-batch response (rare path). */
                slot_weights[slot_idx].push(1);
            }
        }
    }

    let pooled: Vec<Vec<f32>> = slots
        .iter()
        .zip(slot_weights.iter())
        .map(
            |(pieces, weights)| {
                if pieces.is_empty() {
                    Vec::new()
                } else {
                    pool_vectors(pieces, weights)
                }
            },
        )
        .collect();

    Ok((pooled, effective_dim))
}
