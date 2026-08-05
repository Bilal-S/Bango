//! In-memory translation queue and worker task. No `translation_jobs` table;
//! DB-backed progress on `articles` row (`translation_status` + `is_translated`).

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

/// Managed state handle for enqueuing jobs.
pub struct TranslationWorkerHandle {
    sender: mpsc::Sender<TranslationJob>,
}

impl TranslationWorkerHandle {
    /// Enqueue a translation job.
    pub fn try_send(&self, job: TranslationJob) -> Result<(), AppError> {
        self.sender
            .try_send(job)
            .map_err(|e| AppError::Import(format!("Translation worker unavailable: {e}")))
    }

    /// Borrow the sender. Used by `reenqueue_stranded_on_startup` during setup.
    pub fn sender(&self) -> &mpsc::Sender<TranslationJob> {
        &self.sender
    }
}

/// Spawn translation worker. Returns handle for enqueuing.
///
/// Worker reads jobs from channel, fetches LLM config + orchestrator, dispatches
/// to the engine, and emits `translation:complete`. Uses `tauri::async_runtime::spawn`
/// (called from sync `.setup()` closure; raw `tokio::spawn` would panic).
pub fn spawn_translation_worker(app: tauri::AppHandle) -> TranslationWorkerHandle {
    let (sender, mut receiver) = mpsc::channel::<TranslationJob>(64);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(job) = receiver.recv().await {
            let article_id = job.article_id.clone();
            let app_for_job = app_handle.clone();

            // Fetch LLM config + orchestrator under short DB lock.
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
                        // LLM not configured: mark failed + audit.
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
            }; // DB lock released before async LLM call.

            // context_window_tokens plumbed from LlmConfig for batch sizing.
            let context_window_tokens = config.context_window_tokens;
            let client = TranslationLlmClient { config, orchestrator, job_id: article_id.clone() };

            // Engine takes &Mutex<Connection>, locks in bursts across .await.
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
                    // Notify bus so waiters don't poll DB.
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
                    // Emit on failure too so waiters don't stall.
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

/// Max stranded jobs re-enqueued at startup. Set to 0: no auto-recovery;
/// stranded articles marked `failed` with retryable audit note. Set to positive
/// N to re-enable bounded re-enqueueing.
pub const STARTUP_STRANDED_CAP: usize = 0;

/// Crash recovery: mark stranded (queued/running) articles as failed and
/// optionally re-enqueue up to `STARTUP_STRANDED_CAP`. Non-fatal.
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
                // Reset to 'queued'.
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
                // Audit note for capped articles.
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
