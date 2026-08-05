//! Tauri commands for the embedding feature.
//!
//! `generate_embeddings`: parameterized runner (article_ids or status_filter).
//! `recall_articles`: bounded cosine recall (Citation Finder).
//! `get_embedding_status`: triple-state + model + dimensions.
//! `probe_embeddings`: explicit probe (Test Connection).

use std::sync::Arc;

use serde::Serialize;
use tauri::{Manager, State};

use crate::db::app_settings_repo::{self, EmbeddingStatus};
use crate::db::connection::{lock_conn, DbState};
use crate::db::llm_config_repo;
use crate::embedding::director::EmbeddingScope;
use crate::embedding::recall::EmbeddingHit;
use crate::embedding::runner::{
    generate_embeddings_inner, EmbeddingRunReport, HttpEmbeddingBatchSender,
};
use crate::error::AppError;
use crate::llm::embedding::{probe_embedding_support, ProbeOutcome};
use crate::llm::orchestrator::LlmOrchestrator;

/// The status payload returned by `get_embedding_status` + `probe_embeddings`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingStatusInfo {
    pub status: String,
    pub model: String,
    pub dimensions: i32,
    /// User's pinned embedding-model override (premium). `None` = auto-detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
}

/// Model-mismatch payload. `None` (serialized as `null`) when stored rows match
/// the current model. `Some` when rows were generated with a different model,
/// triggering a confirmation dialog before search (since `recall` filters by
/// the new dimensions and would silently return zero hits).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModelMismatch {
    /// Current model from `app_settings.embedding_model`. Empty when probe hasn't run.
    pub current_model: String,
    /// First stored model that differs from current (case-insensitive ASCII).
    pub stored_model: String,
    /// Total embedding rows stored.
    pub stored_row_count: i64,
}

/// Given stored model names + the current model, return the first stored name
/// that does not match (case-insensitive ASCII). `None` when aligned or empty.
/// `#[must_use]` for unit-testing without a DB.
#[must_use]
pub fn first_mismatched_model(stored: &[String], current: Option<&str>) -> Option<String> {
    let current_str = current.unwrap_or("");
    // First pass: find a stored model whose name differs from the current
    // setting (case-insensitive ASCII comparison - model names are ASCII).
    let mismatched = stored.iter().find(|s| !s.eq_ignore_ascii_case(current_str));
    if let Some(m) = mismatched {
        return Some(m.clone());
    }
    // Second pass: if the current model is known but a stored row carries an
    // empty model name (pre-feature row, or corrupt), that's also a mismatch
    // so the row is flagged for regeneration + the column backfilled.
    if !current_str.is_empty() {
        if let Some(m) = stored.iter().find(|s| s.is_empty()) {
            return Some(m.clone());
        }
    }
    None
}

/// Generate (or regenerate) embeddings for a set of articles.
/// - `article_ids`: when non-empty, embeds exactly those articles (overrides
///   `status_filter`). Used by post-summary hook + batch import.
/// - `status_filter`: when `article_ids` is empty, scoped to this status
///   (default `"included"`).
/// - `force`: when true, re-embeds every row regardless of stored hash.
#[tauri::command]
pub async fn generate_embeddings(
    _db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_ids: Option<Vec<String>>,
    status_filter: Option<String>,
    force: Option<bool>,
) -> Result<EmbeddingRunReport, AppError> {
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    /* Wrap the orchestrator into HttpEmbeddingBatchSender so the runner's
     * parallel + cancel behavior is unit-testable without a live LLM. */
    let sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
    let scope = EmbeddingScope { article_ids, status_filter, force: force.unwrap_or(false) };
    /* Spawn background task so IPC returns immediately. Frontend listens to
     * `embedding:progress`/`embedding:done`. Runner re-derives DbState from
     * AppHandle inside the task (State borrow cannot cross spawn boundary). */
    let handle = app_handle.clone();
    tokio::task::spawn(async move {
        let st = handle.state::<DbState>();
        let _ =
            generate_embeddings_inner(&st, Arc::clone(&sender), scope, Some(&handle), true, None)
                .await;
    })
    .await
    .map_err(|e| AppError::Import(format!("embedding task panicked: {e}")))?;
    // Return a minimal report; the real result is delivered via `embedding:done`.
    Ok(EmbeddingRunReport {
        generated: 0,
        skipped: 0,
        errors: 0,
        status: "started".to_string(),
        model: String::new(),
        skip_reason: None,
    })
}

