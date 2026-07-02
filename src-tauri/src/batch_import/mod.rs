#![allow(clippy::expect_used)]
//! Batch Import Processor.
//!
//! A three-phase pipeline that scans the Bango Documents directory for files
//! produced by external tools (Citation Chaser, manual PDF drops) and imports
//! them into the article database:
//!
//! 1. **Full Text** (`fulltext/`): attach `{normalized_doi}.pdf` / `.txt` files
//!    to articles matching by DOI. Skips articles that already have full text.
//! 2. **Citations** (`ris/`): import `{normalized_doi}_references.ris` /
//!    `_citations.ris` / `.bib` files. Skips articles that already have the
//!    corresponding reference/citation details.
//! 3. **AI Summaries**: for each article that got full text attached in Phase
//!    1 (and has no existing summary), generate one via the same path as the
//!    "Generate AI Summary" button in the article detail panel. Only runs when
//!    `auto_summarize` is true (the frontend passes the
//!    `bango-full-text-summaries` localStorage flag).
//!
//! All three phases run inside a single spawned background task so the UI stays
//! responsive and the user can navigate to other sections. The runner emits
//! `batch-import:progress` events **per-item** within each phase; the frontend
//! listens and updates a progress bar. The user can cancel at any point via
//! [`cancel_batch_import`]; the runner checks the cancel token between items
//! (an in-flight LLM request completes naturally).
use crate::db::article_repo::{self, ArticleDoiInfo};
use crate::db::connection::DbState;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

pub mod citations_phase;
pub mod full_text_phase;
pub mod summary_phase;

/// A wrapper around the DOI match map. Defined here so both phase modules can
/// share one type without a circular dependency on each other.
pub struct DoiMatchMap(pub HashMap<String, ArticleDoiInfo>);

impl DoiMatchMap {
    /// Look up an article by its cleaned-DOI key.
    pub fn get(&self, key: &str) -> Option<&ArticleDoiInfo> {
        self.0.get(key)
    }

    /// Number of entries.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the map contains a key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
}

/// Result of a single phase's execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportPhaseResult {
    /// Total items discovered for this phase (before cancellation/processing).
    pub total: usize,
    /// Items processed (attempted) so far. May be less than `total` if cancelled.
    pub processed: usize,
    /// Items that succeeded.
    pub succeeded: usize,
    /// Items that failed.
    pub failed: usize,
    /// Non-fatal error messages collected during the phase.
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Which phase is currently running (1-indexed for display).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BatchImportPhase {
    FullText = 1,
    Citations = 2,
    Summaries = 3,
}

impl BatchImportPhase {
    /// Human-readable phase name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::FullText => "Full Text",
            Self::Citations => "Citations",
            Self::Summaries => "AI Summaries",
        }
    }
}

/// The progress payload emitted via the `batch-import:progress` event and
/// returned by `get_batch_import_progress`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportProgress {
    /// Current phase (1-based).
    pub phase: usize,
    /// Human-readable phase name.
    pub phase_name: String,
    /// Items completed in the current phase so far.
    pub completed: usize,
    /// Total items in the current phase.
    pub total: usize,
    /// Overall percentage across all 3 phases (0-100).
    pub overall_percent: usize,
    /// Human-readable status message (e.g. the filename being processed).
    #[serde(default)]
    pub message: String,
    /// Whether the runner is currently active.
    pub is_running: bool,
    /// Whether the runner was cancelled by the user.
    pub is_cancelled: bool,
    /// Final per-phase results (populated as each phase completes).
    #[serde(default)]
    pub full_text: Option<BatchImportPhaseResult>,
    #[serde(default)]
    pub citations: Option<BatchImportPhaseResult>,
    #[serde(default)]
    pub summaries: Option<BatchImportPhaseResult>,
}

/// Managed state holding the cancel token and progress snapshot. Both use
/// `std::sync::Mutex` (not `tokio::sync`) so per-item progress callbacks can
/// fire synchronously inside the phase loops without any `.await`.
pub struct BatchImportState {
    cancel_token: Arc<Mutex<bool>>,
    progress: Arc<Mutex<BatchImportProgress>>,
}

impl Default for BatchImportState {
    fn default() -> Self {
        Self {
            cancel_token: Arc::new(Mutex::new(false)),
            progress: Arc::new(Mutex::new(BatchImportProgress::default())),
        }
    }
}

