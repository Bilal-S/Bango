use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::db::summary_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::prisma::data;
use crate::screening::engine as screening_engine;
use crate::summary::engine::{self, SummaryInput};
use crate::summary::prompt::{ArticleSummary, ScreeningData};

#[tauri::command]
pub async fn generate_summary(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    citation_style: Option<String>,
) -> Result<String, AppError> {
    let style = citation_style.unwrap_or_else(|| "APA".to_string());

    // Extract all DB data synchronously while holding the lock
    let summary_input = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;

        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aim_list = criteria_repo::get_all_aims(&conn)?;
        let aim_texts: Vec<String> = aim_list.iter().map(|a| a.text.clone()).collect();
        let included = article_repo::get_articles_by_status(&conn, "included")?;

        let articles: Vec<ArticleSummary> = included
            .iter()
            .map(|a| {
                // Combine RIS-imported keywords and user/AI-added tags into one deduplicated CSV list
                let mut combined: Vec<String> = a.keywords.clone();
                for tag in &a.tags {
                    if !combined.iter().any(|k| k.eq_ignore_ascii_case(tag)) {
                        combined.push(tag.clone());
                    }
                }
                ArticleSummary {
                    title: a.title.clone(),
                    authors: a.authors.clone(),
                    year: a.publication_year,
                    abstract_text: a.abstract_text.clone(),
                    keywords: combined,
                }
            })
            .collect();

        // PRISMA / screening statistics
        let prisma = data::compute_prisma_data(&conn)?;

        // AI-screened: articles that have an ai_decision set
        let ai_screened: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE ai_decision IS NOT NULL", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        // Manual review: articles where manual_override = 1
        let manual_reviewed: usize = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE manual_override = 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let screening_data = ScreeningData {
            records_identified: prisma.records_identified,
            duplicates_removed: prisma.duplicates_removed,
            records_screened: prisma.records_screened,
            records_excluded: prisma.records_excluded,
            records_excluded_with_reasons: prisma.records_excluded_with_reasons,
            records_assessed: prisma.records_assessed,
            records_in_progress: prisma.records_in_progress,
            studies_included: prisma.studies_included,
            ai_screened,
            manual_reviewed,
            exclusion_reasons: prisma
                .exclusion_reasons
                .iter()
                .map(|r| (r.criterion_text.clone(), r.count))
                .collect(),
        };

        SummaryInput::new(config, aim_texts, articles, screening_data, style.clone())
    }; // conn lock released here

    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let result = engine::generate_summary(&orchestrator, summary_input).await?;

    // Save to DB after successful generation
    let generated_at = chrono::Utc::now().to_rfc3339();
    {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        summary_repo::save_summary(&conn, &result, &style, &generated_at)?;
    }

    Ok(result)
}

#[tauri::command]
pub fn get_saved_summary(
    db_state: State<'_, DbState>,
) -> Result<Option<summary_repo::SavedSummary>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    summary_repo::get_summary(&conn)
}

/// AI Article Summary prompt from the spec.
const ARTICLE_SUMMARY_SYSTEM_PROMPT: &str =
    include_str!("../../../.worktrees/ai-article-summary.md");

/// Generate an AI summary for a single article based on its full text.
/// Calls the LLM, parses the JSON response, stores it in the database,
/// and emits a Tauri event with the result.
/// On success, records an `ai_summary` entry in the article's audit trail.
/// On error, logs the failure to the general diagnostic audit and emits an error event.
#[tauri::command]
pub async fn generate_article_ai_summary(
    db_state: State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<String, AppError> {
    // 1. Fetch article full text and LLM config while holding the DB lock
    let (title, full_text, config) = {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        let (t, ft) = article_repo::get_full_text_for_summary(&conn, &article_id)?;
        let cfg = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        (t, ft, cfg)
    }; // conn lock released

    // 2. Build user prompt with article title and full text
    // Truncate full text to stay within reasonable token limits
    let max_chars = ((config.context_window_tokens as usize).saturating_sub(2000)) * 4;
    let truncated = if full_text.len() > max_chars { &full_text[..max_chars] } else { &full_text };
    let user_prompt = format!("## Article Title\n{}\n\n## Full Text\n{}", title, truncated);

    // 3. Call LLM via orchestrator - catch errors to log them to audit trail
    let orchestrator = app_handle.state::<Arc<LlmOrchestrator>>();
    let llm_result = orchestrator
        .send(&config, ARTICLE_SUMMARY_SYSTEM_PROMPT, &user_prompt, LlmRequestType::ArticleSummary)
        .await;

    let (response_text, _tokens) = match llm_result {
        Ok(v) => v,
        Err(e) => {
            // Log error to general diagnostic audit
            let err_msg = e.to_string();
            if let Ok(conn) = db_state.conn.lock() {
                let _ = crate::db::audit_repo::log_error(
                    &conn,
                    &format!("AI summary failed for article {article_id} ({title}): {err_msg}"),
                );
            }
            // Emit error event so frontend can react
            let _ = app_handle.emit(
                "article-ai-summary-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(e);
        }
    };

    // 4. Validate the response is valid JSON - strip markdown code fences if present
    let cleaned = screening_engine::extract_json(&response_text);
    let parsed: serde_json::Value = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            let err_msg = format!("Invalid JSON response from LLM: {e}");
            if let Ok(conn) = db_state.conn.lock() {
                let _ = crate::db::audit_repo::log_error(
                    &conn,
                    &format!("AI summary failed for article {article_id} ({title}): {err_msg}"),
                );
            }
            let _ = app_handle.emit(
                "article-ai-summary-error",
                serde_json::json!({ "articleId": article_id, "error": err_msg }),
            );
            return Err(AppError::Import(err_msg));
        }
    };

    // Store the raw JSON string
    let summary_json = parsed.to_string();

    // 5. Store in database
    {
        let conn = db_state.conn.lock().map_err(|e| {
            AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string()))
        })?;
        article_repo::set_ai_summary(&conn, &article_id, &summary_json)?;
        crate::db::audit_repo::create_entry(
            &conn,
            &article_id,
            "ai_summary",
            None,
            None,
            Some("AI summary generated from full text"),
            "ai",
        )?;
    }

    // 6. Emit success event
    let _ = app_handle.emit(
        "article-ai-summary-complete",
        serde_json::json!({ "articleId": article_id, "title": title }),
    );

    Ok(summary_json)
}