/// Recall top-K articles semantically related to `query`.
/// `status_filter`: when non-empty scopes the candidate pool to those statuses.
/// Returns empty vec when embeddings are disabled/empty (caller falls back).
#[tauri::command]
pub async fn recall_articles(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    query: String,
    top_k: Option<usize>,
    status_filter: Vec<String>,
) -> Result<Vec<EmbeddingHit>, AppError> {
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    crate::embedding::recall::recall(
        &db_state,
        &orchestrator,
        &query,
        top_k.unwrap_or(30),
        &status_filter,
    )
    .await
}

/// Read the current embedding capability status.
#[tauri::command]
pub fn get_embedding_status(db_state: State<'_, DbState>) -> Result<EmbeddingStatusInfo, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    let status = app_settings_repo::get_embedding_status(&conn)?;
    let model = app_settings_repo::get_embedding_model(&conn)?.unwrap_or_default();
    let dimensions = app_settings_repo::get_embedding_dimensions(&conn)?;
    let model_override = app_settings_repo::get_embedding_model_override(&conn)?;
    Ok(EmbeddingStatusInfo {
        status: status.as_str().to_string(),
        model,
        dimensions,
        model_override,
    })
}

/// Probe the provider for embedding support. Sets triple-state + model +
/// dimensions. Used by `Test Connection` and after config changes.
#[tauri::command]
pub async fn probe_embeddings(db_state: State<'_, DbState>) -> Result<ProbeOutcome, AppError> {
    // Read config (brief lock), release, then probe (HTTP), then persist (brief lock).
    let config = {
        let conn = lock_conn(&db_state.conn)?;
        llm_config_repo::get_config(&conn)?
    };
    let Some(cfg) = config else {
        let outcome = ProbeOutcome {
            status: "disabled".to_string(),
            model: String::new(),
            dimensions: 0,
            reason: "LLM not configured".to_string(),
        };
        return Ok(outcome);
    };
    // Forward the user's embedding-model override (premium) so the probe tries
    // it first, ahead of auto-detection.
    let override_model = {
        let conn = lock_conn(&db_state.conn)?;
        app_settings_repo::get_embedding_model_override(&conn)?
    };
    let outcome = probe_embedding_support(&cfg, override_model.as_deref()).await;
    let new_status = if outcome.status == "enabled" {
        EmbeddingStatus::Enabled
    } else {
        EmbeddingStatus::Disabled
    };
    let conn = lock_conn(&db_state.conn)?;
    app_settings_repo::set_embedding_status(&conn, new_status, &outcome.model, outcome.dimensions)?;
    Ok(outcome)
}

/// Detect whether stored embeddings were generated with a different model
/// than the current `embedding_model`. Returns `None` when aligned/empty.
/// Cheap (`SELECT DISTINCT model_name` + `COUNT(*)`) so it runs on every
/// Citation Finder submit.
#[tauri::command]
pub fn get_embedding_model_mismatch(
    db_state: State<'_, DbState>,
) -> Result<Option<EmbeddingModelMismatch>, AppError> {
    let conn = lock_conn(&db_state.conn)?;
    let current = app_settings_repo::get_embedding_model(&conn)?;
    let stored = crate::db::embedding_repo::list_distinct_model_names(&conn)?;
    let Some(mismatched) = first_mismatched_model(&stored, current.as_deref()) else {
        return Ok(None);
    };
    let stored_row_count = crate::db::embedding_repo::count_embeddings(&conn)?;
    Ok(Some(EmbeddingModelMismatch {
        current_model: current.unwrap_or_default(),
        stored_model: mismatched,
        stored_row_count,
    }))
}

