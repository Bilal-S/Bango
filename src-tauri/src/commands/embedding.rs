//! Tauri commands for the embedding feature.
//!
//! - `generate_embeddings`: the primary parameterized runner (article_ids OR
//!   status_filter, default `included`; optional `force`).
//! - `recall_articles`: bounded cosine recall for the citation-finding feature.
//! - `get_embedding_status`: returns the triple-state + model + dimensions.
//! - `probe_embeddings`: explicitly probes the provider (used by Test Connection).

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
    /// The user's pinned embedding-model override (premium). `None` when the
    /// key is absent or empty (auto-detection active). Surfaced so the Settings
    /// UI can pre-fill the `EMBEDDING MODEL` input for premium users.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
}

/// Generate (or regenerate) embeddings for a set of articles.
///
/// - `article_ids`: when non-empty, embeds exactly those articles (overrides
///   `status_filter`). Used by the post-summary hook + batch import.
/// - `status_filter`: when `article_ids` is empty, embeds all articles with
///   this status (default `"included"`).
/// - `force`: when true, re-embeds every row regardless of the stored hash.
#[tauri::command]
pub async fn generate_embeddings(
    _db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_ids: Option<Vec<String>>,
    status_filter: Option<String>,
    force: Option<bool>,
) -> Result<EmbeddingRunReport, AppError> {
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    // Wrap the orchestrator into the v2 HttpEmbeddingBatchSender (the runner
    // takes `Arc<dyn EmbeddingBatchSender>` so its parallel + cancel behavior
    // is unit-testable without a live LLM provider).
    let sender: Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
        Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
    let scope = EmbeddingScope { article_ids, status_filter, force: force.unwrap_or(false) };
    // Spawn onto a background task so the IPC handler returns immediately and
    // the frontend listens to `embedding:progress` / `embedding:done` events.
    // The runner re-derives `DbState` from the `AppHandle` inside the task
    // (the `State` borrow cannot cross the spawn boundary).
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

/// Recall the top-K articles semantically related to `query`.
///
/// `status_filter` is a vec of status strings. When non-empty, the candidate
/// pool is scoped to articles in any of those statuses. When empty (or
/// omitted), no filter is applied. The previous `Option<String>` signature was
/// extended to `Vec<String>` for the Citation Finder, which needs `working +
/// included` while excluding `duplicate`/`rejected`.
///
/// Returns an empty vec when embeddings are disabled or the table is empty
/// (the citation-feature caller falls back).
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

/// Explicitly probe the provider for embedding support. Sets the triple-state
/// flag + model + dimensions. Used by `Test Connection` and any UI that wants
/// to re-evaluate after a config change.
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

/// Set the embedding-model override (premium-only).
///
/// When set to a non-empty model name, the probe tries this model first. When
/// set to `None`/empty, the override is cleared and auto-detection is restored.
/// On save, the embedding triple-state is reset to `unknown` so the next probe
/// (next embedding call or `Test Connection`) re-evaluates against the new
/// override.
///
/// Premium enforcement is defense-in-depth: the frontend hides the input for
/// non-premium users, and this command rejects the save with `AppError::Validation`
/// when `AppFlags.premium` is false.
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
