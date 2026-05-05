use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::screening::engine::{ScreeningEngine, ScreeningProgress};
use crate::screening::token_estimation;

/// Global screening engine state managed by Tauri.
pub struct ScreeningState {
    pub engine: tokio::sync::Mutex<Option<ScreeningEngine>>,
}

#[tauri::command]
pub async fn start_screening(
    db_state: State<'_, DbState>,
    screening_state: State<'_, ScreeningState>,
) -> Result<ScreeningProgress, AppError> {
    let config = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?
    };

    let criteria = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        criteria_repo::get_all_criteria(&conn)?
    };

    let aims = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        criteria_repo::get_all_aims(&conn)?
    };

    let engine = ScreeningEngine::new();

    // Store engine in state for progress/control queries
    {
        let mut state_engine = screening_state.engine.lock().await;
        *state_engine = Some(engine);
    }

    // Run screening using the engine stored in state
    {
        let guard = screening_state.engine.lock().await;
        if let Some(ref engine) = *guard {
            engine.run_sync(&db_state.conn, config, criteria, aims).await?;
        }
    }

    // Return final progress
    let guard = screening_state.engine.lock().await;
    match guard.as_ref() {
        Some(engine) => Ok(engine.get_progress().await),
        None => Ok(ScreeningProgress::default()),
    }
}

#[tauri::command]
pub async fn get_screening_progress(
    screening_state: State<'_, ScreeningState>,
) -> Result<ScreeningProgress, AppError> {
    let guard = screening_state.engine.lock().await;
    match guard.as_ref() {
        Some(engine) => Ok(engine.get_progress().await),
        None => Ok(ScreeningProgress::default()),
    }
}

#[tauri::command]
pub async fn pause_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.lock().await;
    match guard.as_ref() {
        Some(engine) => {
            engine.pause().await;
            Ok(())
        }
        None => Err(AppError::Validation("No screening in progress".to_string())),
    }
}

#[tauri::command]
pub async fn resume_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.lock().await;
    match guard.as_ref() {
        Some(engine) => {
            engine.resume().await;
            Ok(())
        }
        None => Err(AppError::Validation("No screening in progress".to_string())),
    }
}

#[tauri::command]
pub async fn stop_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.lock().await;
    match guard.as_ref() {
        Some(engine) => {
            engine.cancel().await;
            Ok(())
        }
        None => Err(AppError::Validation("No screening in progress".to_string())),
    }
}

#[tauri::command]
pub fn estimate_screening_tokens(db_state: State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let config = llm_config_repo::get_config(&conn)?
        .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

    let articles = article_repo::get_articles_by_status(&conn, "working")?;
    let working: Vec<_> = articles.iter().filter(|a| a.screened_at.is_none()).collect();

    if working.is_empty() {
        return Ok(None);
    }

    // Estimate template tokens
    let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
    let template_tokens = token_estimation::estimate_tokens(&template_text);

    // Estimate per-article tokens
    let article_tokens: Vec<usize> = working
        .iter()
        .map(|a| {
            let text = format!("{}{}{}", a.title, a.authors.join(""), a.abstract_text);
            token_estimation::estimate_tokens(&text)
        })
        .collect();

    Ok(token_estimation::check_context_window(
        template_tokens,
        &article_tokens,
        config.context_window_tokens as usize,
    ))
}
