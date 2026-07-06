//! In-memory translation queue and worker.
//!
//! There is no `translation_jobs` table. The DB-backed progress record lives on
//! the `articles` row (`translation_status` + `is_translated`). The worker is a
//! single Tokio task spawned once at app startup; it owns a
//! `tokio::mpsc::Receiver<TranslationJob>` channel. Crash recovery marks any
//! article with `translation_status IN ('queued','running') AND is_translated = 0`
//! as `failed` (see [`STARTUP_STRANDED_CAP`]); the user retries manually.
//!
//! Per the in-memory translation queue and worker design.

use std::sync::Arc;

use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;
use crate::translation::engine::{
    translate_full_text, translate_metadata_only, TranslationLlmClient,
};

/// The kind of translation job to run.
#[derive(Debug, Clone)]
pub enum TranslationJobKind {
    /// Translate title + abstract only (no full text attached).
    MetadataOnly,
    /// Translate title + abstract + all full-text chunks then re-chunk.
    FullText,
}

/// A single queued translation job.
#[derive(Debug, Clone)]
pub struct TranslationJob {
    pub article_id: String,
    pub kind: TranslationJobKind,
}

/// Managed state: handle used by commands to enqueue jobs.
pub struct TranslationWorkerHandle {
    sender: mpsc::Sender<TranslationJob>,
}

impl TranslationWorkerHandle {
    /// Enqueue a translation job. Returns the receiving-end error if the
    /// worker task has exited (treated as a transient failure by callers).
    pub fn try_send(&self, job: TranslationJob) -> Result<(), AppError> {
        self.sender
            .try_send(job)
            .map_err(|e| AppError::Import(format!("Translation worker unavailable: {e}")))
    }

    /// Borrow the channel sender. Used by `reenqueue_stranded_on_startup`
    /// during app setup (the handle has not been managed yet).
    pub fn sender(&self) -> &mpsc::Sender<TranslationJob> {
        &self.sender
    }
}

/// Spawn the translation worker task and return the handle used to enqueue.
///
/// The worker reads jobs from the channel, fetches the current LLM config +
/// orchestrator from managed state, locks the DB mutex only for sync
/// read/write bursts (releasing it across `.await` LLM calls), dispatches to
/// `engine::translate_metadata_only` or `engine::translate_full_text` based on
/// the job kind, and emits `translation:complete` so the frontend refreshes.
pub fn spawn_translation_worker(app: tauri::AppHandle) -> TranslationWorkerHandle {
    let (sender, mut receiver) = mpsc::channel::<TranslationJob>(64);

    let app_handle = app.clone();
    // Use `tauri::async_runtime::spawn` (not raw `tokio::spawn`) because this
    // function is called from the synchronous `.setup(|app| {...})` closure in
    // `lib.rs`, which runs outside the Tokio runtime thread-local context.
    // Raw `tokio::spawn` panics with "there is no reactor running" in that
    // context; `tauri::async_runtime::spawn` routes through Tauri's global
    // runtime handle so it works from sync and async call sites alike. (The
    // other `tokio::spawn` sites in `commands/screening.rs` /
    // `batch_import/mod.rs` are inside `#[tauri::command] async fn` handlers,
    // which already run on the runtime.)
    tauri::async_runtime::spawn(async move {
        while let Some(job) = receiver.recv().await {
            let article_id = job.article_id.clone();
            let app_for_job = app_handle.clone();

            // Fetch LLM config + orchestrator while holding the DB lock briefly.
            let (config, orchestrator) = {
                let db = app_handle.state::<DbState>();
                let conn = match db.conn.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[translation] failed to lock DB for job {article_id}: {e}");
                        continue;
                    }
                };
                let config = match llm_config_repo::get_config(&conn) {
                    Ok(Some(c)) => c,
                    Ok(None) => {
                        // LLM not configured: mark failed + audit, then continue.
                        let _ = article_repo::update_translation_status_failed(
                            &conn,
                            &article_id,
                            "LLM not configured",
                        );
                        let _ = audit_repo::create_entry(
                            &conn,
                            &article_id,
                            "translation_error",
                            None,
                            None,
                            Some("Translation skipped: LLM not configured"),
                            "system",
                        );
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[translation] failed to read LLM config for {article_id}: {e}");
                        continue;
                    }
                };
                let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
                (config, orchestrator)
            }; // DB lock released before the async LLM call.

            // `context_window_tokens` is plumbed into `translate_full_text` for
            // batch sizing (the engine packs chunks into context-window-sized
            // batches; see translation-3-plan.md). It cannot be read from the
            // `&dyn LlmClient` trait object inside the engine, so we extract it
            // here from the concrete `LlmConfig` before constructing the client.
            let context_window_tokens = config.context_window_tokens;
            let client = TranslationLlmClient { config, orchestrator, job_id: article_id.clone() };

            // Run the translation. The engine takes `&Mutex<Connection>` and
            // locks in bursts so the guard is never held across `.await`.
            // Dispatch FullText jobs to the full-text engine; MetadataOnly jobs
            // run the metadata-only path.
            let db = app_for_job.state::<DbState>();
            let result = match job.kind {
                TranslationJobKind::FullText => {
                    translate_full_text(&db.conn, &article_id, &client, context_window_tokens).await
                }
                TranslationJobKind::MetadataOnly => {
                    translate_metadata_only(&db.conn, &article_id, &client).await
                }
            };

            match result {
                Ok(()) => {
                    let _ = app_for_job.emit(
                        "translation:complete",
                        serde_json::json!({ "articleId": article_id, "success": true }),
                    );
                    // Tier 1e: notify the in-process bus so batch-import Phase
                    // 3 + the screening translation pre-step can await
                    // completion without polling the DB every 2s.
                    if let Some(bus) =
                        app_for_job.try_state::<crate::translation::TranslationDoneBus>()
                    {
                        bus.emit_done(&article_id);
                    }
                }
                Err(e) => {
                    eprintln!("[translation] job {article_id} failed: {e}");
                    let _ = app_for_job.emit(
                        "translation:complete",
                        serde_json::json!({ "articleId": article_id, "success": false, "error": e.to_string() }),
                    );
                    // Emit on failure too so waiters don't stall waiting for a
                    // success that will never come; the waiter re-checks the
                    // live status and sees `translation_status = 'failed'`.
                    if let Some(bus) =
                        app_for_job.try_state::<crate::translation::TranslationDoneBus>()
                    {
                        bus.emit_done(&article_id);
                    }
                }
            }
        }
    });

    TranslationWorkerHandle { sender }
}

