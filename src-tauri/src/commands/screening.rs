use serde::{Deserialize, Serialize};
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

/// Readiness check returned on mount — lightweight, single DB lock.
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
    // ── Single DB lock scope: check prerequisites, then counts if needed ──
    struct ReadinessData {
        total_working: usize,
        total_unscreened: usize,
        has_aims: bool,
        has_inclusion: bool,
        has_exclusion: bool,
        has_llm_config: bool,
        token_warning: Option<String>,
    }

    let data = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;

        // 1. Check cheap prerequisites first
        let has_aims = criteria_repo::has_any_aims(&conn)?;
        let has_inclusion = criteria_repo::has_inclusion_criteria(&conn)?;
        let has_exclusion = criteria_repo::has_exclusion_criteria(&conn)?;
        let has_llm_config = llm_config_repo::has_config(&conn)?;

        // Early exit: if prerequisites are missing, skip article queries entirely
        if !has_aims || !has_inclusion || !has_exclusion || !has_llm_config {
            ReadinessData {
                total_working: 0,
                total_unscreened: 0,
                has_aims,
                has_inclusion,
                has_exclusion,
                has_llm_config,
                token_warning: None,
            }
        } else {
            // 2. All prerequisites met — get counts + token warning in same lock scope
            let total_working = article_repo::count_working(&conn)?;
            let total_unscreened = article_repo::count_unscreened_working(&conn)?;

            let token_warning = if total_unscreened > 0 {
                // We still need the context window tokens from the config for estimation
                let config = llm_config_repo::get_config(&conn)?
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

            ReadinessData {
                total_working,
                total_unscreened,
                has_aims,
                has_inclusion,
                has_exclusion,
                has_llm_config,
                token_warning,
            }
        }
        // conn dropped here — single lock acquisition
    };

    // ── Check screening engine (no DB lock held) ──
    let progress = {
        let guard = screening_state.engine.lock().await;
        match guard.as_ref() {
            Some(engine) => Some(engine.get_progress().await),
            None => None,
        }
    };

    Ok(ScreeningReadiness {
        total_working: data.total_working,
        total_unscreened: data.total_unscreened,
        has_aims: data.has_aims,
        has_inclusion: data.has_inclusion,
        has_exclusion: data.has_exclusion,
        has_llm_config: data.has_llm_config,
        token_warning: data.token_warning,
        progress,
    })
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
