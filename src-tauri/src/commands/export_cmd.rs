use serde::Deserialize;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::migration;
use crate::error::AppError;
use crate::export::project;
use crate::export::ris_writer::{articles_to_ris, RisExportArticle};

#[tauri::command]
pub fn export_ris(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let articles = article_repo::get_articles_by_status(&conn, "included")?;

    let export_articles: Vec<RisExportArticle> = articles
        .iter()
        .map(|a| RisExportArticle {
            reference_type: a.reference_type.clone(),
            title: a.title.clone(),
            abstract_text: a.abstract_text.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
            journal: a.journal.clone(),
            volume: a.volume.clone(),
            issue: a.issue.clone(),
            start_page: a.start_page.clone(),
            end_page: a.end_page.clone(),
            keywords: a.keywords.clone(),
            tags: vec![], // TODO: load from article_tags join
            url: a.url.clone(),
            language: a.language.clone(),
            publisher: a.publisher.clone(),
            issn: a.issn.clone(),
            ai_reasoning: a.ai_reasoning.clone(),
            user_notes: a.user_notes.clone(),
            ai_decision: a.ai_decision.as_ref().map(|d| d.as_str().to_string()),
            labels: vec![], // TODO: load from article_labels join
        })
        .collect();

    Ok(articles_to_ris(&export_articles))
}

#[tauri::command]
pub fn export_ris_to_file(db_state: State<'_, DbState>, path: String) -> Result<(), AppError> {
    let content = export_ris(db_state)?;
    std::fs::write(path, content).map_err(AppError::Io)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProjectRequest {}

#[tauri::command]
pub fn export_project_backup(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    project::export_project(&conn)
}

#[tauri::command]
pub fn export_project_to_file(db_state: State<'_, DbState>, path: String) -> Result<(), AppError> {
    let content = export_project_backup(db_state)?;
    std::fs::write(path, content).map_err(AppError::Io)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProjectRequest {
    pub json_content: String,
}

#[tauri::command]
pub fn import_project_backup(
    db_state: State<'_, DbState>,
    request: ImportProjectRequest,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    project::import_project(&conn, &request.json_content)
}

#[tauri::command]
pub fn reset_project(db_state: State<'_, DbState>) -> Result<(), AppError> {
    let mut conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let tx = conn.transaction()?;

    // Disable foreign keys temporarily to allow deleting articles with self-references (duplicate_of)
    tx.execute("PRAGMA foreign_keys = OFF", [])?;

    // Drop all data tables (child tables first for clarity, though FK is OFF)
    tx.execute_batch(
        "DELETE FROM article_labels;
         DELETE FROM article_tags;
         DELETE FROM audit_entries;
         DELETE FROM articles;
         DELETE FROM criteria;
         DELETE FROM research_aims;
         DELETE FROM tags;
         DELETE FROM labels;
         DELETE FROM llm_config;",
    )?;

    // Reset the auto-increment counter
    tx.execute_batch("DELETE FROM sqlite_sequence;")?;

    // Re-enable foreign keys
    tx.execute("PRAGMA foreign_keys = ON", [])?;

    tx.commit()?;

    // Re-run migrations to ensure clean schema (idempotent)
    // We lock again because the transaction was committed and tx is dropped
    drop(conn);
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    migration::run_migrations(&conn)?;

    Ok(())
}
