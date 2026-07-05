use tauri::State;

use crate::db::app_settings_repo;
use crate::db::article_repo::{self, ArticleQuery};
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::Article;
use crate::models::audit::{AuditEntry, ImportActivity};
use crate::models::biblio::JournalInfo;

#[tauri::command]
pub fn query_articles(
    db_state: State<'_, DbState>,
    query: ArticleQuery,
) -> Result<Vec<Article>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::query_articles(&conn, &query)
}

#[tauri::command]
pub fn get_article_counts(
    db_state: State<'_, DbState>,
) -> Result<crate::models::article::ArticleCounts, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::get_article_counts(&conn)
}

#[tauri::command]
pub fn get_article(db_state: State<'_, DbState>, id: String) -> Result<Article, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::get_article_by_id(&conn, &id)
}

#[tauri::command]
pub fn update_article_status(
    db_state: State<'_, DbState>,
    id: String,
    new_status: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::update_article_status(&conn, &id, &new_status)?;
    // Status changes (e.g. to/from 'included') alter the bibliometric corpus.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn get_audit_trail(
    db_state: State<'_, DbState>,
    article_id: String,
) -> Result<Vec<AuditEntry>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_audit_trail(&conn, &article_id)
}

#[tauri::command]
pub fn get_recent_audit_entries(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<AuditEntry>, AppError> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_recent_audit_entries(&conn, limit, offset)
}

#[tauri::command]
pub fn update_article_notes(
    db_state: State<'_, DbState>,
    id: String,
    notes: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    // Tag changes feed the keyword co-occurrence network.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn update_article_labels(
    db_state: State<'_, DbState>,
    id: String,
    label_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    // Labels are part of article metadata used by bibliometrics.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::override_ai_decision(
        &conn,
        &id,
        &new_decision,
        &new_status,
        reasoning.as_deref(),
    )?;
    // Overrides may change an article's status (included/rejected).
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn update_article_criteria(
    db_state: State<'_, DbState>,
    id: String,
    inclusion_ids: Vec<String>,
    exclusion_ids: Vec<String>,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    offset: Option<usize>,
) -> Result<Vec<ImportActivity>, AppError> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_import_activities(&conn, limit, offset)
}

#[tauri::command]
pub fn get_generic_audit_entries(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
) -> Result<Vec<AuditEntry>, AppError> {
    let limit = limit.unwrap_or(10);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_generic_audit_entries(&conn, limit)
}

#[tauri::command]
pub fn bulk_update_article_status(
    db_state: State<'_, DbState>,
    ids: Vec<String>,
    new_status: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::bulk_update_article_status(&conn, &ids, &new_status)?;
    // Bulk status changes alter the bibliometric corpus.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn bulk_add_tag_to_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    tag_name: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::bulk_add_tag_to_articles(&conn, &article_ids, &tag_name)?;
    // Bulk tag changes feed the keyword co-occurrence network.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn bulk_add_label_to_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    label_name: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::bulk_add_label_to_articles(&conn, &article_ids, &label_name)?;
    // Bulk label changes affect article metadata.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn clear_generic_audit(db_state: State<'_, DbState>) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::clear_generic_entries(&conn)
}

/// Re-attempt journal matching for all articles and reference papers that have
/// `journal_index_id IS NULL` and `reference_type = 'JOUR'`.
/// Returns `{ "articles": <n>, "references": <m> }`.
#[tauri::command]
pub fn rematch_journals(db_state: State<'_, DbState>) -> Result<serde_json::Value, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    let articles_matched = article_repo::rematch_all_journals(&conn)?;
    let refs_matched = crate::db::reference_repo::rematch_all_journals(&conn)?;

    Ok(serde_json::json!({
        "articles": articles_matched,
        "references": refs_matched,
    }))
}

/// Fetch full metadata + time-series for one journal_index row.
/// Powers the timeline Journal Info Card. Returns `None` for an unknown id.
#[tauri::command]
pub fn biblio_get_journal_info(
    db_state: State<'_, DbState>,
    journal_index_id: String,
) -> Result<Option<JournalInfo>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    crate::db::journal_repo::get_journal_info(&conn, &journal_index_id)
}
