//! In-memory translation queue and worker.
//!
//! There is no `translation_jobs` table. The DB-backed progress record lives on
//! the `articles` row (`translation_status` + `is_translated`). The worker is a
//! single Tokio task spawned once at app startup; it owns a
//! `tokio::mpsc::Receiver<TranslationJob>` channel. Crash recovery re-enqueues
//! any article with `translation_status IN ('queued','running') AND
//! is_translated = 0`.
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

            let client = TranslationLlmClient { config, orchestrator, job_id: article_id.clone() };

            // Run the translation. The engine takes `&Mutex<Connection>` and
            // locks in bursts so the guard is never held across `.await`.
            // Dispatch FullText jobs to the full-text engine; MetadataOnly jobs
            // run the metadata-only path.
            let db = app_for_job.state::<DbState>();
            let result = match job.kind {
                TranslationJobKind::FullText => {
                    translate_full_text(&db.conn, &article_id, &client).await
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
                }
                Err(e) => {
                    eprintln!("[translation] job {article_id} failed: {e}");
                    let _ = app_for_job.emit(
                        "translation:complete",
                        serde_json::json!({ "articleId": article_id, "success": false, "error": e.to_string() }),
                    );
                }
            }
        }
    });

    TranslationWorkerHandle { sender }
}

/// Crash recovery: re-enqueue articles stranded in `queued` or `running` at
/// startup. Called once after the worker task is spawned so the jobs land in a
/// live channel. Non-fatal - errors are logged.
///
/// Each stranded article is re-enqueued with the correct `TranslationJobKind`
/// based on its `has_full_text` flag (`FullText` when true, else `MetadataOnly`)
/// so a stranded full-text job does not silently degrade to metadata-only on
/// restart (which would leave full text + chunks in the original language while
/// marking the article `is_translated = 1`).
pub fn reenqueue_stranded_on_startup(
    conn: &rusqlite::Connection,
    sender: &mpsc::Sender<TranslationJob>,
) {
    match article_repo::get_stranded_translation_articles(conn) {
        Ok(stranded) => {
            let count = stranded.len();
            for (id, has_full_text) in stranded {
                // Reset to 'queued' so the worker picks it up; if it was
                // 'running' the worker that owned it has died.
                let _ = article_repo::update_translation_status(conn, &id, "queued");
                let kind = if has_full_text {
                    TranslationJobKind::FullText
                } else {
                    TranslationJobKind::MetadataOnly
                };
                let _ = sender.try_send(TranslationJob { article_id: id, kind });
            }
            if count > 0 {
                eprintln!("[translation] re-enqueued {count} stranded job(s) on startup");
            }
        }
        Err(e) => {
            eprintln!("[translation] failed to query stranded jobs on startup: {e}");
        }
    }
}
