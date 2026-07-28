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

#[tauri::command]
pub async fn save_llm_config(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
    config: LlmConfig,
) -> Result<(), AppError> {
    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        llm_config_repo::save_config(&conn, &config)?;
    } // conn dropped here

    // Update the in-memory orchestrator with new concurrency/delay settings
    orchestrator
        .update_settings(config.max_concurrent_requests as usize, config.request_delay_ms as u64)
        .await;

    Ok(())
}

#[derive(Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_llm_connection(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
) -> Result<TestConnectionResult, AppError> {
    let config = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("No LLM config found".to_string()))?
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
                let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                llm_config_repo::save_config(&conn, &retry_config)?;
                Ok(TestConnectionResult {
                    success: true,
                    message: "Connection successful! (temperature not supported by this model - auto-adjusted)".to_string(),
                })
            } else {
                Ok(TestConnectionResult {
                    success: true,
                    message: "Connection successful!".to_string(),
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
                        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                        llm_config_repo::save_config(&conn, &retry_config)?;

                        Ok(TestConnectionResult {
                            success: true,
                            message: "Connection successful! (temperature not supported by this model - auto-adjusted)".to_string(),
                        })
                    }
                    Err(retry_err) => Ok(TestConnectionResult {
                        success: false,
                        message: format!("Connection failed: {retry_err}"),
                    }),
                }
            } else {
                Ok(TestConnectionResult {
                    success: false,
                    message: format!("Connection failed: {e}"),
                })
            }
        }
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
