//! Phase 4: Generate AI summaries for articles that got full text attached
//! in Phase 1 (and don't already have a summary).
//!
//! Reuses [`crate::commands::summary::generate_article_ai_summary_inner`] so the
//! behavior is identical to clicking the "Generate AI Summary" button in the
//! article detail panel (same prompt, same section-aware path, same events).

use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::commands::summary::generate_article_ai_summary_inner;
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
/// The caller's `is_cancelled` async closure is checked before each LLM call so
/// the runner can abort the phase without cancelling an in-flight request.
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
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();

    // Pre-flight: if LLM is not configured, skip the phase entirely with a
    // clear message rather than failing per-article.
    let llm_configured = {
        let conn = match db_state.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                return BatchImportPhaseResult {
                    total,
                    processed: 0,
                    succeeded: 0,
                    failed: total,
                    errors: vec!["Failed to acquire DB lock to check LLM config".to_string()],
                };
            }
        };
        llm_config_repo::has_config(&conn).unwrap_or_default()
    };
    if !llm_configured {
        return BatchImportPhaseResult {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: vec!["Skipped: LLM not configured".to_string()],
        };
    }

    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for article_id in &article_ids {
        // Check cancellation before each LLM call.
        if is_cancelled().await {
            break;
        }

        on_progress(
            processed,
            total,
            &format!(
                "Phase 4 - AI Summaries - found {total} articles - summarizing {} of {total}",
                processed + 1
            ),
        );

        processed += 1;
        match generate_article_ai_summary_inner(
            db_state,
            app_handle,
            &orchestrator,
            article_id,
            include_section_summaries,
        )
        .await
        {
            Ok(_) => succeeded += 1,
            Err(e) => {
                failed += 1;
                errors.push(format!("AI summary failed for article {article_id}: {e}"));
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
