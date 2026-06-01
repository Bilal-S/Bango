use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::db::summary_repo;
use crate::error::AppError;
use crate::prisma::data;
use crate::summary::engine::{self, SummaryInput};
use crate::summary::prompt::{ArticleSummary, ScreeningData};

#[tauri::command]
pub async fn generate_summary(
    db_state: State<'_, DbState>,
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

    let result = engine::generate_summary(summary_input).await?;

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
