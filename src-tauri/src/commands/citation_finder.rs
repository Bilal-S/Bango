//! Tauri commands for the Citation Finder (`citation_finder/AGENTS.md`).
//!
//! `find_citations`: one-button entry - spawns Phase A (readiness) → Phase B
//! (auto-prepare embeddings if coverage <100%) → Phase C (search). Emits
//! `citation:progress` / `citation:done` / `citation:error`.
//! `cancel_citation_search` / `get_citation_finder_readiness` mirror
//! `CitationFinderState`: `Arc<AtomicBool>` cancel + `Arc<Mutex<Progress>>` snapshot.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use tauri::{Emitter, Manager, State};

use crate::citation_finder::{CitationFinderMode, CitationFinderProgress, CitationFinderReadiness};
use crate::db::connection::{lock_conn, DbState};
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;

use crate::citation_finder::readiness::compute_readiness;
use crate::citation_finder::search::{
    find_citations_inner, FindCitationsContext, HttpCitationLlmSender,
};

/// Managed state: cancel token (`Arc<AtomicBool>`) + progress snapshot.
/// Token covers both Phase B (embedding runner) and Phase C.
pub struct CitationFinderState {
    cancel_token: Arc<AtomicBool>,
    progress: Arc<Mutex<CitationFinderProgress>>,
}

impl Default for CitationFinderState {
    fn default() -> Self {
        Self {
            cancel_token: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(CitationFinderProgress::default())),
        }
    }
}

impl CitationFinderState {
    /// Get a cloned handle to the cancel token so the background task can poll
    /// it.
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_token)
    }

    /// Get a cloned handle to the progress struct so the background task can
    /// update it.
    pub fn progress_handle(&self) -> Arc<Mutex<CitationFinderProgress>> {
        Arc::clone(&self.progress)
    }
}

/// Begin a citation run under the shared progress lock.
///
/// Returns `Ok(Some(snapshot))` when a run is already active: the caller
/// returns that snapshot and MUST NOT touch the cancel token (an extra Find
/// click must never un-cancel the active search). Returns `Ok(None)` after
/// resetting the token and marking the run as started.
pub fn begin_citation_run(
    progress: &Mutex<CitationFinderProgress>,
    cancel: &AtomicBool,
) -> Result<Option<CitationFinderProgress>, AppError> {
    let mut prog = crate::db::connection::lock_state(progress)?;
    if prog.is_running {
        return Ok(Some(prog.clone()));
    }
    cancel.store(false, Ordering::Relaxed);
    *prog = CitationFinderProgress {
        phase: "searching".to_string(),
        stage: None,
        done: 0,
        total: 0,
        overall_percent: 0,
        message: "Starting citation search…".to_string(),
        is_running: true,
        is_cancelled: false,
        funnel: None,
    };
    Ok(None)
}

