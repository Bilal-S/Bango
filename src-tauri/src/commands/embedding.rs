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

/// Model-mismatch payload returned by `get_embedding_model_mismatch`. `None`
/// (serialized as `null`) when there is no mismatch (or no embeddings stored).
/// `Some` when stored rows were generated with a different `model_name` than
/// the current `embedding_model` setting, so the user gets a confirmation
/// dialog before searching (which would silently return zero hits because
/// `recall` filters by the new dimensions).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModelMismatch {
    /// The model name currently set in `app_settings.embedding_model` (set by
    /// the probe). Empty when the probe has not run yet.
    pub current_model: String,
    /// One of the stored model names that differs from `current_model`. When
    /// multiple distinct stored models exist (e.g. the user switched models
    /// twice without regenerating), the first non-matching one is reported -
    /// the dialog's CTA ("regenerate to be consistent") is the same regardless.
    pub stored_model: String,
    /// Total embedding rows currently stored (context for the dialog's
    /// "this will re-embed N rows" message).
    pub stored_row_count: i64,
}

/// Pure helper: given the distinct stored model names + the current model,
/// return the first stored name that does not match (case-insensitive ASCII).
/// `None` when there is no mismatch or nothing is stored.
///
/// Extracted as `#[must_use]` so the mismatch-detection logic is unit-testable
/// in isolation without a DB.
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

/// Detect whether stored embeddings were generated with a different model
/// than the current `embedding_model` setting. Returns `None` (serialized as
/// `null`) when there is no mismatch (rows are fresh or the table is empty).
/// Returns `Some(EmbeddingModelMismatch)` when the user switched embedding
/// models since the last `generate_embeddings` run - in that case the Citation
/// Finder shows a confirmation dialog before searching because `recall`
/// filters by the new dimensions and would silently return zero hits.
///
/// The check is intentionally cheap (one `SELECT DISTINCT model_name` + one
/// `COUNT(*)`) so it can run on every Citation Finder submit without a
/// perceptible delay.
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
/// `article_embeddings` then re-runs `generate_embeddings_inner` (which probes
/// + embeds every article in the scope). Used by the Citation Finder's
/// model-mismatch confirmation dialog ("Regenerate" button) and as a future
/// standalone "Regenerate embeddings" Settings affordance.
///
/// The delete-then-regenerate path is clearer than `force=true` re-embed
/// because the latter leaves orphan rows when the per-article chunk count
/// shrinks (e.g. an article's full text was edited down). A clean delete
/// guarantees the table reflects only the current corpus + current model.
///
/// `status_filter` scopes the regeneration (default `"included"` - the
/// Citation Finder's default candidate pool). The delete is ALSO scoped to
/// those statuses so a regenerate-from-Citation-Finder does not wipe
/// embeddings the user may have generated for other statuses via the
/// standalone Settings command. Pass an empty filter to delete + regenerate
/// across all statuses.
///
/// Returns immediately after spawning the background task; the frontend
/// listens to `embedding:progress` / `embedding:done` for the real result.
#[tauri::command]
pub async fn regenerate_embeddings(
    _db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    status_filter: Option<String>,
) -> Result<EmbeddingRunReport, AppError> {
    // Phase 1: scoped delete (brief lock). The delete must run BEFORE the
    // runner re-derives its work list so the director sees an empty table and
    // produces full work. We scope the delete to the same status filter the
    // runner will use so a Citation-Finder regenerate does not wipe rows the
    // user generated for other statuses via the standalone command.
    {
        let db = app_handle.state::<DbState>();
        let conn = lock_conn(&db.conn)?;
        if let Some(ref filter) = status_filter {
            // Build `status IN (?, ?, ?)` - split the comma-joined filter the
            // runner uses (matches `EmbeddingScope.status_filter`'s single-
            // string contract). Every status is bound as a parameter so no
            // SQL injection.
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
    // `force=false` is correct here because the delete above emptied the
    // relevant rows, so the director's hash-comparison naturally produces full
    // work. (force=true would also work but would bypass the model-mismatch
    // signal in the director's report.)
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
