//! Screening engine: batch loop + delegate stage-2 borderline.
//!
//! - `types.rs` - `ScreeningConfig`, `RunSyncContext`, `ScreeningProgress`, `LlmScreeningResponse`
//! - `prompt_parts.rs` - `ScreeningPromptParts` + `Stage2Context` (shared prompt construction)
//! - `stage2.rs` - `run_stage2_borderline` + `is_borderline` predicate
//! - `mod.rs` - `ScreeningEngine` struct, `run_sync`, helpers, re-exports

use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::db::{
    app_settings_repo::{self, ScreeningMode},
    article_repo, audit_repo, label_repo, tag_repo,
};
use crate::error::AppError;
use crate::llm::orchestrator::LlmRequestType;
use crate::models::criterion::{Criterion, CriterionType, ResearchAim};
use crate::screening::article_writer::{
    mark_batch_screening_error, set_screening_error, write_article_screening_result,
};
use crate::screening::chunk_retrieval::ScoredChunk;
use crate::screening::decision::resolve_article_decision;
use crate::screening::error_classify::classify_llm_error;
use crate::screening::llm_client::LlmClient;
use crate::screening::prompt::{self, ArticleEntry};

mod prompt_parts;
mod stage2;
mod types;

/* Always-on diagnostic logger: fires in release builds for phase/cancel/mutex tracing.
Capturable via `Bango 2>screening.log`. Prefix `[screening:diag]`, no PII. */
pub(crate) fn log_diag(args: std::fmt::Arguments<'_>) {
    eprintln!("[screening:diag] {}", args);
}

/// Ergonomic macro wrapper for `log_diag` (mod.rs call sites).
macro_rules! log_diag_macro {
    ($($arg:tt)*) => {
        $crate::screening::engine::log_diag(format_args!($($arg)*))
    };
}
use log_diag_macro as log_diag;

use prompt_parts::{ScreeningPromptParts, Stage2Context};

pub use types::{LlmScreeningResponse, RunSyncContext, ScreeningConfig, ScreeningProgress};

pub struct ScreeningEngine {
    pub(crate) progress: Arc<Mutex<ScreeningProgress>>,
    pub(crate) cancel_token: Arc<Mutex<bool>>,
    /* One-shot cancellation: `notify_waiters()` wakes `select!` branches awaiting
    `notified()`, aborting the in-flight LLM call within milliseconds. */
    pub(crate) cancel_notify: Arc<tokio::sync::Notify>,
    pub(crate) pause_token: Arc<Mutex<bool>>,
    batch_size: usize,
}

