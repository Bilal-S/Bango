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

/// The fields of `LlmConfig` that the embedding capability depends on. A
/// change in any of these means the previously-probed
/// `embedding_status`/`model`/`dimensions` may no longer be accurate, so the
/// triple-state flag must be reset to `unknown` for re-evaluation.
///
/// Parameters-only edits (concurrency / request delay / context window /
/// temperature) are intentionally NOT in this list: they do not affect
/// embedding capability, so resetting the flag on a parameters-only save
/// would discard a known-good `enabled`/`disabled` state and force the next
/// embedding call (e.g. Citation Finder Phase B) to re-probe redundantly.
/// This was the root cause of the "probe fires on first Citation Finder
/// call" bug: the Settings parameters auto-save watcher fired
/// `save_llm_config` after Test Connection, which unconditionally reset the
/// status the probe had just set to `enabled`.
///
/// Pure `#[must_use]` so the field-set is unit-testable in isolation.
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
        // Only reset the embedding capability when a field it depends on
        // (`provider` / `endpoint_url` / `model_name` / `api_key_encrypted`)
        // actually changed. A parameters-only edit (concurrency / delay /
        // context window / temperature) does NOT affect embedding capability,
        // so resetting here would discard a known-good `enabled`/`disabled`
        // state (e.g. one just set by `test_llm_connection`) and force the
        // next embedding call to re-probe redundantly. See
        // `embedding_relevant_changed` for the bug context.
        let prev = llm_config_repo::get_config(&conn)?;
        let needs_reset = prev.as_ref().is_none_or(|p| embedding_relevant_changed(p, &config));
        llm_config_repo::save_config(&conn, &config)?;
        if needs_reset {
            // Reset the triple-state flag to `unknown` so the next
            // `generate_embeddings` or `probe_embeddings` call re-probes
            // against the new provider/endpoint/model. Keeps the
            // model/dimensions so the Settings UI can still show what was
            // last working.
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

    // First attempt: use config as-is (temperature included unless already
    // skipped). The client-level recovery (`send_with_temperature_recovery`)
    // may transparently retry without temperature and return Ok with
    // `CallMeta.temperature_was_rejected = true`. We detect that here so we
    // can persist the flag + show the auto-adjusted toast - without this,
    // the recovery would be silent and screening batch 1 would rediscover
    // the rejection (the regression fixed in this change).
    match orchestrator.test_connection(&config).await {
        Ok((_, _, meta)) => {
            if meta.temperature_was_rejected && !config.skip_temperature {
                // The client recovered from a temperature-rejection 400. Persist
                // `skip_temperature = true` so future calls (screening, chat,
                // summaries) skip the wasteful first-attempt failure. The
                // orchestrator's in-session latch is already flipped inside
                // `test_connection`.
                let mut retry_config = config.clone();
                retry_config.skip_temperature = true;
                {
                    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                    llm_config_repo::save_config(&conn, &retry_config)?;
                }
                // Test Connection succeeded: probe embedding support
                // synchronously so the response payload + message include the
                // outcome (critique 2.4). Forward the user's embedding-model
                // override (premium) so the probe tries it first.
                let (emb_status, emb_model, emb_dims, emb_suffix) =
                    probe_embeddings_sync(&retry_config, embedding_override.as_deref()).await;
                // Persist the probe outcome (brief lock burst). Forwards the
                // real dimensions from the probe so `recall` (which gates on
                // `dimensions > 0`) works immediately after Test Connection
                // instead of waiting for the first `generate_embeddings` call.
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
                // Plain success: probe embedding support synchronously so the
                // response payload + message include the outcome (critique 2.4).
                // Forward the user's embedding-model override (premium) so the
                // probe tries it first.
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

            // Check if the error is temperature-related (pre-client-recovery
            // fallback: some providers may still surface the 400 as an error
            // if the retry-without-temperature path also fails).
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
                        // Probe embedding support synchronously (critique 2.4).
                        // Forward the user's embedding-model override (premium)
                        // so the probe tries it first.
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

/// Synchronous embedding probe. Runs the probe HTTP call inline (not
/// fire-and-forget) so the caller can include the outcome in its response
/// payload + message. Persists the outcome to `app_settings` after the probe
/// completes (brief lock burst, not held across the HTTP call).
/// Returns `(status, model, message_suffix)` where:
/// - `status` = `Some("enabled")` / `Some("disabled")`.
/// - `model` = `Some(model_name)` only when enabled.
/// - `message_suffix` = a human-readable fragment appended to the Test
///   Connection message, e.g. `" Embeddings enabled: using text-embedding-3-small."`
///   or `" Embeddings disabled: provider does not support embeddings."`.
///
/// The probe is one tiny HTTP call (embeds the word "probe"), adding ~1-2s to
/// the Test Connection flow - acceptable within the existing "Testing…"
/// spinner UX. All probe errors are swallowed (mapped to `disabled`) so a probe
/// failure never fails the Test Connection result.
///
/// Persist the probe outcome to `app_settings`. Called by the command handler
/// after `probe_embeddings_sync` returns, so the `State` borrow is not held
/// across any `.await`. `status` = `"enabled"` / `"disabled"`; `model` is the
/// working model name (empty when disabled).
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

/// The DB-write core of [`persist_embedding_probe`], taking a `&Connection`
/// directly so the dimension-forwarding contract is regression-testable
/// without constructing a Tauri `State<DbState>`. The bug this split fixes:
/// the prior shape hardcoded `dimensions = 0`, which left `recall` gated off
/// (`dimensions <= 0`) until the first `generate_embeddings` call.
///
/// `pub` (not `pub(crate)`) so integration tests under `src-tauri/tests/` can
/// exercise the dimension-forwarding contract directly.
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
    // Forward the real dimensions from `ProbeOutcome` so `recall` (which
    // gates on `dimensions > 0`) works immediately after Test Connection.
    // Disabled probes carry `dimensions = 0` from `probe_embeddings_sync`,
    // which is correct (no vectors were returned).
    crate::db::app_settings_repo::set_embedding_status(
        conn,
        st,
        model.as_deref().unwrap_or(""),
        dimensions,
    )
}
/// Runs the embedding probe HTTP call and returns the outcome tuple. Does NOT
/// persist - the caller owns the `State<DbState>` and persists after the probe
/// returns (brief lock burst). This split keeps the function `Send` (no `State`
/// borrow held across the `.await`).
///
/// `override_model` is the premium user's pinned embedding model name (read
/// once from `app_settings` by the caller). When `Some` + non-empty, the probe
/// tries it first and falls back to auto-detection on failure.
async fn probe_embeddings_sync(
    config: &LlmConfig,
    override_model: Option<&str>,
) -> (Option<String>, Option<String>, i32, String) {
    let outcome = crate::llm::embedding::probe_embedding_support(config, override_model).await;

    // Build the response fields + message suffix (no DB access here). The
    // dimensions are forwarded so `persist_embedding_probe` can store the real
    // value (the previous shape dropped them, hardcoding 0, which left
    // `recall` gated off until the first `generate_embeddings` call).
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
    // Route through the orchestrator so the discovery call participates in
    // concurrency + rate limiting (CLAUDE.md: "All LLM calls MUST go through
    // LlmOrchestrator"). Never call `client::list_models` directly here.
    orchestrator
        .list_models(&request.provider, &request.endpoint_url, request.api_key.as_deref())
        .await
}

#[tauri::command]
pub fn has_llm_config(db_state: State<'_, DbState>) -> Result<bool, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    llm_config_repo::has_config(&conn)
}
