use serde::Deserialize;
use tauri::State;

use std::collections::HashMap;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
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

    // Build criteria lookup: id → text
    let criteria_map: HashMap<String, String> =
        criteria_repo::get_all_criteria(&conn)?.into_iter().map(|c| (c.id, c.text)).collect();

    let resolve_criteria = |ids: &[String]| -> Vec<String> {
        ids.iter().filter_map(|id| criteria_map.get(id).cloned()).collect()
    };

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
            tags: a.tags.clone(),
            url: a.url.clone(),
            language: a.language.clone(),
            publisher: a.publisher.clone(),
            issn: a.issn.clone(),
            ai_reasoning: a.ai_reasoning.clone(),
            user_notes: a.user_notes.clone(),
            ai_decision: a.ai_decision.as_ref().map(|d| d.as_str().to_string()),
            labels: a.labels.clone(),
            matched_inclusion_criteria: resolve_criteria(&a.matched_inclusion_criteria),
            matched_exclusion_criteria: resolve_criteria(&a.matched_exclusion_criteria),
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
pub fn write_text_to_file(path: String, content: String) -> Result<(), AppError> {
    std::fs::write(path, content).map_err(AppError::Io)
}

#[tauri::command]
pub fn reset_project(db_state: State<'_, DbState>) -> Result<(), AppError> {
    let mut conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // PRAGMA foreign_keys cannot be changed inside a transaction.
    // Set it on the connection *before* starting one.
    conn.execute("PRAGMA foreign_keys = OFF", [])?;

    {
        let tx = conn.transaction()?;

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
             DELETE FROM llm_config;
             DELETE FROM summary;",
        )?;

        tx.commit()?;
    }

    // Re-enable foreign keys (outside transaction)
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Re-run migrations to ensure clean schema (idempotent)
    migration::run_migrations(&conn)?;

    Ok(())
}