/// Regenerate ALL embeddings from scratch. Deletes every row in
/// `article_embeddings` then re-runs `generate_embeddings_inner`. Used by the
/// Citation Finder's model-mismatch dialog and standalone Settings.
///
/// Delete-then-regenerate is clearer than `force=true` (no orphan rows when
/// chunk counts shrink). `status_filter` scopes both delete + regenerate;
/// empty = all statuses. Returns immediately; real result via events.
#[tauri::command]
pub async fn regenerate_embeddings(
    _db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    status_filter: Option<String>,
) -> Result<EmbeddingRunReport, AppError> {
    /* Phase 1: scoped delete (brief lock). Must run BEFORE the runner re-derives
     * its work list so the director sees an empty table. Scoped to the same
     * status filter the runner uses, so a Citation-Finder regenerate doesn't
     * wipe rows generated for other statuses. */
    {
        let db = app_handle.state::<DbState>();
        let conn = lock_conn(&db.conn)?;
        if let Some(ref filter) = status_filter {
            /* Build `status IN (?, ?, ?)` from comma-joined filter (matches
             * `EmbeddingScope.status_filter`'s single-string contract). All
             * statuses are bound as params - no SQL injection. */
            let statuses: Vec<&str> =
                filter.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
            if statuses.is_empty() {
                crate::db::embedding_repo::delete_all_embeddings(&conn)?;
            } else {
                let placeholders: Vec<&str> = (0..statuses.len()).map(|_| "?").collect();
                let in_clause = placeholders.join(", ");
                let sql = format!(
                    "DELETE FROM article_embeddings \
                     WHERE article_id IN (SELECT id FROM articles WHERE status IN ({in_clause}))"
                );
                let pairs: Vec<&dyn rusqlite::ToSql> =
                    statuses.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                conn.execute(&sql, rusqlite::params_from_iter(pairs.iter()))?;
            }
        } else {
            crate::db::embedding_repo::delete_all_embeddings(&conn)?;
        }
    }

    // Phase 2: re-embed (background task; emits `embedding:progress`/`done`).
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    let sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
    /* `force=false` is correct: the delete above emptied relevant rows, so the
     * director's hash-comparison naturally produces full work. (force=true
     * would work but bypass the model-mismatch signal in the report.) */
    let scope = EmbeddingScope { article_ids: None, status_filter, force: false };
    let handle = app_handle.clone();
    tokio::task::spawn(async move {
        let st = handle.state::<DbState>();
        let _ =
            generate_embeddings_inner(&st, Arc::clone(&sender), scope, Some(&handle), true, None)
                .await;
    })
    .await
    .map_err(|e| AppError::Import(format!("regenerate task panicked: {e}")))?;
    Ok(EmbeddingRunReport {
        generated: 0,
        skipped: 0,
        errors: 0,
        status: "started".to_string(),
        model: String::new(),
        skip_reason: None,
    })
}

/// Set the embedding-model override (premium-only). Non-empty: probe tries
/// this model first. None/empty: clear, restore auto-detection. Resets
/// embedding triple-state to `unknown` so next probe re-evaluates.
/// Premium gate is defense-in-depth (frontend hides input + command rejects).
#[tauri::command]
pub fn set_embedding_model_override(
    db_state: State<'_, DbState>,
    flags: State<'_, crate::AppFlags>,
    value: Option<String>,
) -> Result<(), AppError> {
    if !flags.premium {
        return Err(AppError::Validation(
            "Embedding model override is a premium feature".to_string(),
        ));
    }
    let conn = lock_conn(&db_state.conn)?;
    app_settings_repo::set_embedding_model_override(&conn, value.as_deref())?;
    // Reset the capability state so the next probe re-evaluates with the new
    // override model (the prior status/model/dimensions may no longer apply).
    app_settings_repo::reset_embedding_status(&conn)?;
    Ok(())
}
