use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

use crate::db::app_settings_repo::{self, ScreeningMode};
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::orchestrator::LlmOrchestrator;
use crate::screening::engine::{ScreeningConfig, ScreeningEngine, ScreeningProgress};
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

    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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
            let llm_config = llm_config_repo::get_config_no_decrypt(&conn)?
                .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

            // Tier 3 Gap 5: mode-aware worst-case footprint per §4.3.
            let mode = app_settings_repo::get_screening_mode(&conn)?;
            let chunk_budget = app_settings_repo::get_chunk_budget_per_article(&conn)?;
            let borderline_fraction =
                app_settings_repo::get_two_stage_expected_borderline_fraction(&conn)?;

            let max_chars = article_repo::max_article_char_len(&conn)?;
            let abstract_tokens = max_chars / 4; // chars/4 heuristic
            let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
            let template_tokens = token_estimation::estimate_tokens(&template_text);
            let worst_case = token_estimation::worst_case_per_article_tokens(
                mode,
                abstract_tokens,
                template_tokens,
                chunk_budget,
                borderline_fraction,
            );

            let threshold = (llm_config.context_window_tokens as f64 * 0.8) as usize;
            if worst_case > threshold {
                Some(format!(
                    "Estimated worst-case per-article tokens ({}) exceed 80% of context window ({}). \
                         Articles with large abstracts may produce truncated responses.",
                    worst_case, threshold,
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
    max_articles: Option<u32>,
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
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?
    };

    let criteria = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        criteria_repo::get_all_criteria(&conn)?
    };

    let aims = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        article_repo::count_unscreened_working(&conn)?
    };
    if unscreened == 0 {
        return Err(AppError::Validation(
            "No unscreened articles in the working list. Import and deduplicate articles first."
                .to_string(),
        ));
    }

    // Tier 3: read the screening mode + params from `app_settings` and build
    // the ScreeningConfig the engine consumes. Defaults preserve abstract-only
    // behavior exactly when no mode key is set.
    //
    // NOTE: the chunk-backfill guard (`ensure_chunks_for_full_text_articles`)
    // previously ran inside this synchronous block, holding the DbState mutex
    // for the full PDF-parse + chunk-write pass and freezing the Tauri UI. It
    // now runs inside the spawned background task below, before `run_sync`,
    // so the IPC handler returns immediately and the lock is held only for the
    // millisecond-scale SQLite writes the engine itself performs.
    let screening_config = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let mode = app_settings_repo::get_screening_mode(&conn)?;
        ScreeningConfig {
            mode,
            enhanced_top_k: app_settings_repo::get_enhanced_top_k(&conn)?,
            enhanced_sections: app_settings_repo::get_enhanced_screening_sections(&conn)?,
            two_stage_low: app_settings_repo::get_two_stage_low(&conn)?,
            two_stage_high: app_settings_repo::get_two_stage_high(&conn)?,
            chunk_budget_per_article: app_settings_repo::get_chunk_budget_per_article(&conn)?,
            max_articles: max_articles.map(|n| n.max(1) as usize),
        }
    };

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

        // Tier 3: pre-screening translation step (decision b). When
        // `auto_translate` is enabled, enqueue `MetadataOnly` translation
        // jobs for unscreened working articles with a non-English `language`
        // and wait for all to finish BEFORE screening runs, so the screening
        // LLM reads English text. Emits `screening:progress` events with a
        // translation sub-stage so the existing progress UI shows
        // "Translating N/M articles..." before the normal screening stage.
        run_pre_screening_translation(&app_handle, &db.conn).await;

        // Tier 3: for enhanced / two-stage, backfill chunks for any article
        // with full text but no chunks (pure CPU, no LLM). Runs here in the
        // background task - not in the IPC handler - so the DbState mutex is
        // held only per-article during chunk write, not for the whole pass,
        // and the UI stays responsive. `force=false` so already-chunked
        // articles are skipped (the Settings "Rebuild" button uses `force=true`).
        if screening_config.mode != ScreeningMode::Abstract {
            if let Ok(conn) = db.conn.lock() {
                let _ =
                    crate::commands::full_text::ensure_chunks_for_full_text_articles(&conn, false);
            }
        }

        let _ = engine
            .run_sync(
                &db.conn,
                &llm,
                delay_ms,
                criteria,
                aims,
                screening_config,
                Some(app_handle.clone()),
            )
            .await;

        // Screening decisions change article statuses (included/rejected), which
        // alters the bibliometric corpus. Mark it stale if any articles were
        // actually processed (completed > 0).
        let completed = engine.get_progress().await.completed;
        if completed > 0 {
            if let Ok(conn) = db.conn.lock() {
                crate::db::app_settings_repo::mark_biblio_needs_refresh(&conn);
                crate::db::app_settings_repo::mark_wiki_needs_refresh(&conn);
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let count = article_repo::reset_screening_errors(&conn)?;
    Ok(count)
}

#[tauri::command]
pub fn reset_working_list(db_state: State<'_, DbState>) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let count = article_repo::reset_working_list(&conn)?;
    Ok(count)
}

#[tauri::command]
pub fn estimate_screening_tokens(db_state: State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    let config = llm_config_repo::get_config_no_decrypt(&conn)?
        .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

    let max_len = article_repo::max_article_char_len(&conn)?;
    if max_len == 0 {
        return Ok(None);
    }

    // Tier 3 Gap 5: mode-aware worst-case footprint per §4.3. Previously this
    // always computed `abstract_tokens + template_tokens`, ignoring the
    // Enhanced chunk budget and the Two-stage borderline overhead. Now both
    // command entry points (`get_screening_readiness`, `estimate_screening_tokens`)
    // route through the same pure helper so their estimates stay in sync.
    let mode = app_settings_repo::get_screening_mode(&conn)?;
    let chunk_budget = app_settings_repo::get_chunk_budget_per_article(&conn)?;
    let borderline_fraction = app_settings_repo::get_two_stage_expected_borderline_fraction(&conn)?;

    let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
    let template_tokens = token_estimation::estimate_tokens(&template_text);
    let abstract_tokens = max_len / 4;
    let worst_case = token_estimation::worst_case_per_article_tokens(
        mode,
        abstract_tokens,
        template_tokens,
        chunk_budget,
        borderline_fraction,
    );

    let threshold = (config.context_window_tokens as f64 * 0.8) as usize;
    if worst_case > threshold {
        Ok(Some(format!(
            "Estimated worst-case per-article tokens ({}) exceed 80% of context window ({}). \
             Articles with large abstracts may produce truncated responses.",
            worst_case, threshold,
        )))
    } else {
        Ok(None)
    }
}

// ── Tier 3 screening-mode commands ──────────────────────────────────────────

/// Read the active screening mode (`abstract` | `enhanced` | `two_stage`).
/// Powers the Settings -> Screening Preferences radio-card selector.
#[tauri::command]
pub fn get_screening_mode(db_state: State<'_, DbState>) -> Result<ScreeningMode, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::get_screening_mode(&conn)
}

/// Persist the active screening mode.
#[tauri::command]
pub fn set_screening_mode(
    db_state: State<'_, DbState>,
    mode: ScreeningMode,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    app_settings_repo::set_screening_mode(&conn, mode)
}

/// Count articles with full text attached. Drives the Settings gate that
/// disables Enhanced/Two-stage mode until at least one full-text article exists.
#[tauri::command]
pub fn get_full_text_article_count(db_state: State<'_, DbState>) -> Result<i64, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    chunk_repo::count_articles_with_full_text(&conn)
}

// ── Tier 3: pre-screening translation step (decision b) ─────────────────────

/// `screening:progress` payload for the translation sub-stage. Emitted before
/// the screening engine runs so the frontend progress UI shows
/// "Translating N/M articles..." while the worker finishes metadata-only
/// translations of unscreened working non-English articles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSubStage {
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

/// Pre-screening translation step (decision b).
///
/// When `auto_translate` is enabled, enqueues `MetadataOnly` translation jobs
/// for unscreened working articles with a non-English `language` and waits for
/// all to finish BEFORE the screening engine runs, so the screening LLM reads
/// English text. Emits `screening:translation-progress` events with a
/// per-article counter so the existing progress UI can surface
/// "Translating 3/12 articles...".
///
/// Skipped entirely when `auto_translate` is `false` (the new opt-in default),
/// so screening starts immediately and reads whatever text is present
/// (original or previously-translated).
async fn run_pre_screening_translation(
    app: &tauri::AppHandle,
    db: &std::sync::Mutex<rusqlite::Connection>,
) {
    // Read the toggle + candidate set in one short lock scope.
    let to_translate: Vec<String> = {
        let Ok(conn) = crate::db::connection::lock_conn(db) else {
            return;
        };
        let auto = app_settings_repo::get_auto_translate(&conn).unwrap_or(false);
        if !auto {
            return;
        }
        // Unscreened working non-English articles not already translated or
        // queued. Reuses the import batch helper to filter by status.
        let unscreened_ids: Vec<String> = match article_repo::get_unscreened_working_ids(&conn) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[screening] pre-translate: failed to read unscreened ids: {e}");
                return;
            }
        };
        let candidates = match article_repo::get_translatable_import_ids(&conn, &unscreened_ids) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[screening] pre-translate: get_translatable_import_ids failed: {e}");
                return;
            }
        };
        candidates
            .into_iter()
            .filter(|(_, language, _)| {
                !crate::translation::should_skip_translation(language.as_deref())
            })
            .map(|(id, _, _)| id)
            .collect()
    };

    if to_translate.is_empty() {
        return;
    }

    // Enqueue via the batch helper (re-locks briefly). `MetadataOnly` because
    // screening consumes only the title + abstract, not full-text chunks.
    crate::commands::translation::try_enqueue_translations_for_import(app, db, &to_translate);

    // Wait for each translation to finish, emitting progress per article.
    let total = to_translate.len();
    for (idx, article_id) in to_translate.iter().enumerate() {
        let _ = app.emit(
            "screening:translation-progress",
            TranslationSubStage {
                completed: idx,
                total,
                message: format!("Translating {}/{} articles...", idx + 1, total),
            },
        );
        if let Err(e) = crate::translation::wait_for_article_translation(app, db, article_id).await
        {
            eprintln!("[screening] pre-translate: {e} - proceeding with current text");
        }
    }

    // Final sub-stage event so the UI clears the "Translating..." line.
    let _ = app.emit(
        "screening:translation-progress",
        TranslationSubStage {
            completed: total,
            total,
            message: "Translations complete; starting screening.".to_string(),
        },
    );
}
