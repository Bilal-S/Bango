//! Tauri commands for the Citation Finder (`citation_finder/AGENTS.md`).
//!
//! `find_citations`: one-button entry - spawns Phase A (readiness) → Phase B
//! (auto-prepare embeddings if coverage <100%) → Phase C (search). Emits
//! `citation:progress` / `citation:done` / `citation:error`.
//! `cancel_citation_search` / `get_citation_finder_readiness` mirror
//! `CitationFinderState`: `Arc<AtomicBool>` cancel + `Arc<Mutex<Progress>>` snapshot.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, State};

use crate::citation_finder::{CitationFinderMode, CitationFinderProgress, CitationFinderReadiness};
use crate::db::connection::{lock_conn, DbState};
use crate::embedding::runner::HttpEmbeddingBatchSender;
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
    cancel_handle.store(false, Ordering::Relaxed);
    {
        let Ok(mut prog) = cf_state.progress.lock() else {
            return Err(AppError::Import("Citation Finder mutex poisoned".to_string()));
        };
        if prog.is_running {
            return Ok(prog.clone());
        }
        *prog = CitationFinderProgress {
            phase: "searching".to_string(),
            stage: None,
            done: 0,
            total: 0,
            overall_percent: 0,
            message: "Starting citation search…".to_string(),
            is_running: true,
            is_cancelled: false,
        };
    }

    let progress = cf_state.progress_handle();
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    let embedding_sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
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

    tokio::task::spawn(async move {
        let db = app_handle_for_task.state::<DbState>();
        let progress_snapshot = Arc::clone(&progress_for_task);
        let app_handle_for_emit = app_handle_for_task.clone();
        let emit = move |p: CitationFinderProgress| {
            /* Update shared snapshot (so cancel_citation_search + polling see
             * latest state) and emit the event. */
            if let Ok(mut guard) = progress_snapshot.lock() {
                *guard = p.clone();
            }
            let _ = app_handle_for_emit.emit("citation:progress", p);
        };

        let result = find_citations_inner(
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
        )
        .await;

        // Mark not-running in the snapshot regardless of outcome.
        if let Ok(mut guard) = progress_for_task.lock() {
            guard.is_running = false;
            if cancel_for_task.load(Ordering::Relaxed) {
                guard.is_cancelled = true;
            }
        }

        match result {
            Ok(results) => {
                let _ = app_handle_for_task.emit("citation:done", &results);
            }
            Err(e) => {
                /* Strip `AppError::Import`'s `"Import error: "` prefix (a
                 * thiserror artifact). Citation Finder uses Import because it's
                 * the only free-form String variant, not due to import errors. */
                let raw = format!("{e}");
                let msg = raw.strip_prefix("Import error: ").unwrap_or(&raw).to_string();
                let _ = app_handle_for_task.emit("citation:error", &msg);
            }
        }
    });

    let Ok(guard) = cf_state.progress.lock() else {
        return Err(AppError::Import("Citation Finder mutex poisoned".to_string()));
    };
    Ok(guard.clone())
}

/// Cancel a running citation search. The token is checked between pipeline
/// stages; an in-flight LLM/embedding request completes naturally.
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
