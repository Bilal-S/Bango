use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;
use crate::models::llm_config::{LlmConfig, LlmProvider};

#[tauri::command]
pub fn get_llm_config(db_state: State<'_, DbState>) -> Result<Option<LlmConfig>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    llm_config_repo::get_config(&conn)
}

/// Fields of `LlmConfig` that the embedding capability depends on. A change in
/// any of these resets `embedding_status`/`model`/`dimensions` to `unknown` for
/// re-evaluation. Parameters-only edits (concurrency, delay, context window,
/// temperature) are NOT in this list: resetting on a parameters-only save would
/// discard a known-good `enabled`/`disabled` state. This was the root cause of
/// the "probe fires on first Citation Finder call" bug.
///
/// Pure `#[must_use]` so the field-set is unit-testable.
#[must_use]
pub fn embedding_relevant_changed(prev: &LlmConfig, next: &LlmConfig) -> bool {
    prev.provider != next.provider
        || prev.endpoint_url != next.endpoint_url
        || prev.model_name != next.model_name
        || prev.api_key_encrypted != next.api_key_encrypted
}

#[tauri::command]
pub async fn save_llm_config(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    config: LlmConfig,
) -> Result<(), AppError> {
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        /* Only reset embedding capability when a field it depends on
        (`provider` / `endpoint_url` / `model_name` / `api_key_encrypted`)
        actually changed. Parameters-only edits do NOT affect embedding
        capability, so resetting here would discard a known-good
        `enabled`/`disabled` state. See `embedding_relevant_changed`. */
        let prev = llm_config_repo::get_config(&conn)?;
        let needs_reset = prev.as_ref().is_none_or(|p| embedding_relevant_changed(p, &config));
        llm_config_repo::save_config(&conn, &config)?;
        if needs_reset {
            /* Reset to `unknown` so the next `generate_embeddings` or
            `probe_embeddings` call re-probes against the new provider/
            endpoint/model. Keeps model/dimensions so Settings still shows
            what was last working. */
            let _ = crate::db::app_settings_repo::reset_embedding_status(&conn);
        }
    } // conn dropped here

    // Update the in-memory orchestrator with new concurrency/delay settings
    orchestrator
        .update_settings(config.max_concurrent_requests as usize, config.request_delay_ms as u64)
        .await;

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
    /// Embedding capability outcome from the synchronous probe. `None` when
    /// the connection test failed (no probe ran). `"enabled"` / `"disabled"`
    /// when the probe ran so the frontend can show a badge + the model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_status: Option<String>,
    /// The working embedding model name (only set when `embedding_status ==
    /// Some("enabled")`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

#[tauri::command]
pub async fn test_llm_connection(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    _app_handle: tauri::AppHandle,
) -> Result<TestConnectionResult, AppError> {
    let (config, embedding_override) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let cfg = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("No LLM config found".to_string()))?;
        let ov = crate::db::app_settings_repo::get_embedding_model_override(&conn)?;
        (cfg, ov)
    };

    /* First attempt: use config as-is (temperature included unless already
    skipped). The client-level `send_with_temperature_recovery` may
    transparently retry without temperature and return Ok with
    `CallMeta.temperature_was_rejected = true`. Detect this to persist
    the flag + show the auto-adjusted toast. */
    match orchestrator.test_connection(&config).await {
        Ok((_, _, meta)) => {
            if meta.temperature_was_rejected && !config.skip_temperature {
                /* The client recovered from a temperature-rejection 400.
                Persist `skip_temperature = true` so future calls skip the
                wasteful first-attempt failure. The orchestrator's
                in-session latch is already flipped. */
                let mut retry_config = config.clone();
                retry_config.skip_temperature = true;
                {
                    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                    llm_config_repo::save_config(&conn, &retry_config)?;
                }
                /* Probe embedding support synchronously so the response
                includes the outcome. Forward the embedding-model override
                (premium) so the probe tries it first. */
                let (emb_status, emb_model, emb_dims, emb_suffix) =
                    probe_embeddings_sync(&retry_config, embedding_override.as_deref()).await;
                /* Persist the probe outcome. Forwards real dimensions so
                `recall` (gated on `dimensions > 0`) works immediately
                instead of waiting for the first `generate_embeddings`. */
                persist_embedding_probe(&db_state, &emb_status, &emb_model, emb_dims);
                Ok(TestConnectionResult {
                    success: true,
                    message: format!(
                        "Connection successful! (temperature not supported by this model - auto-adjusted){emb_suffix}"
                    ),
                    embedding_status: emb_status,
                    embedding_model: emb_model,
                })
            } else {
                /* Plain success: probe embedding support synchronously so the
                response includes the outcome. Forward the embedding-model
                override (premium) so the probe tries it first. */
                let (emb_status, emb_model, emb_dims, emb_suffix) =
                    probe_embeddings_sync(&config, embedding_override.as_deref()).await;
                persist_embedding_probe(&db_state, &emb_status, &emb_model, emb_dims);
                Ok(TestConnectionResult {
                    success: true,
                    message: format!("Connection successful!{emb_suffix}"),
                    embedding_status: emb_status,
                    embedding_model: emb_model,
                })
            }
        }
        Err(e) => {
            let err_msg = format!("{e}");

            /* Check if the error is temperature-related (pre-client-recovery
            fallback: some providers may still surface the 400 as an error). */
            if err_msg.contains("temperature") && !config.skip_temperature {
                // Retry without temperature
                let mut retry_config = config.clone();
                retry_config.skip_temperature = true;

                match orchestrator.test_connection(&retry_config).await {
                    Ok(_) => {
                        // Save the updated config so future calls skip temperature
                        {
                            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                            llm_config_repo::save_config(&conn, &retry_config)?;
                        }
                        /* Probe embedding support synchronously. Forward the
                        embedding-model override (premium) so the probe
                        tries it first. */
                        let (emb_status, emb_model, emb_dims, emb_suffix) =
                            probe_embeddings_sync(&retry_config, embedding_override.as_deref())
                                .await;
                        persist_embedding_probe(&db_state, &emb_status, &emb_model, emb_dims);
                        Ok(TestConnectionResult {
                            success: true,
                            message: format!(
                                "Connection successful! (temperature not supported by this model - auto-adjusted){emb_suffix}"
                            ),
                            embedding_status: emb_status,
                            embedding_model: emb_model,
                        })
                    }
                    Err(retry_err) => Ok(TestConnectionResult {
                        success: false,
                        message: format!("Connection failed: {retry_err}"),
                        embedding_status: None,
                        embedding_model: None,
                    }),
                }
            } else {
                Ok(TestConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                    embedding_status: None,
                    embedding_model: None,
                })
            }
        }
    }
}

