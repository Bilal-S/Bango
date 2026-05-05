use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::summary::engine::{self, SummaryInput, SummaryOutput};
use crate::summary::prompt::ArticleSummary;

#[tauri::command]
pub async fn generate_summary(
    db_state: State<'_, DbState>,
    target_length: Option<usize>,
) -> Result<SummaryOutput, AppError> {
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
            .map(|a| ArticleSummary {
                title: a.title.clone(),
                authors: a.authors.clone(),
                year: a.publication_year,
                abstract_text: a.abstract_text.clone(),
                ai_reasoning: a.ai_reasoning.clone(),
            })
            .collect();

        let length = target_length.unwrap_or(1000);
        SummaryInput::new(config, aim_texts, articles, length)
    }; // conn lock released here

    engine::generate_summary(summary_input).await
}