impl BatchImportState {
    /// Get a cloned handle to the cancel token so background tasks can poll it.
    pub fn cancel_handle(&self) -> Arc<Mutex<bool>> {
        Arc::clone(&self.cancel_token)
    }

    /// Get a cloned handle to the progress struct so background tasks can
    /// update it.
    pub fn progress_handle(&self) -> Arc<Mutex<BatchImportProgress>> {
        Arc::clone(&self.progress)
    }
}

/// Compute a percentage (0-100) avoiding division by zero.
#[allow(clippy::manual_checked_ops)]
fn percent(processed: usize, total: usize) -> usize {
    if total == 0 {
        100
    } else {
        (processed * 100) / total
    }
}

/// Emit a progress update via both the event system and the shared progress
/// struct (so `get_batch_import_progress` returns the latest snapshot).
///
/// This is **synchronous** (not `async`) so it can be called from inside the
/// phase loops via the `on_progress` callback. The `app_handle.emit()` call is
/// non-blocking (it queues the event on the JS side).
#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app_handle: &tauri::AppHandle,
    progress: &Arc<Mutex<BatchImportProgress>>,
    phase: BatchImportPhase,
    completed: usize,
    total: usize,
    overall_percent: usize,
    message: &str,
    is_running: bool,
    is_cancelled: bool,
    full_text: Option<BatchImportPhaseResult>,
    citations: Option<BatchImportPhaseResult>,
    summaries: Option<BatchImportPhaseResult>,
) {
    let payload = BatchImportProgress {
        phase: phase as usize,
        phase_name: phase.name().to_string(),
        completed,
        total,
        overall_percent,
        message: message.to_string(),
        is_running,
        is_cancelled,
        full_text: full_text.clone(),
        citations: citations.clone(),
        summaries: summaries.clone(),
    };
    {
        let mut guard = progress.lock().expect("batch import mutex");
        guard.phase = payload.phase;
        guard.phase_name = payload.phase_name.clone();
        guard.completed = payload.completed;
        guard.total = payload.total;
        guard.overall_percent = payload.overall_percent;
        guard.message = payload.message.clone();
        guard.is_running = payload.is_running;
        guard.is_cancelled = payload.is_cancelled;
        if full_text.is_some() {
            guard.full_text = full_text;
        }
        if citations.is_some() {
            guard.citations = citations;
        }
        if summaries.is_some() {
            guard.summaries = summaries;
        }
    }
    let _ = app_handle.emit("batch-import:progress", payload);
}

