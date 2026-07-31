//! Status / dedup / tags / labels / notes / AI-decision / criteria mutations
//! + the per-field non-empty counter.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;

use super::get_article_by_id;

pub fn mark_as_duplicate(
    conn: &Connection,
    article_id: &str,
    surviving_id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET duplicate_of = ?1, changed_at = datetime('now') WHERE id = ?2",
        params![surviving_id, article_id],
    )?;
    Ok(())
}

pub fn move_to_working(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET status = 'working', changed_at = datetime('now') WHERE id = ?1",
        params![article_id],
    )?;
    Ok(())
}

/// Move multiple articles to 'working' status in a single transaction.
pub fn move_articles_to_working_batch(
    conn: &Connection,
    article_ids: &[String],
) -> Result<usize, AppError> {
    if article_ids.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    for id in article_ids {
        let rows = conn.execute(
            "UPDATE articles SET status = 'working', changed_at = datetime('now') WHERE id = ?1 AND status = 'duplicate'",
            params![id],
        )?;
        count += rows;
    }
    Ok(count)
}

pub fn update_article_status(
    conn: &Connection,
    article_id: &str,
    new_status: &str,
) -> Result<(), AppError> {
    let old_status: String =
        conn.query_row("SELECT status FROM articles WHERE id = ?1", [article_id], |row| {
            row.get(0)
        })?;

    // When moving an article back to 'working', reset the screening flags so the
    // article becomes eligible for re-screening on the next run. Without this the
    // stale `screened_at` timestamp survives the status change and excludes the
    // article from `get_next_unscreened_working_batch`, leaving it stuck in a
    // "previously screened" limbo that surfaces in the Error tab even though
    // `screening_error` is 0. See the state machine in `docs/bango-v4-spec.md`
    // §4.2 - "Working ↔ Included ↔ Rejected" is an explicit allowed transition.
    if new_status == "working" {
        conn.execute(
            "UPDATE articles SET status = ?1, manual_override = 1, \
             screened_at = NULL, screening_error = 0, changed_at = datetime('now') \
             WHERE id = ?2",
            params![new_status, article_id],
        )?;
    } else {
        conn.execute(
            "UPDATE articles SET status = ?1, manual_override = 1, changed_at = datetime('now') \
             WHERE id = ?2",
            params![new_status, article_id],
        )?;
    }

    let audit_detail =
        if new_status == "working" && old_status != "working" && old_status != "duplicate" {
            "Manual status change (screening flags reset for re-screening)"
        } else {
            "Manual status change"
        };

    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "status_change",
        Some(&old_status),
        Some(new_status),
        Some(audit_detail),
        "user",
    )?;

    Ok(())
}

/// Bump `changed_at` on an article. Used by tag/label mutations that touch
/// junction rows but don't go through the full `update_article_*` path (e.g.
/// the merge commands). Centralizes the `datetime('now')` contract so raw SQL
/// stays out of the command layer.
pub fn bump_changed_at(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute("UPDATE articles SET changed_at = datetime('now') WHERE id = ?1", [article_id])?;
    Ok(())
}

pub fn update_article_tags(
    conn: &Connection,
    article_id: &str,
    tag_names: &[String],
) -> Result<(), AppError> {
    conn.execute("UPDATE articles SET changed_at = datetime('now') WHERE id = ?1", [article_id])?;
    conn.execute("DELETE FROM article_tags WHERE article_id = ?1", [article_id])?;

    for tag_name in tag_names {
        let existing_id: Option<String> = conn
            .query_row("SELECT id FROM tags WHERE name = ?1", [tag_name], |row| row.get(0))
            .ok();

        let tag_id = if let Some(id) = existing_id {
            id
        } else {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'user_created')",
                params![id, tag_name],
            )?;
            id
        };

        conn.execute(
            "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
            params![article_id, tag_id],
        )?;
    }

    Ok(())
}

pub fn update_article_labels(
    conn: &Connection,
    article_id: &str,
    label_names: &[String],
) -> Result<(), AppError> {
    conn.execute("UPDATE articles SET changed_at = datetime('now') WHERE id = ?1", [article_id])?;
    conn.execute("DELETE FROM article_labels WHERE article_id = ?1", [article_id])?;

    for label_name in label_names {
        let existing_id: Option<String> = conn
            .query_row("SELECT id FROM labels WHERE name = ?1", [label_name], |row| row.get(0))
            .ok();

        let label_id = if let Some(id) = existing_id {
            id
        } else {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO labels (id, name, source) VALUES (?1, ?2, 'user_created')",
                params![id, label_name],
            )?;
            id
        };

        conn.execute(
            "INSERT INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
            params![article_id, label_id],
        )?;
    }

    Ok(())
}

