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

/// Helper: convert articles into RIS string, resolving criteria labels.
fn articles_to_ris_export(
    articles: &[crate::models::article::Article],
    criteria_map: &HashMap<String, String>,
) -> String {
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
            notes: a.notes.clone(),
            ai_reasoning: a.ai_reasoning.clone(),
            user_notes: a.user_notes.clone(),
            ai_decision: a.ai_decision.as_ref().map(|d| d.as_str().to_string()),
            labels: a.labels.clone(),
            matched_inclusion_criteria: resolve_criteria(&a.matched_inclusion_criteria),
            matched_exclusion_criteria: resolve_criteria(&a.matched_exclusion_criteria),
        })
        .collect();

    articles_to_ris(&export_articles)
}

/// Build the criteria lookup map (id → text).
fn build_criteria_map(conn: &rusqlite::Connection) -> Result<HashMap<String, String>, AppError> {
    Ok(criteria_repo::get_all_criteria(conn)?.into_iter().map(|c| (c.id, c.text)).collect())
}

#[tauri::command]
pub fn export_ris(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let articles = article_repo::get_articles_by_status(&conn, "included")?;
    let criteria_map = build_criteria_map(&conn)?;
    Ok(articles_to_ris_export(&articles, &criteria_map))
}

#[tauri::command]
pub fn export_ris_to_file(db_state: State<'_, DbState>, path: String) -> Result<(), AppError> {
    let content = export_ris(db_state)?;
    std::fs::write(path, content).map_err(AppError::Io)
}

#[tauri::command]
pub fn export_ris_for_tab_to_file(
    db_state: State<'_, DbState>,
    path: String,
    status: String,
    screening_errors_only: bool,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let articles = article_repo::get_articles_for_export(&conn, &status, screening_errors_only)?;
    let criteria_map = build_criteria_map(&conn)?;
    let content = articles_to_ris_export(&articles, &criteria_map);
    std::fs::write(&path, content).map_err(AppError::Io)
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
    eprintln!(
        "[import_project_backup] Command received, content length: {}",
        request.json_content.len()
    );
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    eprintln!("[import_project_backup] DB lock acquired, calling import_project...");
    let result = project::import_project(&conn, &request.json_content);
    match &result {
        Ok(()) => eprintln!("[import_project_backup] Import succeeded"),
        Err(e) => eprintln!("[import_project_backup] Import failed: {:?}", e),
    }
    result
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

        // Drop all tables so migrations can re-create them from scratch.
        // Using DROP (not DELETE) avoids ALTER TABLE conflicts when
        // migrations are re-run against an existing schema.
        tx.execute_batch(
            "DROP TABLE IF EXISTS article_reference_links;
             DROP TABLE IF EXISTS reference_papers;
             DROP TABLE IF EXISTS article_labels;
             DROP TABLE IF EXISTS article_tags;
             DROP TABLE IF EXISTS audit_entries;
             DROP TABLE IF EXISTS articles;
             DROP TABLE IF EXISTS criteria;
             DROP TABLE IF EXISTS research_aims;
             DROP TABLE IF EXISTS tags;
             DROP TABLE IF EXISTS labels;
             DROP TABLE IF EXISTS llm_config;
             DROP TABLE IF EXISTS summary;
             DROP TABLE IF EXISTS app_settings;
             DROP INDEX IF EXISTS idx_articles_status;
             DROP INDEX IF EXISTS idx_articles_duplicate_of;
             DROP INDEX IF EXISTS idx_articles_screened_at;
             DROP INDEX IF EXISTS idx_articles_data_length;
             DROP INDEX IF EXISTS idx_articles_sequence_id;
             DROP INDEX IF EXISTS idx_audit_entries_article_id;
             DROP INDEX IF EXISTS idx_criteria_type;
             DROP INDEX IF EXISTS idx_articles_changed_at;
             DROP INDEX IF EXISTS uq_ref_papers_doi;
             DROP INDEX IF EXISTS uq_ref_papers_title_authors_year;
             DROP INDEX IF EXISTS idx_ref_papers_match;
             DROP INDEX IF EXISTS idx_ref_papers_matched_article;
             DROP INDEX IF EXISTS idx_ref_links_parent;
             DROP INDEX IF EXISTS idx_ref_links_paper;
             DROP INDEX IF EXISTS idx_ref_links_parent_type;",
        )?;

        tx.commit()?;
    }

    // Re-enable foreign keys (outside transaction)
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // Reset migration version to 0 so all migrations re-run from scratch.
    conn.pragma_update(None, "user_version", 0)?;

    // Re-run migrations to rebuild clean schema
    migration::run_migrations(&conn)?;

    Ok(())
}