/// Start the batch import pipeline. Returns immediately after spawning the
/// background task; the frontend tracks progress via the `batch-import:progress`
/// event and can cancel via [`cancel_batch_import`].
///
/// # Arguments
/// * `auto_summarize` - When true, Phase 3 runs (generate AI summaries for
///   newly-attached articles). When false, only Phases 1 and 2 run.
/// * `include_section_summaries` - Forwarded to the summary generator. Should
///   mirror the `bango-section-summaries` localStorage flag.
#[tauri::command]
pub async fn start_batch_import(
    app_handle: tauri::AppHandle,
    _db_state: State<'_, DbState>,
    batch_state: State<'_, BatchImportState>,
    auto_summarize: Option<bool>,
    include_section_summaries: Option<bool>,
) -> Result<BatchImportProgress, AppError> {
    // ── Concurrent-start guard ──
    {
        let guard = batch_state.progress.lock().expect("batch import mutex");
        if guard.is_running {
            return Ok(guard.clone());
        }
    }

    // Reset cancel token + progress snapshot.
    let cancel_handle = batch_state.cancel_handle();
    {
        let mut token = cancel_handle.lock().expect("batch import mutex");
        *token = false;
        let mut prog = batch_state.progress.lock().expect("batch import mutex");
        *prog = BatchImportProgress::default();
    }

    let auto_sum = auto_summarize.unwrap_or(false);
    let include_sections = include_section_summaries.unwrap_or(false);
    let progress = batch_state.progress_handle();
    let app_handle_clone = app_handle.clone();
    let cancel_for_task = Arc::clone(&cancel_handle);

    // Emit initial state.
    emit_progress(
        &app_handle,
        &progress,
        BatchImportPhase::FullText,
        0,
        0,
        0,
        "Starting batch import...",
        true,
        false,
        None,
        None,
        None,
    );

    // Spawn the background task.
    tokio::spawn(async move {
        let db = app_handle_clone.state::<DbState>();

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 1: Full Text (runs on spawn_blocking to avoid UI hang)
        // ═══════════════════════════════════════════════════════════════════
        let p1_cancel = Arc::clone(&cancel_for_task);
        let p1_progress = Arc::clone(&progress);
        let p1_app = app_handle_clone.clone();
        let p1_db = app_handle_clone.clone();
        let phase1_result = tokio::task::spawn_blocking(move || {
            let db = p1_db.state::<DbState>();
            let conn = match db.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    return (
                        BatchImportPhaseResult::default(),
                        vec![],
                        Some(format!("DB lock error: {e}")),
                    );
                }
            };
            let is_cancelled = || {
                let guard = p1_cancel.try_lock();
                matches!(guard, Ok(g) if *g)
            };
            let mut on_progress = |processed: usize, total: usize, msg: &str| {
                let overall = percent(processed, total) / 3;
                emit_progress(
                    &p1_app,
                    &p1_progress,
                    BatchImportPhase::FullText,
                    processed,
                    total,
                    overall,
                    msg,
                    true,
                    false,
                    None,
                    None,
                    None,
                );
            };
            match full_text_phase::run_full_text_phase(&conn, &is_cancelled, &mut on_progress) {
                Ok((result, ids)) => (result, ids, None),
                Err(e) => (
                    BatchImportPhaseResult {
                        total: 0,
                        processed: 0,
                        succeeded: 0,
                        failed: 0,
                        errors: vec![format!("Phase 1 error: {e}")],
                    },
                    vec![],
                    None,
                ),
            }
        })
        .await
        .unwrap_or_else(|e| {
            (
                BatchImportPhaseResult {
                    errors: vec![format!("Phase 1 thread panic: {e}")],
                    ..Default::default()
                },
                vec![],
                None,
            )
        });
        let (ft_result, newly_attached, ft_lock_err) = phase1_result;

        // Handle deferred lock error.
        if let Some(msg) = ft_lock_err {
            emit_progress(
                &app_handle_clone,
                &progress,
                BatchImportPhase::FullText,
                0,
                0,
                0,
                &msg,
                false,
                false,
                None,
                None,
                None,
            );
            return;
        }

        // Phase 1 complete summary.
        let ft_percent = percent(ft_result.processed, ft_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::FullText,
            ft_result.processed,
            ft_result.total,
            ft_percent / 3,
            &format!(
                "Phase 1 (Full Text): {} attached, {} failed",
                ft_result.succeeded, ft_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            None,
            None,
        );

        // Check cancel after phase 1.
        if let Ok(g) = cancel_for_task.try_lock() {
            if *g {
                emit_progress(
                    &app_handle_clone,
                    &progress,
                    BatchImportPhase::FullText,
                    ft_result.processed,
                    ft_result.total,
                    ft_percent / 3,
                    "Cancelled by user",
                    false,
                    true,
                    Some(ft_result),
                    None,
                    None,
                );
                return;
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 2: Citations (runs on spawn_blocking to avoid UI hang)
        // ═══════════════════════════════════════════════════════════════════
        let p2_cancel = Arc::clone(&cancel_for_task);
        let p2_progress = Arc::clone(&progress);
        let p2_app = app_handle_clone.clone();
        let p2_db = app_handle_clone.clone();
        let p2_ft = ft_result.clone();
        let cit_result = tokio::task::spawn_blocking(move || {
            let db = p2_db.state::<DbState>();
            let conn = match db.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    return BatchImportPhaseResult {
                        errors: vec![format!("DB lock error: {e}")],
                        ..Default::default()
                    };
                }
            };
            let is_cancelled = || {
                let guard = p2_cancel.try_lock();
                matches!(guard, Ok(g) if *g)
            };
            let mut on_progress = |processed: usize, total: usize, msg: &str| {
                let overall = 33 + percent(processed, total) / 3;
                emit_progress(
                    &p2_app,
                    &p2_progress,
                    BatchImportPhase::Citations,
                    processed,
                    total,
                    overall,
                    msg,
                    true,
                    false,
                    Some(p2_ft.clone()),
                    None,
                    None,
                );
            };
            match citations_phase::run_citations_phase(&conn, &is_cancelled, &mut on_progress) {
                Ok(result) => result,
                Err(e) => BatchImportPhaseResult {
                    total: 0,
                    processed: 0,
                    succeeded: 0,
                    failed: 0,
                    errors: vec![format!("Phase 2 error: {e}")],
                },
            }
        })
        .await
        .unwrap_or_else(|e| BatchImportPhaseResult {
            errors: vec![format!("Phase 2 thread panic: {e}")],
            ..Default::default()
        });

        // Phase 2 complete summary.
        let cit_percent = percent(cit_result.processed, cit_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Citations,
            cit_result.processed,
            cit_result.total,
            33 + cit_percent / 3,
            &format!(
                "Phase 2 (Citations): {} imported, {} failed",
                cit_result.succeeded, cit_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            None,
        );

        // Check cancel after phase 2.
        if let Ok(g) = cancel_for_task.try_lock() {
            if *g {
                emit_progress(
                    &app_handle_clone,
                    &progress,
                    BatchImportPhase::Citations,
                    cit_result.processed,
                    cit_result.total,
                    33 + cit_percent / 3,
                    "Cancelled by user",
                    false,
                    true,
                    Some(ft_result),
                    Some(cit_result),
                    None,
                );
                return;
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        //  Phase 3: AI Summaries (optional)
        // ═══════════════════════════════════════════════════════════════════
        let sum_result = if auto_sum {
            let to_summarize: Vec<String> = {
                let conn = match db.conn.lock() {
                    Ok(c) => c,
                    Err(_) => {
                        return;
                    }
                };
                match article_repo::get_articles_with_doi_info(&conn) {
                    Ok(articles) => {
                        let id_set: std::collections::HashSet<&str> =
                            newly_attached.iter().map(String::as_str).collect();
                        articles
                            .into_iter()
                            .filter(|a| id_set.contains(a.id.as_str()) && !a.has_ai_summary)
                            .map(|a| a.id)
                            .collect::<Vec<_>>()
                    }
                    Err(_) => vec![],
                }
            };

            // Per-item progress callback for Phase 3.
            let prog_snap = Arc::clone(&progress);
            let app_snap = app_handle_clone.clone();
            let ft_snap = ft_result.clone();
            let cit_snap = cit_result.clone();
            let mut on_progress = move |processed: usize, total: usize, msg: &str| {
                let overall = 66 + percent(processed, total) / 3;
                emit_progress(
                    &app_snap,
                    &prog_snap,
                    BatchImportPhase::Summaries,
                    processed,
                    total,
                    overall,
                    msg,
                    true,
                    false,
                    Some(ft_snap.clone()),
                    Some(cit_snap.clone()),
                    None,
                );
            };

            let cancel_snap = Arc::clone(&cancel_for_task);
            summary_phase::run_summary_phase(
                &db,
                &app_handle_clone,
                to_summarize,
                include_sections,
                &mut on_progress,
                move || {
                    let snap = Arc::clone(&cancel_snap);
                    async move { *snap.lock().expect("batch import mutex") }
                },
            )
            .await
        } else {
            BatchImportPhaseResult {
                errors: vec!["Phase 3 skipped (auto-summarize disabled)".to_string()],
                ..Default::default()
            }
        };

        // Phase 3 complete summary.
        let sum_percent = percent(sum_result.processed, sum_result.total);
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Summaries,
            sum_result.processed,
            sum_result.total,
            66 + sum_percent / 3,
            &format!(
                "Phase 3 (AI Summaries): {} summarized, {} failed",
                sum_result.succeeded, sum_result.failed
            ),
            true,
            false,
            Some(ft_result.clone()),
            Some(cit_result.clone()),
            Some(sum_result.clone()),
        );

        // Final state.
        emit_progress(
            &app_handle_clone,
            &progress,
            BatchImportPhase::Summaries,
            sum_result.processed,
            sum_result.total,
            100,
            "Batch import complete",
            false,
            false,
            Some(ft_result),
            Some(cit_result),
            Some(sum_result),
        );
    });

    let guard = batch_state.progress.lock().expect("batch import mutex");
    Ok(guard.clone())
}

/// Cancel a running batch import. The runner checks the token between items; an
/// in-flight LLM request completes naturally.
#[tauri::command]
pub async fn cancel_batch_import(batch_state: State<'_, BatchImportState>) -> Result<(), AppError> {
    let handle = batch_state.cancel_handle();
    *handle.lock().expect("batch import mutex") = true;
    Ok(())
}

/// Get the current batch-import progress snapshot (for polling or initial load).
#[tauri::command]
pub async fn get_batch_import_progress(
    batch_state: State<'_, BatchImportState>,
) -> Result<BatchImportProgress, AppError> {
    let guard = batch_state.progress.lock().expect("batch import mutex");
    Ok(guard.clone())
}
