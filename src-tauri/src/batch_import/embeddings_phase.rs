//! Phase 5: Generate embeddings for articles after the full pipeline.
//!
//! Runs after Phase 4 (AI Summaries) so the title/abstract text is enriched
//! with the summary signal before embedding. Reuses
//! [`crate::embedding::runner::generate_embeddings_inner`] so the behavior is
//! identical to the `generate_embeddings` Tauri command (same director
//! staleness check, same 3-burst lock discipline, same v2 outer-JoinSet
//! parallelism).
//!
//! The phase targets the `included` corpus by default (the recall candidate
//! pool). Articles already embedded with a matching `input_hash` are skipped
//! by the director's staleness check, so this phase is idempotent and cheap
//! on repeat runs.
//!
//! # Pre-flight gate
//!
//! Mirrors Phase 4's `llm_configured_with_audit` pre-flight: if the LLM is not
//! configured, the phase skips with the canonical `"Skipped: LLM not
//! configured"` message + a system-level audit record so the skip surfaces in
//! Diagnostics rather than churning every article through the director's
//! base-condition gate.
//!
//! # v2 cancel-token bridge
//!
//! The batch-import state holds the cancel flag as `Arc<Mutex<bool>>` (so
//! per-item progress callbacks can fire synchronously inside the phase loops).
//! The v2 runner takes `Option<Arc<AtomicBool>>`. We bridge the two by
//! snapshotting `cancel_handle` into an `Arc<AtomicBool>` ONCE before calling
//! the runner. This is safe because:
//! - The pre-flight check above already polled `cancel_handle` and returned
//!   early if cancelled, so the snapshot starts `false`.
//! - The runner's outer `JoinSet::abort_all` now handles mid-run cancel
//!   cleanly: a Cancel click between `join_next()` completions aborts all
//!   in-flight article tasks, dropping their vectors (no DB writes from
//!   cancelled tasks). The previous live-mirror task (polling `cancel_handle`
//!   every 100ms to forward to an atomic) was a workaround for the sequential
//!   runner's inability to abort in-flight work; the v2 outer JoinSet makes it
//!   obsolete.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, State};

use crate::db::app_settings_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::embedding::director::EmbeddingScope;
use crate::embedding::runner::{
    generate_embeddings_inner, EmbeddingRunReport, HttpEmbeddingBatchSender,
};
use crate::llm::orchestrator::LlmOrchestrator;

use super::BatchImportPhaseResult;

