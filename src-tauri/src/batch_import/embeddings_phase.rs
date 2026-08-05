//! Phase 5: Generate embeddings after Phase 4. Reuses
//! [`crate::embedding::runner::generate_embeddings_inner`] (same director
//! staleness, 3-burst lock, v2 outer-JoinSet parallelism). Targets `included`
//! corpus; idempotent via `input_hash` staleness.
//!
//! # Pre-flight gate
//!
//! LLM-configured + embeddings-not-disabled gate with system audit record,
//! mirroring Phase 4's `llm_configured_with_audit`.
//!
//! # v2 cancel-token bridge
//!
//! Batch-import cancel flag is `Arc<Mutex<bool>>` (sync for per-item
//! callbacks). The v2 runner takes `Option<Arc<AtomicBool>>`. We snapshot
//! `cancel_handle` into `Arc<AtomicBool>` ONCE before calling the runner
//! because the pre-flight already polled it and `JoinSet::abort_all` handles
//! mid-run cancel cleanly (drops in-flight vectors with no DB writes).

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

/// Run Phase 5: generate embeddings. Delegates to
/// [`generate_embeddings_inner`] with `emit_events = false` (batch runner
/// emits its own `batch-import:progress`). The runner still emits
/// `embedding:done`.
///
/// Cancel polled once pre-flight; mid-run handled inside the runner via
/// `JoinSet::abort_all` on the snapshotted `Arc<AtomicBool>`.
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

    /* v2 cancel-token bridge: snapshot `cancel_handle` into `Arc<AtomicBool>`
    ONCE. The runner's JoinSet + abort_all handles mid-run cancel; Cancel
    takes effect at next `join_next()` completion (~100ms granularity =
    one LLM call + DB write per article). The previous live-mirror task
    (polling every 100ms) is obsolete. */
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

    // Map report: generated=succeeded, skipped counts toward processed only.
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

/// Skip message constant (mirrors Phase 3/4).
const LLM_NOT_CONFIGURED_SKIP_MSG: &str = "Skipped: LLM not configured";

/// Pre-flight LLM config check for Phase 5. Mirrors Phase 3/4.
pub fn llm_configured_with_audit(db_state: &State<'_, DbState>) -> bool {
    let conn = match db_state.conn.lock() {
        Ok(c) => c,
        Err(_) => return false,
    };
    check_llm_configured_or_skip_conn(&conn).is_none()
}

/// Pure I/O over `&Connection` for unit-testability. Returns
/// `Some(skip_result)` (with audit record) when LLM not configured, `None`
/// when it is (phase should proceed).
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
