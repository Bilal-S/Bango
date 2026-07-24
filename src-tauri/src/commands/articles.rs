use tauri::State;

use crate::db::app_settings_repo;
use crate::db::article_repo::{self, ArticleMetaField, ArticleMetaValue, ArticleQuery};
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::Article;
use crate::models::audit::{ActivityFeedEntry, AuditEntry, ImportActivity};
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

/// Permanently delete an article and all of its related data (full text,
/// extracted chunks, AI summary, audit history, user notes, tag/label
/// associations, translation archive, dedup links, and reference links to
/// papers that no other article uses). The frontend MUST show a confirmation
/// dialog before invoking this; the backend performs no second confirmation.
///
/// See [`article_repo::delete_article`] for the full cascade contract.
#[tauri::command]
pub fn delete_article(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::delete_article(&conn, &id)
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
    before_timestamp: Option<String>,
) -> Result<Vec<AuditEntry>, AppError> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_recent_audit_entries(&conn, limit, offset, before_timestamp.as_deref())
}

#[tauri::command]
pub fn update_article_notes(
    db_state: State<'_, DbState>,
    id: String,
    notes: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::update_user_notes(&conn, &id, &notes)?;
    audit_repo::create_or_update_entry(
        &conn,
        &id,
        "note_add",
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
    audit_repo::create_or_update_entry(
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
    audit_repo::create_or_update_entry(
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
    audit_repo::create_or_update_entry(
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

/// Update a single metadata field (Authors, Affiliation, Journal, Year, Lang,
/// DOI, Keywords) on an article. Powers the double-click inline editing in the
/// Article Detail "Metadata" card. The `field` enum validates the column name
/// (no string interpolation); `value` is a serde-untagged scalar-or-array
/// payload (arrays for Authors/Keywords, scalar string for the rest).
#[tauri::command]
pub fn update_article_metadata(
    db_state: State<'_, DbState>,
    id: String,
    field: ArticleMetaField,
    value: ArticleMetaValue,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::update_article_metadata_field(&conn, &id, field, value)?;
    audit_repo::create_or_update_entry(
        &conn,
        &id,
        "metadata_edit",
        None,
        None,
        Some(&format!("Metadata edited: {}", field.label())),
        "user",
    )?;
    // Metadata changes (authors, journal, year, language, keywords) feed both
    // the bibliometric pipelines and the LLM Wiki knowledge base.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

#[tauri::command]
pub fn get_import_activities(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
    offset: Option<usize>,
    before_timestamp: Option<String>,
) -> Result<Vec<ImportActivity>, AppError> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_import_activities(&conn, limit, offset, before_timestamp.as_deref())
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
pub fn get_activity_feed(
    db_state: State<'_, DbState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<ActivityFeedEntry>, AppError> {
    let limit = limit.unwrap_or(10);
    let offset = offset.unwrap_or(0);
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::get_activity_feed(&conn, limit, offset)
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

/// Shared helper: write one coalesced audit entry per affected article for a
/// bulk tag/label add or remove. Uses `create_or_update_entry` so rapid repeats
/// of the same action on the same article collapse into a single timeline row
/// within the 5-minute coalesce window.
fn write_bulk_tag_label_audit(
    conn: &rusqlite::Connection,
    affected_ids: &[String],
    action: &str,
    name: &str,
) -> Result<(), AppError> {
    let detail = format!("Bulk {action}: \"{name}\"");
    for id in affected_ids {
        audit_repo::create_or_update_entry(conn, id, action, None, None, Some(&detail), "user")?;
    }
    Ok(())
}

/// Bulk add a tag to multiple articles. Returns the number of articles that
/// actually received the tag (articles that already had it are skipped). One
/// coalesced `tag_add` audit entry is written per affected article so the
/// Audit Timeline reflects the change on each article's history.
#[tauri::command]
pub fn bulk_add_tag_to_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    tag_name: String,
) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let affected = article_repo::bulk_add_tag_to_articles(&conn, &article_ids, &tag_name)?;
    write_bulk_tag_label_audit(&conn, &affected, "tag_add", &tag_name)?;
    // Bulk tag changes feed the keyword co-occurrence network.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(affected.len())
}

/// Bulk add a label to multiple articles. Returns the number of articles that
/// actually received the label. One coalesced `label_add` audit entry is
/// written per affected article.
#[tauri::command]
pub fn bulk_add_label_to_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    label_name: String,
) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let affected = article_repo::bulk_add_label_to_articles(&conn, &article_ids, &label_name)?;
    write_bulk_tag_label_audit(&conn, &affected, "label_add", &label_name)?;
    // Bulk label changes affect article metadata.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(affected.len())
}

/// Bulk remove a tag from multiple articles. Returns the number of articles
/// from which the tag was actually removed (0 means the tag was not present on
/// any selected article or the named tag does not exist at all). One coalesced
/// `tag_remove` audit entry is written per affected article. Powers the
/// "Remove Tag" button in the "Change Tag of N articles" dialog.
#[tauri::command]
pub fn bulk_remove_tag_from_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    tag_name: String,
) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let affected = article_repo::bulk_remove_tag_from_articles(&conn, &article_ids, &tag_name)?;
    write_bulk_tag_label_audit(&conn, &affected, "tag_remove", &tag_name)?;
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(affected.len())
}

/// Bulk remove a label from multiple articles. Returns the number of articles
/// from which the label was actually removed. See
/// [`bulk_remove_tag_from_articles`] for the semantics and audit contract.
#[tauri::command]
pub fn bulk_remove_label_from_articles(
    db_state: State<'_, DbState>,
    article_ids: Vec<String>,
    label_name: String,
) -> Result<usize, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let affected = article_repo::bulk_remove_label_from_articles(&conn, &article_ids, &label_name)?;
    write_bulk_tag_label_audit(&conn, &affected, "label_remove", &label_name)?;
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(affected.len())
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
