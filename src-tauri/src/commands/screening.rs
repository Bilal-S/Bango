use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;
use crate::screening::engine::{ScreeningEngine, ScreeningProgress};
use crate::screening::llm_client::HttpLlmClient;
use crate::screening::token_estimation;

/// Global screening engine state managed by Tauri.
pub struct ScreeningState {
    pub engine: RwLock<Option<Arc<ScreeningEngine>>>,
}

/// Readiness check returned on mount - lightweight, single DB lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningReadiness {
    /// Total articles in the working list (status = 'working').
    pub total_working: usize,
    /// Unscreened articles in the working list (screened_at IS NULL).
    pub total_unscreened: usize,
    /// Whether at least one research aim exists.
    pub has_aims: bool,
    /// Whether at least one inclusion criterion exists.
    pub has_inclusion: bool,
    /// Whether at least one exclusion criterion exists.
    pub has_exclusion: bool,
    /// Whether LLM config is set up.
    pub has_llm_config: bool,
    /// Token warning if articles exceed context window thresholds.
    pub token_warning: Option<String>,
    /// Progress if a screening run is already in progress.
    pub progress: Option<ScreeningProgress>,
}

#[tauri::command]
pub async fn get_screening_readiness(
    db_state: State<'_, DbState>,
    screening_state: State<'_, ScreeningState>,
) -> Result<ScreeningReadiness, AppError> {
    // ── Check screening engine (no DB lock held) ──
    let progress = {
        let guard = screening_state.engine.read().await;
        match guard.as_ref() {
            Some(engine) => Some(engine.get_progress().await),
            None => None,
        }
    };

    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // 1. Check cheap prerequisites first
    let has_aims = criteria_repo::has_any_aims(&conn)?;
    let has_inclusion = criteria_repo::has_inclusion_criteria(&conn)?;
    let has_exclusion = criteria_repo::has_exclusion_criteria(&conn)?;
    let has_llm_config = llm_config_repo::has_config(&conn)?;

    let (total_working, total_unscreened, token_warning) = if !has_aims
        || !has_inclusion
        || !has_exclusion
        || !has_llm_config
    {
        (0, 0, None)
    } else {
        // 2. All prerequisites met - get counts + token warning in same lock scope
        let total_working = article_repo::count_working(&conn)?;
        let total_unscreened = article_repo::count_unscreened_working(&conn)?;

        let token_warning = if total_unscreened > 0 {
            let config = llm_config_repo::get_config_no_decrypt(&conn)?
                .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

            let max_chars = article_repo::max_article_char_len(&conn)?;
            let worst_case_tokens = max_chars / 4; // chars/4 heuristic

            let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
            let template_tokens = token_estimation::estimate_tokens(&template_text);

            let threshold = (config.context_window_tokens as f64 * 0.8) as usize;
            let total = worst_case_tokens + template_tokens;

            if total > threshold {
                Some(format!(
                    "Estimated worst-case per-article tokens ({}) exceed 80% of context window ({}). \
                         Articles with large abstracts may produce truncated responses.",
                    total, threshold,
                ))
            } else {
                None
            }
        } else {
            None
        };

        (total_working, total_unscreened, token_warning)
    };

    Ok(ScreeningReadiness {
        total_working,
        total_unscreened,
        has_aims,
        has_inclusion,
        has_exclusion,
        has_llm_config,
        token_warning,
        progress,
    })
}

