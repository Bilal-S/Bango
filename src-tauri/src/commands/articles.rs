use tauri::State;

use crate::db::app_settings_repo;
use crate::db::article_repo::{self, ArticleMetaField, ArticleMetaValue, ArticleQuery};
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::journal_repo::JournalIndexMatch;
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

/// Clear the AI decision, reasoning text, and confidence for a single article.
/// Powers the trashcan icon in the AI Decision card's expanded header. The
/// `ai_decision` + `ai_reasoning` + `ai_confidence` are all nulled so the
/// entire card unmounts; the user's own Include/Exclude choice lives on the
/// separate `status` field, which stays intact. `screened_at` is preserved so
/// the screening history survives. Writes an `ai_screen_clear` audit entry so
/// the action appears in the Audit Timeline.
#[tauri::command]
pub fn clear_ai_reasoning(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::clear_ai_reasoning(&conn, &id)?;
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

/// Update a single metadata field (Title, Authors, Affiliation, Journal, Year,
/// Lang, DOI, Keywords) on an article. Powers the double-click inline editing
/// in the Article Detail header (Title) and the "Metadata" card (the rest).
/// The `field` enum validates the column name (no string interpolation);
/// `value` is a serde-untagged scalar-or-array payload (arrays for
/// Authors/Keywords, scalar string for the rest).
///
/// Audit detail string:
/// - For `Title`: captures the old → new transition
///   (`"Title changed: \"<old>\" → \"<new>\""`, each side truncated to ~80
///   chars) so the Audit Timeline shows what the title was changed from/to.
/// - For all other fields: the generic `"Metadata edited: <Label>"` string
///   (unchanged from the pre-Title behavior).
#[tauri::command]
pub fn update_article_metadata(
    db_state: State<'_, DbState>,
    id: String,
    field: ArticleMetaField,
    value: ArticleMetaValue,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    // For Title edits, capture the old title BEFORE the update so the audit
    // detail can record the from → to transition. Title is the only metadata
    // field where recording the actual values is useful (the others are
    // adequately described by "Metadata edited: <Label>").
    let audit_detail: String = if field == ArticleMetaField::Title {
        // Extract the new title from the payload (mirrors the repo's trim +
        // empty-reject so the detail string matches what will be persisted).
        let new_title = match &value {
            ArticleMetaValue::Scalar(Some(s)) => s.trim(),
            _ => "",
        };
        if new_title.is_empty() {
            // The repo layer will reject this with AppError::Validation; no
            // point building a detail string for a doomed update. Use the
            // generic label so the audit shape stays consistent if the call
            // ever relaxes the empty gate.
            format!("Metadata edited: {}", field.label())
        } else {
            let old_title: String = conn
                .query_row("SELECT title FROM articles WHERE id = ?1", [&id], |row| row.get(0))
                .unwrap_or_default();
            format!(
                "Title changed: \"{}\" → \"{}\"",
                truncate_for_audit(&old_title),
                truncate_for_audit(new_title)
            )
        }
    } else {
        format!("Metadata edited: {}", field.label())
    };

    article_repo::update_article_metadata_field(&conn, &id, field, value)?;
    audit_repo::create_or_update_entry(
        &conn,
        &id,
        "metadata_edit",
        None,
        None,
        Some(&audit_detail),
        "user",
    )?;
    // Metadata changes (title, authors, journal, year, language, keywords) feed
    // both the bibliometric pipelines and the LLM Wiki knowledge base.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    Ok(())
}

/// Truncate a string for the audit detail field to keep the row readable.
/// Caps at ~80 chars with an ellipsis to avoid flooding the Audit Timeline
/// with long titles. Pure helper used only by `update_article_metadata`.
#[must_use]
fn truncate_for_audit(s: &str) -> String {
    const MAX: usize = 80;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push('…');
    out
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
    // Delegate to the shared helper so the audit trail shape stays byte-identical
    // across the bulk add/remove commands and the merge commands. Only the detail
    // string is formatted here (bulk-specific prefix); the loop lives in the shared
    // `audit_repo::write_tag_label_audit`.
    let detail = format!("Bulk {action}: \"{name}\"");
    audit_repo::write_tag_label_audit(conn, affected_ids, action, &detail)
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

/// Fetch the original (pre-translation) title for an article, if any.
/// Returns `None` when the article has not been translated or no original
/// content is archived. Used by the frontend detail-header to display the
/// original title in brackets alongside the translated English title.
#[tauri::command]
pub fn get_original_title(
    db_state: State<'_, DbState>,
    article_id: String,
) -> Result<Option<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let content = crate::db::article_original_repo::get_original_content(&conn, &article_id)?;
    Ok(content.and_then(|c| c.original_title))
}

/// Interactive journal search for the article-metadata autocomplete. Returns
/// candidate `journal_index` rows ranked by ISSN, exact name, then LIKE
/// substring (shortest title first). Unlike the automatic `match_journal`,
/// substring matching is safe here because the user reviews the candidates.
#[tauri::command]
pub fn search_journal_index(
    db_state: State<'_, DbState>,
    query: String,
) -> Result<Vec<JournalIndexMatch>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    crate::db::journal_repo::search_journal_index(&conn, &query, None)
}

/// Link an article to a `journal_index` row chosen by the user from the
/// autocomplete. Sets `articles.journal_index_id`, backfills `issn`/`eissn`
/// from the matched row (COALESCE - never overwrites existing values), writes
/// a coalesced `metadata_edit` audit row, and marks the biblio + wiki
/// staleness flags so dependent pipelines re-derive.
#[tauri::command]
pub fn link_article_to_journal_index(
    db_state: State<'_, DbState>,
    article_id: String,
    journal_index_id: String,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    // Fetch the matched journal row for title (audit) + ISSN backfill.
    let (title, issn, eissn): (String, Option<String>, Option<String>) = conn.query_row(
        "SELECT journal_title, issn, eissn FROM journal_index WHERE id = ?1",
        [&journal_index_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    // 1. Set the link + backfill ISSNs (COALESCE preserves existing values).
    conn.execute(
        "UPDATE articles
         SET journal_index_id = ?1,
             issn = COALESCE(NULLIF(issn, ''), ?2),
             eissn = COALESCE(NULLIF(eissn, ''), ?3)
         WHERE id = ?4",
        rusqlite::params![journal_index_id, issn, eissn, article_id],
    )?;

    // 2. Coalesced audit row (5-min window groups rapid journal edits).
    let details = format!("Journal linked: {title}");
    let _ = audit_repo::create_or_update_entry(
        &conn,
        &article_id,
        "metadata_edit",
        None,
        None,
        Some(&details),
        "user",
    );

    // 3. Staleness flags: journal_index_id feeds bibliometrics + the LLM Wiki.
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);

    Ok(())
}