/// Persist the probe outcome to `app_settings` in a brief lock burst (any
/// `State` borrow already dropped). `status` = `"enabled"` / `"disabled"`.
fn persist_embedding_probe(
    db_state: &State<'_, DbState>,
    status: &Option<String>,
    model: &Option<String>,
    dimensions: i32,
) {
    if let Ok(conn) = crate::db::connection::lock_conn(&db_state.conn) {
        let _ = persist_embedding_probe_to_conn(&conn, status, model, dimensions);
    }
}

/// DB-write core of [`persist_embedding_probe`]. Takes `&Connection` directly
/// so the dimension-forwarding contract is regression-testable. Fix: the prior
/// shape hardcoded `dimensions = 0`, leaving `recall` gated off until the first
/// `generate_embeddings` call. `pub` so integration tests can exercise it.
pub fn persist_embedding_probe_to_conn(
    conn: &rusqlite::Connection,
    status: &Option<String>,
    model: &Option<String>,
    dimensions: i32,
) -> Result<(), crate::error::AppError> {
    let st = if status.as_deref() == Some("enabled") {
        crate::db::app_settings_repo::EmbeddingStatus::Enabled
    } else {
        crate::db::app_settings_repo::EmbeddingStatus::Disabled
    };
    /* Forward the real dimensions so `recall` (gated on `dimensions > 0`)
    works immediately. Disabled probes carry `dimensions = 0` (correct:
    no vectors returned). */
    crate::db::app_settings_repo::set_embedding_status(
        conn,
        st,
        model.as_deref().unwrap_or(""),
        dimensions,
    )
}
/// Runs the embedding probe HTTP call, returns the outcome tuple. Does NOT
/// persist (caller does that after the `.await` in a brief lock burst, keeping
/// the function `Send`). `override_model`: premium pinned model name; tried
/// first with auto-detection fallback on failure.
async fn probe_embeddings_sync(
    config: &LlmConfig,
    override_model: Option<&str>,
) -> (Option<String>, Option<String>, i32, String) {
    let outcome = crate::llm::embedding::probe_embedding_support(config, override_model).await;

    /* Build the response fields + message suffix (no DB access). Forwards
    real dimensions so `recall` works immediately instead of waiting for
    the first `generate_embeddings` call. */
    if outcome.status == "enabled" {
        (
            Some("enabled".to_string()),
            Some(outcome.model.clone()),
            outcome.dimensions,
            format!(" Embeddings enabled: using {}.", outcome.model),
        )
    } else {
        (
            Some("disabled".to_string()),
            None,
            0,
            format!(" Embeddings disabled: {}.", outcome.reason),
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListModelsRequest {
    pub provider: LlmProvider,
    pub endpoint_url: String,
    pub api_key: Option<String>,
}

#[tauri::command]
pub async fn list_llm_models(
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    request: ListModelsRequest,
) -> Result<Vec<String>, AppError> {
    /* Route through orchestrator (concurrency + rate limiting per
    CLAUDE.md: "All LLM calls MUST go through LlmOrchestrator"). */
    orchestrator
        .list_models(&request.provider, &request.endpoint_url, request.api_key.as_deref())
        .await
}

#[tauri::command]
pub fn has_llm_config(db_state: State<'_, DbState>) -> Result<bool, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    llm_config_repo::has_config(&conn)
}
