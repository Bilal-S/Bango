//! Phase 4: Generate AI summaries for articles newly attached in Phase 1
//! (without existing summaries). Reuses
//! [`crate::commands::summary::generate_article_ai_summary_inner`] (identical
//! to the "Generate AI Summary" button).
//!
//! # Parallel dispatch (Concern 1)
//!
//! Concurrent via `tokio::task::JoinSet`; the orchestrator's
//! `max_concurrent_requests` semaphore bounds real concurrency. Mirrors
//! `wiki::ingest::batching::run_chunked_ingest`.

use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::commands::summary::generate_article_ai_summary_inner;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::llm::orchestrator::LlmOrchestrator;

use super::BatchImportPhaseResult;

/// Run Phase 4: generate AI summaries concurrently via `JoinSet`. Each task
/// calls [`generate_article_ai_summary_inner`]; orchestrator semaphore bounds
/// concurrency. `is_cancelled` polled before each spawn and on completion;
/// remaining tasks aborted on cancel via `JoinSet::abort_all`.
pub async fn run_summary_phase<F, Fut, P>(
    db_state: &State<'_, DbState>,
    app_handle: &tauri::AppHandle,
    article_ids: Vec<String>,
    include_section_summaries: bool,
    on_progress: &mut P,
    is_cancelled: F,
) -> BatchImportPhaseResult
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
    P: FnMut(usize, usize, &str),
{
    let total = article_ids.len();

    /* Pre-flight: if LLM not configured, skip with audit record (mirrors
    Phase 3). */
    if !llm_configured_with_audit(db_state) {
        return BatchImportPhaseResult {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: vec!["Skipped: LLM not configured".to_string()],
        };
    }

    // Exit early when nothing to summarize (matches previous behavior).
    if total == 0 {
        return BatchImportPhaseResult {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: Vec::new(),
        };
    }

    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // Dispatch each article into a JoinSet. Each task resolves DbState +
    // LlmOrchestrator from the AppHandle inside the task (the &State borrow
    // cannot cross the spawn boundary).
    let mut join_set: tokio::task::JoinSet<Result<String, String>> = tokio::task::JoinSet::new();

    for article_id in article_ids {
        // Check cancellation before each spawn. If the user cancelled, stop
        // dispatching and let the drain loop below abort anything already in
        // flight.
        if is_cancelled().await {
            break;
        }

        let app_for_task = app_handle.clone();
        let include_sections = include_section_summaries;
        join_set.spawn(async move {
            let db_state = app_for_task.state::<DbState>();
            let orchestrator = app_for_task.state::<Arc<LlmOrchestrator>>().inner().clone();
            generate_article_ai_summary_inner(
                &db_state,
                &app_for_task,
                &orchestrator,
                &article_id,
                include_sections,
            )
            .await
            .map(|_| article_id.clone())
            .map_err(|e| format!("AI summary failed for article {article_id}: {e}"))
        });
    }

    // Drain results as tasks complete. Progress is emitted per completion so
    // the bar advances smoothly even though the LLM calls finish out of order.
    while let Some(res) = join_set.join_next().await {
        // Check cancellation on each completion; abort remaining tasks.
        if is_cancelled().await {
            join_set.abort_all();
            // Drain any already-ready results so the JoinSet is not dropped
            // with pending tasks (which would also abort them, but this makes
            // the intent explicit and avoids a Tokio warning).
            while join_set.join_next().await.is_some() {}
            break;
        }

        processed += 1;
        on_progress(
            processed,
            total,
            &format!("Phase 4 - AI Summaries - completed {processed} of {total} article summaries"),
        );

        match res {
            Ok(Ok(_article_id)) => succeeded += 1,
            Ok(Err(e)) => {
                failed += 1;
                errors.push(e);
            }
            Err(join_err) => {
                failed += 1;
                errors.push(format!("Summary task panicked: {join_err}"));
            }
        }
    }

    // Emit batch-level event so frontend shows completion toast even if user
    // navigated away (per-article events still fire).
    let _ = app_handle.emit(
        "batch-import-summary-phase-done",
        serde_json::json!({ "succeeded": succeeded, "failed": failed, "total": total }),
    );

    BatchImportPhaseResult { total, processed, succeeded, failed, errors }
}

/// Pre-flight LLM check for Phase 4. Returns `true` when configured (proceed);
/// `false` when not, after writing a system-level audit record so the skip
/// surfaces in Diagnostics. Mirrors Phase 3.
pub fn llm_configured_with_audit(db_state: &State<'_, DbState>) -> bool {
    let conn = match db_state.conn.lock() {
        Ok(c) => c,
        Err(_) => return false, // lock-error surfaced by caller's skip message
    };
    if llm_config_repo::has_config(&conn).unwrap_or(false) {
        return true;
    }
    let audit_detail = "Batch import Phase 4 (AI Summaries) skipped: LLM not \
         configured. Configure an LLM provider in Settings to generate summaries.";
    let _ = audit_repo::log_error(&conn, audit_detail);
    false
}
