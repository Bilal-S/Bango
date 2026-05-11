use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::llm_config::{LlmConfig, LlmProvider};

#[tauri::command]
pub fn get_llm_config(db_state: State<'_, DbState>) -> Result<Option<LlmConfig>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    llm_config_repo::get_config(&conn)
}

#[tauri::command]
pub fn save_llm_config(db_state: State<'_, DbState>, config: LlmConfig) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    llm_config_repo::save_config(&conn, &config)
}

#[derive(Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_llm_connection(
    db_state: State<'_, DbState>,
) -> Result<TestConnectionResult, AppError> {
    let config = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("No LLM config found".to_string()))?
    };

    // First attempt: use config as-is (temperature included unless already skipped)
    match client::send_chat_completion(&config, "You are a test.", "Say hello.").await {
        Ok(_) => Ok(TestConnectionResult {
            success: true,
            message: "Connection successful!".to_string(),
        }),
        Err(e) => {
            let err_msg = format!("{e}");

            // Check if the error is temperature-related
            if err_msg.contains("temperature") && !config.skip_temperature {
                // Retry without temperature
                let mut retry_config = config.clone();
                retry_config.skip_temperature = true;

                match client::send_chat_completion(&retry_config, "You are a test.", "Say hello.")
                    .await
                {
                    Ok(_) => {
                        // Save the updated config so future calls skip temperature
                        let conn = db_state.conn.lock().map_err(|e| {
                            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
                        })?;
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
pub async fn list_llm_models(request: ListModelsRequest) -> Result<Vec<String>, AppError> {
    client::list_models(&request.provider, &request.endpoint_url, request.api_key.as_deref()).await
}
