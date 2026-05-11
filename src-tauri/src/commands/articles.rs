use tauri::State;

use crate::db::article_repo::{self, ArticleQuery};
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::Article;
use crate::models::audit::{AuditEntry, ImportActivity};

#[tauri::command]
pub fn query_articles(
    db_state: State<'_, DbState>,
    query: ArticleQuery,
) -> Result<Vec<Article>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::query_articles(&conn, &query)
}

#[tauri::command]
pub fn get_article_counts(
    db_state: State<'_, DbState>,
) -> Result<crate::models::article::ArticleCounts, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_article_counts(&conn)
}

#[tauri::command]
pub fn get_article(db_state: State<'_, DbState>, id: String) -> Result<Article, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_article_by_id(&conn, &id)
}

#[tauri::command]
pub fn update_article_status(
    db_state: State<'_, DbState>,
    id: String,
    new_status: String,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_article_status(&conn, &id, &new_status)
}

#[tauri::command]
pub fn get_audit_trail(
    db_state: State<'_, DbState>,
    article_id: String,
) -> Result<Vec<AuditEntry>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    audit_repo::get_audit_trail(&conn, &article_id)
}

#[tauri::command]
pub fn get_recent_audit_entries(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
) -> Result<Vec<AuditEntry>, AppError> {
    let limit = limit.unwrap_or(10);
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    audit_repo::get_recent_audit_entries(&conn, limit)
}

#[tauri::command]
pub fn update_article_notes(
    db_state: State<'_, DbState>,
    id: String,
    notes: String,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_user_notes(&conn, &id, &notes)?;
    audit_repo::create_entry(
        &conn,
        &id,
        "status_change",
        None,
        None,
        Some(&format!("Notes updated: {}", if notes.is_empty() { "(cleared)" } else { &notes })),
        "user",
    )?;
    Ok(())
}

#[tauri::command]
pub fn update_article_tags(
    db_state: State<'_, DbState>,
    id: String,
    tag_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_article_tags(&conn, &id, &tag_ids)?;
    audit_repo::create_entry(
        &conn,
        &id,
        "tag_add",
        None,
        None,
        Some(&format!("Tags updated: {} tag(s)", tag_ids.len())),
        "user",
    )?;
    Ok(())
}

#[tauri::command]
pub fn update_article_labels(
    db_state: State<'_, DbState>,
    id: String,
    label_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_article_labels(&conn, &id, &label_ids)?;
    audit_repo::create_entry(
        &conn,
        &id,
        "label_add",
        None,
        None,
        Some(&format!("Labels updated: {} label(s)", label_ids.len())),
        "user",
    )?;
    Ok(())
}

#[tauri::command]
pub fn override_ai_decision(
    db_state: State<'_, DbState>,
    id: String,
    new_decision: String,
    new_status: String,
    reasoning: Option<String>,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::override_ai_decision(&conn, &id, &new_decision, &new_status, reasoning.as_deref())
}

#[tauri::command]
pub fn update_article_criteria(
    db_state: State<'_, DbState>,
    id: String,
    inclusion_ids: Vec<String>,
    exclusion_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_article_criteria(&conn, &id, &inclusion_ids, &exclusion_ids)?;
    audit_repo::create_entry(
        &conn,
        &id,
        "criteria_match",
        None,
        None,
        Some(&format!(
            "Criteria updated: {} inclusion, {} exclusion",
            inclusion_ids.len(),
            exclusion_ids.len()
        )),
        "user",
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_import_activities(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
) -> Result<Vec<ImportActivity>, AppError> {
    let limit = limit.unwrap_or(10);
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    audit_repo::get_import_activities(&conn, limit)
}

#[tauri::command]
pub fn clear_generic_audit(db_state: State<'_, DbState>) -> Result<usize, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    audit_repo::clear_generic_entries(&conn)
}
