//! Phase 4: Generate AI summaries for articles that got full text attached
//! in Phase 1 (and don't already have a summary).
//!
//! Reuses [`crate::commands::summary::generate_article_ai_summary_inner`] so the
//! behavior is identical to clicking the "Generate AI Summary" button in the
//! article detail panel (same prompt, same section-aware path, same events).
//!
//! # Parallel dispatch (Concern 1)
//!
//! The summaries are generated concurrently via a `tokio::task::JoinSet`. The
//! orchestrator's `max_concurrent_requests` semaphore bounds the real
//! concurrency; Phase 4 simply dispatches all article IDs up front and lets the
//! orchestrator gate the actual LLM calls. This mirrors the
//! `wiki::ingest::batching::run_chunked_ingest` pattern and replaces the
//! previous sequential `for` loop that left the configured concurrency unused.

use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::commands::summary::generate_article_ai_summary_inner;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::llm::orchestrator::LlmOrchestrator;

use super::BatchImportPhaseResult;

/// Run Phase 4: generate an AI summary for each article ID in `article_ids`
/// (the newly-attached articles from Phase 1).
///
/// Each summary is generated via [`generate_article_ai_summary_inner`], which
/// emits the standard `article-ai-summary-complete` / `-error` events so the
/// article detail panel and `useAiSummary` composable refresh automatically.
///
/// Articles are dispatched concurrently via a `JoinSet`; the orchestrator's
/// semaphore bounds real LLM concurrency. The caller's `is_cancelled` async
/// closure is polled before each spawn and on every completion; on cancel the
/// remaining tasks are aborted via `JoinSet::abort_all`.
///
/// `include_section_summaries` is forwarded to the summary core so the section-
/// aware path runs when the user has enabled "Section Summaries" in Settings.
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

    // Pre-flight: if LLM is not configured, skip the phase entirely with a
    // clear message + system audit record rather than failing per-article.
    // Mirrors the Phase 3 (Translations) pre-flight check so both LLM-gated
    // phases surface the same actionable message in Diagnostics.
    if !llm_configured_with_audit(db_state) {
        return BatchImportPhaseResult {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: vec!["Skipped: LLM not configured".to_string()],
        };
    }

    // If there is nothing to summarize, exit early without emitting the
    // batch-level "done" event (matches the previous behavior for the empty
    // case, though the loop below handles it correctly too).
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

    // Emit a final batch-level event so the frontend can show a completion toast
    // even if the user navigated away from the settings view (the per-article
    // events still fire too).
    let _ = app_handle.emit(
        "batch-import-summary-phase-done",
        serde_json::json!({ "succeeded": succeeded, "failed": failed, "total": total }),
    );

    BatchImportPhaseResult { total, processed, succeeded, failed, errors }
}

/// Pre-flight LLM configuration check for Phase 4.
///
/// Returns `true` when an LLM is configured (the phase should proceed
/// normally). Returns `false` when no LLM is configured, after writing a
/// system-level audit record (`article_id = NULL`, `action = 'error'`) so the
/// skip surfaces in Diagnostics with an actionable explanation.
///
/// Mirrors [`super::translations_phase::check_llm_configured_or_skip`] so both
/// LLM-gated phases report consistently.
pub fn llm_configured_with_audit(db_state: &State<'_, DbState>) -> bool {
    let conn = match db_state.conn.lock() {
        Ok(c) => c,
        Err(_) => {
            // Defer the error to the caller via the early-return path; here we
            // just return false so the caller emits its skip result. The
            // lock-error variant is reported by the caller's own skip message.
            return false;
        }
    };
    if llm_config_repo::has_config(&conn).unwrap_or(false) {
        return true;
    }
    let audit_detail = "Batch import Phase 4 (AI Summaries) skipped: LLM not \
         configured. Configure an LLM provider in Settings to generate summaries.";
    let _ = audit_repo::log_error(&conn, audit_detail);
    false
}