#[tauri::command]
pub async fn start_screening(
    app_handle: AppHandle,
    db_state: State<'_, DbState>,
    screening_state: State<'_, ScreeningState>,
    batch_size: Option<u32>,
) -> Result<ScreeningProgress, AppError> {
    // ── Concurrent-start guard ──
    {
        let guard = screening_state.engine.read().await;
        if let Some(ref existing) = *guard {
            let progress = existing.get_progress().await;
            if progress.is_running {
                return Ok(progress);
            }
        }
    }

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

    // Validate prerequisites
    if aims.is_empty() {
        return Err(AppError::Validation(
            "No research aims defined. Add at least one research aim in the Criteria Editor."
                .to_string(),
        ));
    }
    if !criteria
        .iter()
        .any(|c| matches!(c.criterion_type, crate::models::criterion::CriterionType::Inclusion))
    {
        return Err(AppError::Validation(
            "No inclusion criteria defined. Add at least one inclusion criterion in the Criteria Editor.".to_string(),
        ));
    }
    if !criteria
        .iter()
        .any(|c| matches!(c.criterion_type, crate::models::criterion::CriterionType::Exclusion))
    {
        return Err(AppError::Validation(
            "No exclusion criteria defined. Add at least one exclusion criterion in the Criteria Editor.".to_string(),
        ));
    }

    let unscreened = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        article_repo::count_unscreened_working(&conn)?
    };
    if unscreened == 0 {
        return Err(AppError::Validation(
            "No unscreened articles in the working list. Import and deduplicate articles first."
                .to_string(),
        ));
    }

    // Create and store engine in state (clamp batch_size to 1–15)
    let effective_batch_size = batch_size.unwrap_or(1).clamp(1, 15) as usize;
    let engine = Arc::new(ScreeningEngine::with_batch_size(effective_batch_size));
    let initial_progress = {
        let mut state_engine = screening_state.engine.write().await;
        *state_engine = Some(engine.clone());
        engine.get_progress().await
    };

    // ── Non-blocking: spawn engine in background task ──
    let delay_ms = config.request_delay_ms as u64;
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>().inner().clone();
    let llm: HttpLlmClient = HttpLlmClient { config, orchestrator };
    tokio::spawn(async move {
        let db = app_handle.state::<DbState>();
        let screening = app_handle.state::<ScreeningState>();
        let _ = engine
            .run_sync(&db.conn, &llm, delay_ms, criteria, aims, Some(app_handle.clone()))
            .await;

        // Screening decisions change article statuses (included/rejected), which
        // alters the bibliometric corpus. Mark it stale if any articles were
        // actually processed (completed > 0).
        let completed = engine.get_progress().await.completed;
        if completed > 0 {
            if let Ok(conn) = db.conn.lock() {
                crate::db::app_settings_repo::mark_biblio_needs_refresh(&conn);
            }
        }

        // Clear engine from state after completion
        let mut state_engine = screening.engine.write().await;
        *state_engine = None;
    });

    Ok(initial_progress)
}

#[tauri::command]
pub async fn get_screening_progress(
    screening_state: State<'_, ScreeningState>,
) -> Result<ScreeningProgress, AppError> {
    let guard = screening_state.engine.read().await;
    match guard.as_ref() {
        Some(engine) => Ok(engine.get_progress().await),
        None => Ok(ScreeningProgress::default()),
    }
}

#[tauri::command]
pub async fn pause_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.read().await;
    if let Some(ref engine) = *guard {
        engine.pause().await;
        Ok(())
    } else {
        Err(AppError::Validation("No screening in progress".to_string()))
    }
}

#[tauri::command]
pub async fn resume_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.read().await;
    if let Some(ref engine) = *guard {
        engine.resume().await;
        Ok(())
    } else {
        Err(AppError::Validation("No screening in progress".to_string()))
    }
}

#[tauri::command]
pub async fn stop_screening(screening_state: State<'_, ScreeningState>) -> Result<(), AppError> {
    let guard = screening_state.engine.read().await;
    if let Some(ref engine) = *guard {
        engine.cancel().await;
        Ok(())
    } else {
        Err(AppError::Validation("No screening in progress".to_string()))
    }
}

#[tauri::command]
pub fn reset_screening_errors(db_state: State<'_, DbState>) -> Result<usize, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let count = article_repo::reset_screening_errors(&conn)?;
    Ok(count)
}

#[tauri::command]
pub fn reset_working_list(db_state: State<'_, DbState>) -> Result<usize, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let count = article_repo::reset_working_list(&conn)?;
    Ok(count)
}

#[tauri::command]
pub fn estimate_screening_tokens(db_state: State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let config = llm_config_repo::get_config_no_decrypt(&conn)?
        .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

    let max_len = article_repo::max_article_char_len(&conn)?;

    if max_len == 0 {
        return Ok(None);
    }

    let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
    let template_tokens = token_estimation::estimate_tokens(&template_text);

    let worst_case_article_tokens = max_len / 4;

    let result = token_estimation::check_context_window(
        template_tokens,
        &[worst_case_article_tokens],
        config.context_window_tokens as usize,
    );

    Ok(result)
}