/// Run Phase 5: generate embeddings for the `included` corpus.
///
/// Delegates to [`generate_embeddings_inner`] with `emit_events = false` (the
/// batch-import runner emits its own `batch-import:progress` events; the
/// per-article `embedding:progress` events are suppressed to avoid event spam
/// during a large-corpus backfill). The final `embedding:done` event IS
/// emitted by the runner so the frontend can show a completion toast.
///
/// `is_cancelled` is polled once before the phase starts; if cancelled, the
/// phase returns a skip result without entering the director (which would
/// acquire the DB lock). Mid-run cancellation is handled inside the runner via
/// the cancel-token bridge (see the module docs): a single `Arc<AtomicBool>`
/// snapshot is taken before the run, and the runner's outer `JoinSet::abort_all`
/// aborts in-flight tasks when the snapshot is observed set between
/// `join_next()` completions.
pub async fn run_embeddings_phase(
    db_state: &State<'_, DbState>,
    app_handle: &tauri::AppHandle,
    cancel_handle: Arc<Mutex<bool>>,
) -> BatchImportPhaseResult {
    // Pre-flight: LLM configured?
    if !llm_configured_with_audit(db_state) {
        return BatchImportPhaseResult {
            total: 0,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: vec!["Skipped: LLM not configured".to_string()],
        };
    }

    // Pre-flight: embeddings enabled?
    {
        let conn = match db_state.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                return BatchImportPhaseResult {
                    total: 0,
                    processed: 0,
                    succeeded: 0,
                    failed: 0,
                    errors: vec!["Phase 5: DB lock error".to_string()],
                };
            }
        };
        let status = app_settings_repo::get_embedding_status(&conn).unwrap_or_default();
        if status == app_settings_repo::EmbeddingStatus::Disabled {
            return BatchImportPhaseResult {
                total: 0,
                processed: 0,
                succeeded: 0,
                failed: 0,
                errors: vec!["Skipped: Embeddings disabled for this provider".to_string()],
            };
        }
    }

    // Check cancellation before starting.
    if *cancel_handle.lock().expect("batch import mutex") {
        return BatchImportPhaseResult {
            total: 0,
            processed: 0,
            succeeded: 0,
            failed: 0,
            errors: vec!["Skipped: Cancelled by user".to_string()],
        };
    }

    // v2 cancel-token bridge: snapshot `cancel_handle` into an `Arc<AtomicBool>`
    // ONCE. The runner's outer JoinSet + abort_all handles mid-run cancel
    // cleanly now, so the previous live-mirror task (polling cancel_handle
    // every 100ms) is obsolete and removed.
    //
    // NOTE: this snapshot means a Cancel click DURING the run is observed at
    // the next `join_next()` completion (when the runner re-checks the token),
    // not the 100ms mirror tick. This is acceptable: the outer JoinSet
    // completes articles faster than the sequential loop, and abort_all drops
    // in-flight vectors immediately on cancel. The ~100ms granularity matches
    // the per-article embedding latency (one LLM call + DB write), so the user
    // perceives near-instant cancellation.
    let cancel_atomic =
        Arc::new(AtomicBool::new(*cancel_handle.lock().expect("batch import mutex")));

    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    // Wrap the orchestrator into the v2 HttpEmbeddingBatchSender.
    let sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
    let scope = EmbeddingScope {
        article_ids: None,
        status_filter: Some("included".to_string()),
        force: false,
    };

    let report_result = generate_embeddings_inner(
        db_state,
        sender,
        scope,
        Some(app_handle),
        false, // suppress per-article events; emit the final `embedding:done`
        Some(Arc::clone(&cancel_atomic)),
    )
    .await;

    let report: EmbeddingRunReport = match report_result {
        Ok(r) => r,
        Err(e) => {
            return BatchImportPhaseResult {
                total: 0,
                processed: 0,
                succeeded: 0,
                failed: 1,
                errors: vec![format!("Phase 5 (Embeddings) error: {e}")],
            };
        }
    };

    // Map the report to a BatchImportPhaseResult. `generated` = succeeded,
    // `skipped` counts toward `processed` but not `succeeded` or `failed`.
    let processed = report.generated + report.skipped;
    let skip_reason = report.skip_reason.clone();
    let result = BatchImportPhaseResult {
        total: processed + report.errors,
        processed,
        succeeded: report.generated,
        failed: report.errors,
        errors: skip_reason.into_iter().collect(),
    };

    // Emit the final embedding:done event so the frontend settings listener
    // can show a completion toast even if the user navigated away.
    let _ = app_handle.emit("embedding:done", &report);

    result
}

/// Phase 5 skip message constant (mirrors Phase 3/4's
/// `LLM_NOT_CONFIGURED_SKIP_MSG`).
const LLM_NOT_CONFIGURED_SKIP_MSG: &str = "Skipped: LLM not configured";

/// Pre-flight LLM configuration check for Phase 5. Mirrors Phase 3/4's
/// `check_llm_configured_or_skip` so both LLM-gated phases report consistently.
pub fn llm_configured_with_audit(db_state: &State<'_, DbState>) -> bool {
    let conn = match db_state.conn.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    check_llm_configured_or_skip_conn(&conn).is_none()
}

/// Pure I/O over `&Connection` so the Phase 5 pre-flight gate is unit-testable
/// per `docs/CLAUDE.md` ("Prefer testing extracted logic over `#[tauri::command]`
/// shims"), mirroring Phase 3's `translations_phase::check_llm_configured_or_skip`.
///
/// Returns `Some(BatchImportPhaseResult)` (the skip result carrying the canonical
/// `"Skipped: LLM not configured"` message + a system-level audit record) when
/// the LLM is not configured, or `None` when it is (the phase should proceed).
pub fn check_llm_configured_or_skip_conn(
    conn: &rusqlite::Connection,
) -> Option<BatchImportPhaseResult> {
    if llm_config_repo::has_config(conn).unwrap_or(false) {
        return None;
    }
    let audit_detail = "Batch import Phase 5 (Embeddings) skipped: LLM not \
         configured. Configure an LLM provider in Settings to generate embeddings.";
    let _ = audit_repo::log_error(conn, audit_detail);
    Some(BatchImportPhaseResult {
        total: 0,
        processed: 0,
        succeeded: 0,
        failed: 0,
        errors: vec![LLM_NOT_CONFIGURED_SKIP_MSG.to_string()],
    })
}