/// Human message from a caught panic payload (for the terminal event).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// One-button entry. Returns immediately after spawning the background task;
/// frontend tracks progress via events.
#[tauri::command]
pub async fn find_citations(
    app_handle: tauri::AppHandle,
    _db_state: State<'_, DbState>,
    cf_state: State<'_, CitationFinderState>,
    text: String,
    mode: CitationFinderMode,
    status_filter: Vec<String>,
) -> Result<CitationFinderProgress, AppError> {
    /* Concurrent-start guard + reset (atomic under ONE lock). The guard check
     * and `is_running = true` reset MUST run under the same lock to close the
     * TOCTOU race where two rapid calls both pass the guard. */
    let cancel_handle = cf_state.cancel_handle();
    if let Some(snapshot) = begin_citation_run(&cf_state.progress, &cancel_handle)? {
        return Ok(snapshot);
    }

    let progress = cf_state.progress_handle();
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    let embedding_sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        crate::embedding::runner::backend_sender(&app_handle)?;
    let llm_sender: Arc<dyn crate::citation_finder::search::CitationLlmSender> =
        Arc::new(HttpCitationLlmSender {
            orchestrator: Arc::clone(&orchestrator),
            app_handle: app_handle.clone(),
        });

    let app_handle_for_task = app_handle.clone();
    let cancel_for_task = Arc::clone(&cancel_handle);
    let progress_for_task = Arc::clone(&progress);
    let text_for_task = text.clone();
    let statuses_for_task = status_filter.clone();

    eprintln!("[citation] run start mode={mode:?} statuses={statuses_for_task:?}");
    tokio::task::spawn(async move {
        let db = app_handle_for_task.state::<DbState>();
        let progress_snapshot = Arc::clone(&progress_for_task);
        let app_handle_for_emit = app_handle_for_task.clone();
        let emit = move |p: CitationFinderProgress| {
            /* Update shared snapshot (so cancel_citation_search + polling see
             * latest state) and emit the event. */
            match crate::db::connection::lock_state(&progress_snapshot) {
                Ok(mut guard) => *guard = p.clone(),
                Err(e) => eprintln!("[citation] progress snapshot lock failed: {e}"),
            }
            let _ = app_handle_for_emit.emit("citation:progress", p);
        };

        /* Panic-safe: a panic anywhere in the pipeline still produces a
         * terminal `citation:error` and clears `is_running` below, so the UI
         * can never wedge on "Classifying…" with the guard left closed. */
        let result = std::panic::AssertUnwindSafe(find_citations_inner(
            &db,
            Arc::clone(&embedding_sender),
            Arc::clone(&llm_sender),
            FindCitationsContext {
                text: text_for_task,
                mode,
                status_filter: statuses_for_task,
                cancel_token: Arc::clone(&cancel_for_task),
                emit_progress: &emit,
                /* Thread app_handle so Phase B forwards embedding progress
                 * events to the frontend (use-citation-finder.ts translates
                 * each into a citation:progress update). */
                app_handle: Some(app_handle_for_task.clone()),
            },
        ))
        .catch_unwind()
        .await;
        let result = match result {
            Ok(result) => result,
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                eprintln!("[citation] run panic: {message}");
                Err(AppError::Import(format!("Citation search crashed: {message}")))
            }
        };

        // Mark not-running in the snapshot regardless of outcome.
        match crate::db::connection::lock_state(&progress_for_task) {
            Ok(mut guard) => {
                guard.is_running = false;
                if cancel_for_task.load(Ordering::Relaxed) {
                    guard.is_cancelled = true;
                }
            }
            Err(e) => eprintln!("[citation] progress finalize lock failed: {e}"),
        }

        match result {
            Ok(results) => {
                eprintln!("[citation] run done");
                let _ = app_handle_for_task.emit("citation:done", &results);
            }
            Err(e) => {
                /* Strip `AppError::Import`'s `"Import error: "` prefix (a
                 * thiserror artifact). Citation Finder uses Import because it's
                 * the only free-form String variant, not due to import errors. */
                let raw = format!("{e}");
                let msg = raw.strip_prefix("Import error: ").unwrap_or(&raw).to_string();
                eprintln!("[citation] run error: {msg}");
                let _ = app_handle_for_task.emit("citation:error", &msg);
            }
        }
    });

    let guard = crate::db::connection::lock_state(&cf_state.progress)?;
    Ok(guard.clone())
}

/// Cancel a running citation search. Phase C awaits (recall, claim split,
/// classify) run through `await_cancellable`, which polls this token every
/// 150ms and drops the in-flight future (aborting the request); a
/// classification that completed after the click is discarded by the
/// post-call check. Phase B's embedding runner checks the token between
/// completed articles, so an in-flight embedding batch still finishes
/// naturally.
#[tauri::command]
pub async fn cancel_citation_search(
    cf_state: State<'_, CitationFinderState>,
) -> Result<(), AppError> {
    cf_state.cancel_handle().store(true, Ordering::Relaxed);
    Ok(())
}

/// Read the readiness payload (toggle visibility + tooltip hint). Does NOT
/// gate the action - `find_citations` runs its own Phase A check internally.
#[tauri::command]
pub async fn get_citation_finder_readiness(
    db_state: State<'_, DbState>,
    status_filter: Vec<String>,
) -> Result<CitationFinderReadiness, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    compute_readiness(&conn, &status_filter)
}

/* `CitationResult` is re-exported by the search module and reaches the
 * frontend via `citation:done` event serialization, not via a direct
 * command return. */