pub fn update_user_notes(conn: &Connection, article_id: &str, notes: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET user_notes = ?1, changed_at = datetime('now') WHERE id = ?2",
        params![notes, article_id],
    )?;
    Ok(())
}

pub fn update_article_criteria(
    conn: &Connection,
    article_id: &str,
    inclusion_ids: &[String],
    exclusion_ids: &[String],
) -> Result<(), AppError> {
    let inc_json = serde_json::to_string(inclusion_ids)?;
    let exc_json = serde_json::to_string(exclusion_ids)?;
    conn.execute(
        "UPDATE articles SET matched_inclusion_criteria = ?1, matched_exclusion_criteria = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![inc_json, exc_json, article_id],
    )?;
    Ok(())
}

pub fn override_ai_decision(
    conn: &Connection,
    article_id: &str,
    new_decision: &str,
    new_status: &str,
    reasoning: Option<&str>,
) -> Result<(), AppError> {
    let old_decision: Option<String> =
        conn.query_row("SELECT ai_decision FROM articles WHERE id = ?1", [article_id], |row| {
            row.get(0)
        })?;

    conn.execute(
        "UPDATE articles SET ai_decision = ?1, status = ?2, manual_override = 1, changed_at = datetime('now') WHERE id = ?3",
        params![new_decision, new_status, article_id],
    )?;

    if let Some(reason) = reasoning {
        conn.execute(
            "UPDATE articles SET ai_reasoning = ?1, changed_at = datetime('now') WHERE id = ?2",
            params![reason, article_id],
        )?;
    }

    let detail = format!(
        "Override AI decision from {} to {}",
        old_decision.as_deref().unwrap_or("none"),
        new_decision
    );
    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "manual_override",
        None,
        Some(new_status),
        Some(&detail),
        "user",
    )?;

    Ok(())
}

/// Clear the AI decision, reasoning text, and confidence from an article
/// while preserving the status, screening timestamp, and manual-override flag.
///
/// Powers the trashcan icon in the AI Decision card's expanded header. The
/// `ai_decision` + `ai_reasoning` + `ai_confidence` are all nulled so the
/// entire card unmounts (the card renders only when `ai_decision` is set);
/// the user's own Include/Exclude choice lives on the separate `status` field,
/// which stays intact. `screened_at` is preserved so the screening history
/// (audit trail) survives the clear and the article is NOT re-enqueued for
/// screening. Restoring the AI assessment requires re-screening the article
/// (LLM token cost).
///
/// Writes an `ai_screen_clear` audit entry so the action is visible in the
/// Audit Timeline.
pub fn clear_ai_reasoning(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET ai_decision = NULL, ai_reasoning = NULL, ai_confidence = NULL, \
         changed_at = datetime('now') WHERE id = ?1",
        [article_id],
    )?;
    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "ai_screen_clear",
        None,
        None,
        Some("AI reasoning and confidence cleared"),
        "user",
    )?;
    Ok(())
}

pub fn get_article_field_count(conn: &Connection, id: &str) -> Result<usize, AppError> {
    let article = get_article_by_id(conn, id)?;
    let mut count = 0;
    if article.doi.is_some() {
        count += 1;
    }
    if article.journal.is_some() {
        count += 1;
    }
    if article.volume.is_some() {
        count += 1;
    }
    if article.issue.is_some() {
        count += 1;
    }
    if article.start_page.is_some() {
        count += 1;
    }
    if article.end_page.is_some() {
        count += 1;
    }
    if article.publication_year.is_some() {
        count += 1;
    }
    if article.url.is_some() {
        count += 1;
    }
    if article.language.is_some() {
        count += 1;
    }
    if article.publisher.is_some() {
        count += 1;
    }
    if article.issn.is_some() {
        count += 1;
    }
    if article.eissn.is_some() {
        count += 1;
    }
    if article.reference_type.is_some() {
        count += 1;
    }
    if article.date.is_some() {
        count += 1;
    }
    if !article.keywords.is_empty() {
        count += 1;
    }
    if article.notes.is_some() {
        count += 1;
    }
    if !article.abstract_text.is_empty() {
        count += 1;
    }
    Ok(count)
}