/// Maximum number of stranded jobs re-enqueued at startup.
///
/// **Decision: no auto-recovery on restart.** Set to `0` so every stranded
/// article (one left in `queued`/`running` when the previous process died) is
/// marked `failed` with a retryable audit note instead of being silently
/// re-enqueued. The user selectively retranslates via the manual translate
/// button on the article detail header (the enqueue gate accepts `failed`).
///
/// Set this to a positive `N` to re-enable bounded re-enqueueing of the first
/// `N` stranded jobs; any excess is still marked `failed`. A large backlog
/// (e.g. a crash during a 200-article batch import) would otherwise dump all
/// of them into the channel at once, producing a startup burst that saturates
/// the worker and contends with UI/LLM traffic.
pub const STARTUP_STRANDED_CAP: usize = 0;

/// Crash recovery for articles stranded in `queued` or `running` at startup.
/// Called once after the worker task is spawned. Non-fatal - errors are logged.
///
/// With [`STARTUP_STRANDED_CAP`] = 0 (the current decision), **no** stranded
/// job is re-enqueued: every stranded row is reset to `failed` via
/// [`article_repo::mark_stranded_capped_failed`] with a retryable audit note.
/// The user selectively retranslates via the manual translate button on the
/// article detail header (the enqueue gate accepts `failed`). Raising the cap
/// to a positive `N` re-enables bounded re-enqueueing of the first `N`
/// stranded jobs, choosing `FullText` when `has_full_text = 1` else
/// `MetadataOnly` so a stranded full-text job does not silently degrade to
/// metadata-only on restart.
pub fn reenqueue_stranded_on_startup(
    conn: &rusqlite::Connection,
    sender: &mpsc::Sender<TranslationJob>,
) {
    match article_repo::get_stranded_translation_articles(conn) {
        Ok(stranded) => {
            let total = stranded.len();
            let enqueued = stranded.iter().take(STARTUP_STRANDED_CAP);
            let capped: Vec<String> =
                stranded.iter().skip(STARTUP_STRANDED_CAP).map(|(id, _)| id.clone()).collect();

            for (id, has_full_text) in enqueued {
                // Reset to 'queued' so the worker picks it up; if it was
                // 'running' the worker that owned it has died.
                let _ = article_repo::update_translation_status(conn, id, "queued");
                let kind = if *has_full_text {
                    TranslationJobKind::FullText
                } else {
                    TranslationJobKind::MetadataOnly
                };
                if let Err(e) = sender.try_send(TranslationJob { article_id: id.clone(), kind }) {
                    eprintln!("[translation] failed to re-enqueue stranded job {id}: {e}");
                }
            }

            if !capped.is_empty() {
                // User-facing audit note. With STARTUP_STRANDED_CAP = 0 this is
                // the path every stranded article takes; the wording avoids the
                // odd "capped at 0" phrasing while staying accurate for a
                // future positive cap.
                let note = format!(
                    "Translation interrupted by application restart and not auto-recovered. \
                     Retry it manually from the article detail panel. \
                     (Startup recovery cap: {STARTUP_STRANDED_CAP}.)"
                );
                if let Err(e) = article_repo::mark_stranded_capped_failed(conn, &capped, &note) {
                    eprintln!("[translation] failed to mark capped stranded jobs as failed: {e}");
                }
                eprintln!(
                    "[translation] re-enqueued {STARTUP_STRANDED_CAP} of {total} stranded job(s); \
                     marked {} as failed (cap exceeded)",
                    capped.len()
                );
            } else if total > 0 {
                eprintln!("[translation] re-enqueued {total} stranded job(s) on startup");
            }
        }
        Err(e) => {
            eprintln!("[translation] failed to query stranded jobs on startup: {e}");
        }
    }
}
