pub mod app_settings;
pub mod articles;
pub mod biblio_cmd;
pub mod chat;
pub mod citation_finder;
pub mod criteria;
pub mod dedup;
pub mod embedding;
pub mod export_cmd;
pub mod full_text;
pub mod import;
pub mod labels;
pub mod llm_config;
pub mod openalex;
pub mod prisma;
pub mod references;
pub mod scraping;
pub mod screening;
pub mod search_strategy;
pub mod startup;
pub mod summary;
pub mod tags;
pub mod translation;
pub mod trends;
pub mod wiki_cmd;

use crate::db::connection::DbState;
use crate::error::AppError;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub status: String,
    pub article_count: usize,
}

#[tauri::command]
pub fn health_check(db_state: tauri::State<'_, DbState>) -> Result<HealthCheck, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let count: usize =
        conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0)).unwrap_or(0);
    Ok(HealthCheck { status: "ok".to_string(), article_count: count })
}