impl Default for ScreeningEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreeningEngine {
    /// Create engine with default batch size 1.
    pub fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(ScreeningProgress::default())),
            cancel_token: Arc::new(Mutex::new(false)),
            cancel_notify: Arc::new(tokio::sync::Notify::new()),
            pause_token: Arc::new(Mutex::new(false)),
            batch_size: 1,
        }
    }

    /// Create engine with specific batch size (min 1).
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self {
            progress: Arc::new(Mutex::new(ScreeningProgress::default())),
            cancel_token: Arc::new(Mutex::new(false)),
            cancel_notify: Arc::new(tokio::sync::Notify::new()),
            pause_token: Arc::new(Mutex::new(false)),
            batch_size: batch_size.max(1),
        }
    }

    pub async fn get_progress(&self) -> ScreeningProgress {
        self.progress.lock().await.clone()
    }

    pub async fn cancel(&self) {
        log_diag!("cancel_engine: cancel_token=true, notify_waiters()");
        *self.cancel_token.lock().await = true;
        // Wake any `select!` branch awaiting cancellation so the in-flight LLM
        // call is dropped (and its reqwest request cancelled) immediately.
        self.cancel_notify.notify_waiters();
    }

    pub async fn pause(&self) {
        *self.pause_token.lock().await = true;
    }

    pub async fn resume(&self) {
        *self.pause_token.lock().await = false;
    }

    /// Sleep `delay_ms`, aborting immediately if cancelled via `cancel_notify`.
    /// Returns `true` if aborted (caller returns immediately); `false` if completed.
    ///
    /// **Cancel contract**: always poll `notified()`, check token inside branch -
    /// never use `if` precondition (unregistered waiter = lost signal).
    pub(super) async fn delay_or_cancel(
        &self,
        app_handle: &Option<tauri::AppHandle>,
        delay_ms: u64,
    ) -> bool {
        if delay_ms == 0 {
            return false;
        }
        let cancel_notify = self.cancel_notify.clone();
        tokio::select! {
            biased;
            () = cancel_notify.notified() => {
                if *self.cancel_token.lock().await {
                    let mut progress = self.progress.lock().await;
                    progress.is_running = false;
                    progress.current_article_titles = vec![];
                    self.emit_progress(app_handle, &progress);
                    true
                } else {
                    // Spurious notify: complete normally.
                    false
                }
            }
            () = tokio::time::sleep(Duration::from_millis(delay_ms)) => false,
        }
    }

    /// Run screening. `config` selects abstract/enhanced/two-stage mode.
    /// `ctx.target_article_id = Some(id)` = single article; `None` = batch.
    /// Article must be `working` + unscreened, else no-op.
    pub async fn run_sync(
        &self,
        conn_mutex: &std::sync::Mutex<Connection>,
        llm: &dyn LlmClient,
        criteria: Vec<Criterion>,
        aims: Vec<ResearchAim>,
        config: ScreeningConfig,
        ctx: &RunSyncContext,
    ) -> Result<(), AppError> {
        let request_delay_ms = ctx.request_delay_ms;
        let app_handle = ctx.app_handle.clone();
        let target_article_id = ctx.target_article_id.clone();

        /* Reset state. `Notify` needs no reset — `notify_waiters()` only wakes
        current waiters; fresh `notified()` created per select!. */
        *self.cancel_token.lock().await = false;
        *self.pause_token.lock().await = false;

        let total = {
            let c = crate::db::connection::lock_conn(conn_mutex)?;
            article_repo::count_unscreened_working(&c)?
        };
        if total == 0 {
            return Ok(());
        }

        let effective_total = config.max_articles.map_or(total, |n| n.min(total));

        let inclusion_criteria: Vec<&Criterion> = criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Inclusion))
            .collect();
        let exclusion_criteria: Vec<&Criterion> = criteria
            .iter()
            .filter(|c| matches!(c.criterion_type, CriterionType::Exclusion))
            .collect();

        let global_numbering = crate::screening::decision::build_global_criterion_numbering(
            &inclusion_criteria,
            &exclusion_criteria,
        );

        // Fetch existing tags/labels + custom-logic text once per run.
        let (existing_tag_names, existing_label_names, custom_logic) = {
            let c = crate::db::connection::lock_conn(conn_mutex)?;
            let tags = tag_repo::get_all_tags(&c)?;
            let labels = label_repo::get_all_labels(&c)?;
            let logic = app_settings_repo::get_screening_custom_logic(&c)?;
            (
                tags.into_iter().map(|t| t.name).collect::<Vec<_>>(),
                labels.into_iter().map(|l| l.name).collect::<Vec<_>>(),
                logic,
            )
        };

        /* Custom-logic gate: when in force, LLM decision is final (combinatorial
        rules transcend the generic priority resolver). */
        let has_custom_logic =
            custom_logic.as_deref().map(str::trim).is_some_and(|text| !text.is_empty());

        let prompt_parts = ScreeningPromptParts::new(
            &inclusion_criteria,
            &exclusion_criteria,
            &aims,
            &global_numbering,
            existing_tag_names,
            existing_label_names,
            custom_logic,
        );
        let enhanced_mode = config.mode == ScreeningMode::Enhanced;
        let two_stage_mode = config.mode == ScreeningMode::TwoStage;

        if two_stage_mode {
            self.update_progress(&app_handle, |p| {
                p.stage = Some("Stage 1: abstract".to_string());
                p.stage_total = None;
            })
            .await;
        }

        // Initialize progress.
        {
            let mut progress = self.progress.lock().await;
            progress.total = effective_total;
            progress.completed = 0;
            progress.included = 0;
            progress.rejected = 0;
            progress.errors = 0;
            progress.is_running = true;
            progress.phase = Some("screening".to_string());
            log_diag!(
                "phase=screening total={effective_total} batch_size={} mode={:?}",
                self.batch_size,
                config.mode
            );
        }

        let start = Instant::now();

        /* Heartbeat: emits `[screening:diag] HEARTBEAT` every 5s; exits when
        run completes or is cancelled. */
        let hb_cancel = self.cancel_token.clone();
        let hb_progress = self.progress.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let p = hb_progress.lock().await;
                let is_running = p.is_running;
                let completed = p.completed;
                let total = p.total;
                let deferred = p.deferred;
                let errors = p.errors;
                let phase = p.phase.clone();
                drop(p);
                log_diag!(
                    "HEARTBEAT: {completed}/{total} completed, {deferred} deferred, {errors} errors, phase={:?}, is_running={is_running}",
                    phase.unwrap_or_else(|| "none".to_string())
                );
                if !is_running || *hb_cancel.lock().await {
                    break;
                }
            }
        });

        let mut last_attempted_seq: Option<i64> = None;
        let mut consecutive_transient_failures: u32 = 0;
        let mut total_timeouts: u32 = 0;

        loop {
            // Cancel/pause gate.
            if *self.cancel_token.lock().await {
                break;
            }
            while *self.pause_token.lock().await {
                sleep(Duration::from_millis(200)).await;
                if *self.cancel_token.lock().await {
                    break;
                }
            }
            if *self.cancel_token.lock().await {
                break;
            }

            // Fetch next batch. `target_article_id` = per-article path.
            let mut batch = {
                let c = crate::db::connection::lock_conn(conn_mutex)?;
                if let Some(ref target_id) = target_article_id {
                    match article_repo::get_unscreened_working_article_by_id(&c, target_id)? {
                        Some(article) => vec![article],
                        None => break,
                    }
                } else {
                    article_repo::get_next_unscreened_working_batch(
                        &c,
                        self.batch_size,
                        last_attempted_seq,
                    )?
                }
            };
            if batch.is_empty() {
                break;
            }

            /* Advance cursor past this batch so transient-deferred articles
            aren't re-fetched infinitely (reset per new run). */
            if target_article_id.is_none() {
                last_attempted_seq = batch.iter().map(|a| a.sequence_id).max();
            }

            // max_articles cap.
            if let Some(cap) = config.max_articles {
                let progress = self.progress.lock().await;
                let processed = progress.completed;
                drop(progress);
                if processed >= cap {
                    break;
                }
                let remaining = cap - processed;
                if remaining < batch.len() {
                    batch.truncate(remaining);
                }
            }

            // Emit current titles so the UI shows what's being screened.
            {
                let mut progress = self.progress.lock().await;
                progress.current_article_titles = batch.iter().map(|a| a.title.clone()).collect();
                let completed = progress.completed;
                let total_count = progress.total;
                log_diag!(
                    "batch_start: completed={completed}/{total_count}, batch_size={}, titles=[{:?}]",
                    batch.len(),
                    progress.current_article_titles.iter().take(3).collect::<Vec<_>>()
                );
                self.emit_progress(&app_handle, &progress);
            }

            /* Build article entries. Enhanced mode retrieves top-K chunks per
            article with `has_full_text`. Two-stage stage-1 is abstract-only. */
            let mut enhanced_evidence_labels: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();

            let article_entries: Vec<ArticleEntry> = {
                let mut entries: Vec<ArticleEntry> = batch
                    .iter()
                    .map(|a| ArticleEntry {
                        title: a.title.clone(),
                        authors: a.authors.join("; "),
                        year: a.publication_year,
                        abstract_text: a.abstract_text.clone(),
                        full_text_evidence: None,
                    })
                    .collect();
                if enhanced_mode {
                    let c = crate::db::connection::lock_conn(conn_mutex)?;
                    for (entry, article) in entries.iter_mut().zip(batch.iter()) {
                        if !article.has_full_text {
                            continue;
                        }
                        if let Some(ev) = crate::screening::evidence::retrieve_evidence_for_article(
                            &c,
                            &article.id,
                            &prompt_parts.inclusion_texts,
                            &prompt_parts.exclusion_texts,
                            &config,
                        ) {
                            entry.full_text_evidence = Some(ev.text);
                            enhanced_evidence_labels.insert(article.id.clone(), ev.sections_label);
                        }
                    }
                }
                entries
            };

            let prompt_input = prompt_parts.build_prompt_input(article_entries);
            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;
            let request_type = if enhanced_mode {
                LlmRequestType::EnhancedScreening
            } else {
                LlmRequestType::Screening
            };

            /* Stage-1 LLM call: single attempt (inner retry handles 429/408/5xx).
            Wrapped in `tokio::select!` against `cancel_notify` for instant Stop. */
            let llm_result = {
                let cancel_notify = self.cancel_notify.clone();
                loop {
                    tokio::select! {
                        biased;
                        () = cancel_notify.notified() => {
                            if *self.cancel_token.lock().await {
                                log_diag!("llm_call: cancel detected during stage-1, dropping response + returning");
                                let mut progress = self.progress.lock().await;
                                progress.is_running = false;
                                progress.current_article_titles = vec![];
                                self.emit_progress(&app_handle, &progress);
                                return Ok(());
                            }
                            continue;
                        }
                        res = llm.send_with_type(system_prompt, &user_prompt, request_type.clone()) => break res,
                    }
                }
            };

            let response_data = match llm_result {
                Ok((text, tokens)) => {
                    consecutive_transient_failures = 0;
                    // Clear prior slow-LLM warning on success.
                    let mut progress = self.progress.lock().await;
                    if progress.warning.is_some() {
                        progress.warning = None;
                        self.emit_progress(&app_handle, &progress);
                    }
                    Some((text, tokens))
                }
                Err(e) => {
                    let outcome = classify_llm_error(
                        &e,
                        batch.len(),
                        &mut consecutive_transient_failures,
                        &mut total_timeouts,
                    );
                    match outcome {
                        crate::screening::error_classify::LlmErrorOutcome::HardError => {
                            {
                                let c = crate::db::connection::lock_conn(conn_mutex)?;
                                let _ =
                                    audit_repo::log_error(&c, &format!("LLM request failed: {e}"));
                                mark_batch_screening_error(&c, &batch, &e.to_string(), None)?;
                            }
                            None
                        }
                        crate::screening::error_classify::LlmErrorOutcome::Defer {
                            batch_len,
                            should_stop,
                            warn_slow_llm,
                            ..
                        } => {
                            {
                                let c = crate::db::connection::lock_conn(conn_mutex)?;
                                let _ = audit_repo::log_error(
                                    &c,
                                    &format!("LLM request failed (transient): {e}"),
                                );
                            }
                            {
                                let mut progress = self.progress.lock().await;
                                progress.deferred += batch_len;
                                self.emit_progress(&app_handle, &progress);
                            }
                            if let Some(reason) = should_stop {
                                let mut progress = self.progress.lock().await;
                                progress.is_running = false;
                                progress.fatal_error = Some(reason.message);
                                progress.current_article_titles = vec![];
                                self.emit_progress(&app_handle, &progress);
                                return Ok(());
                            }
                            if warn_slow_llm {
                                let mut progress = self.progress.lock().await;
                                progress.warning = Some(
                                    "The LLM is responding slowly (last batch timed out at 120s). Consider reducing batch_size or checking your LLM provider status.".to_string()
                                );
                                self.emit_progress(&app_handle, &progress);
                            }
                            if self.delay_or_cancel(&app_handle, request_delay_ms).await {
                                return Ok(());
                            }
                            continue;
                        }
                    }
                }
            };

            // Cancellable post-success delay.
            if self.delay_or_cancel(&app_handle, request_delay_ms).await {
                return Ok(());
            }

            let (response_text, total_tokens) = match response_data {
                Some(data) => data,
                None => {
                    // Hard-error path only (transients `continue` above).
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
                    self.emit_progress(&app_handle, &progress);
                    continue;
                }
            };

            let tokens_per_article = total_tokens / batch.len();

            match crate::screening::json_parse::process_screening_responses(&response_text) {
                Ok(screenings) => {
                    // Count mismatch → mark batch as errors.
                    if screenings.len() != batch.len() {
                        {
                            let c = crate::db::connection::lock_conn(conn_mutex)?;
                            mark_batch_screening_error(
                                &c,
                                &batch,
                                &format!(
                                    "LLM returned {} results for {} articles",
                                    screenings.len(),
                                    batch.len()
                                ),
                                Some(&response_text),
                            )?;
                        }
                        let mut progress = self.progress.lock().await;
                        progress.errors += batch.len();
                        progress.completed += batch.len();
                        self.emit_progress(&app_handle, &progress);
                        continue;
                    }

                    // Per-article decision + write.
                    for (article, screening) in batch.iter().zip(screenings.iter()) {
                        if screening.decision == "error" {
                            {
                                let c = crate::db::connection::lock_conn(conn_mutex)?;
                                set_screening_error(&c, &article.id, &screening.reasoning, None)?;
                            }
                            let mut progress = self.progress.lock().await;
                            progress.errors += 1;
                            progress.completed += 1;
                            continue;
                        }

                        let decision = resolve_article_decision(
                            screening,
                            &article.id,
                            &criteria,
                            &inclusion_criteria,
                            &global_numbering,
                            has_custom_logic,
                            &enhanced_evidence_labels,
                        );

                        {
                            let c = crate::db::connection::lock_conn(conn_mutex)?;
                            write_article_screening_result(
                                &c,
                                &article.id,
                                &decision,
                                screening.confidence,
                                Some(tokens_per_article),
                                &screening.suggested_tags,
                                true,
                                &screening.extracted_terms,
                            )?;
                        }

                        let mut progress = self.progress.lock().await;
                        progress.completed += 1;
                        if decision.final_decision == "include" {
                            progress.included += 1;
                        } else {
                            progress.rejected += 1;
                        }
                    }

                    // Two-stage: delegate borderline re-screening.
                    if two_stage_mode {
                        let stage2_ctx = Stage2Context {
                            prompt_parts: &prompt_parts,
                            criteria: &criteria,
                            inclusion_criteria: &inclusion_criteria,
                            global_numbering: &global_numbering,
                            has_custom_logic,
                            enhanced_evidence_labels: &enhanced_evidence_labels,
                            config: &config,
                            request_delay_ms,
                            app_handle: &app_handle,
                        };
                        if self
                            .run_stage2_borderline(
                                conn_mutex,
                                llm,
                                &batch,
                                &screenings,
                                &stage2_ctx,
                            )
                            .await?
                        {
                            return Ok(());
                        }
                    }

                    // Post-batch elapsed/ETA.
                    let mut progress = self.progress.lock().await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    progress.elapsed_ms = elapsed;
                    if progress.completed > 0 {
                        let avg_per_article = elapsed / progress.completed as u64;
                        let remaining = (effective_total - progress.completed) as u64;
                        progress.estimated_remaining_ms = Some(avg_per_article * remaining);
                    }
                    self.emit_progress(&app_handle, &progress);
                }
                Err(parse_err) => {
                    {
                        let c = crate::db::connection::lock_conn(conn_mutex)?;
                        mark_batch_screening_error(
                            &c,
                            &batch,
                            &format!("Malformed LLM response: {parse_err}"),
                            Some(&response_text),
                        )?;
                        let _ = audit_repo::log_error(
                            &c,
                            &format!("Malformed LLM response: {parse_err}"),
                        );
                    }
                    let mut progress = self.progress.lock().await;
                    progress.errors += batch.len();
                    progress.completed += batch.len();
                    self.emit_progress(&app_handle, &progress);
                }
            }
        }

        // Final event.
        {
            let mut progress = self.progress.lock().await;
            progress.is_running = false;
            progress.current_article_titles = vec![];
            self.emit_progress(&app_handle, &progress);
        }
        Ok(())
    }

    /// Lock progress, apply `f`, emit `screening:progress` if app_handle set.
    pub(super) async fn update_progress(
        &self,
        app_handle: &Option<tauri::AppHandle>,
        f: impl FnOnce(&mut ScreeningProgress),
    ) {
        let mut progress = self.progress.lock().await;
        f(&mut progress);
        self.emit_progress(app_handle, &progress);
    }

    /// Emit a `screening:progress` event if an app handle is available.
    pub(super) fn emit_progress(
        &self,
        handle: &Option<tauri::AppHandle>,
        snapshot: &ScreeningProgress,
    ) {
        if let Some(h) = handle {
            let _ = h.emit("screening:progress", snapshot);
        }
    }
}

// Backward-compat re-exports (external tests import these from `engine`).

/// Re-export `json_parse` helpers.
pub use crate::screening::json_parse::{
    balance_braces, extract_json, process_screening_responses, repair_truncated_json_array,
};

/// Delegate to `chunk_retrieval::format_chunks_as_evidence` (pub for test compat).
#[must_use]
pub fn format_chunks_as_evidence(chunks: &[ScoredChunk]) -> Option<String> {
    crate::screening::chunk_retrieval::format_chunks_as_evidence(chunks)
}

/// Re-export `tags_labels` helpers.
pub use crate::screening::tags_labels::{
    create_or_match_label, create_or_match_tag, sanitize_tag_or_label_name,
    truncate_at_word_boundary,
};

/// Re-export `decision` helpers.
pub use crate::screening::decision::{
    augment_matched_from_reasoning, build_global_criterion_numbering,
};

/// Re-export `error_classify` helpers.
pub use crate::screening::error_classify::{is_auth_failure, is_transient_llm_error};
