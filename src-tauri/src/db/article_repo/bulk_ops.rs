//! Bulk status + bulk tag/label add/remove + screening/working-list resets.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;

/// Reset screening errors: clear `screened_at` and `screening_error` for all working articles
/// that were screened but didn't get a status change, so they can be re-screened.
pub fn reset_screening_errors(conn: &Connection) -> Result<usize, AppError> {
    let rows = conn.execute(
        "UPDATE articles SET screened_at = NULL, screening_error = 0, changed_at = datetime('now') \
         WHERE status = 'working' AND screened_at IS NOT NULL",
        [],
    )?;
    Ok(rows)
}

/// Reset the working list: semantically identical to `reset_screening_errors` (both clear
/// `screened_at` and `screening_error` for previously-screened working articles). Kept as a
/// thin delegate so the two Tauri command endpoints (`reset_screening_errors` /
/// `reset_working_list` in `commands::screening`) can keep their distinct frontend contracts
/// while sharing one implementation.
pub fn reset_working_list(conn: &Connection) -> Result<usize, AppError> {
    reset_screening_errors(conn)
}

/// Bulk update status for multiple articles in a single transaction.
///
/// When moving articles back to 'working', reset the screening flags
/// (`screened_at`, `screening_error`) so the articles become eligible for
/// re-screening on the next run. This mirrors the single-article
/// `update_article_status` behavior - see the state-machine note there and
/// in `docs/bango-v4-spec.md` §4.2.
pub fn bulk_update_article_status(
    conn: &Connection,
    ids: &[String],
    new_status: &str,
) -> Result<usize, AppError> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    for id in ids {
        let rows = if new_status == "working" {
            conn.execute(
                "UPDATE articles SET status = ?1, manual_override = 1, \
                 screened_at = NULL, screening_error = 0, changed_at = datetime('now') \
                 WHERE id = ?2",
                params![new_status, id],
            )?
        } else {
            conn.execute(
                "UPDATE articles SET status = ?1, manual_override = 1, \
                 changed_at = datetime('now') \
                 WHERE id = ?2",
                params![new_status, id],
            )?
        };
        count += rows;
    }
    Ok(count)
}

/// Bulk add a tag to multiple articles (by tag name).
/// Creates the tag if it doesn't exist.
///
/// Returns the IDs of articles that actually received the tag. Articles that
/// already had the tag are skipped (`INSERT OR IGNORE`), so the returned vec
/// may be shorter than `article_ids`. Each article touched by the insert has
/// its `changed_at` bumped so sort-by-modified and the bibliometric/wiki
/// staleness flags reflect the change. The caller (command layer) uses the
/// returned IDs to write per-article audit entries and to report the accurate
/// affected count in the UI toast.
pub fn bulk_add_tag_to_articles(
    conn: &Connection,
    article_ids: &[String],
    tag_name: &str,
) -> Result<Vec<String>, AppError> {
    if article_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Ensure tag exists
    let existing_id: Option<String> =
        conn.query_row("SELECT id FROM tags WHERE name = ?1", [tag_name], |row| row.get(0)).ok();
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
    let mut affected = Vec::new();
    for article_id in article_ids {
        let rows = conn.execute(
            "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
            params![article_id, tag_id],
        )?;
        if rows > 0 {
            // Bump changed_at only when the tag was newly linked (matches the
            // single-article `update_article_tags` behavior).
            conn.execute(
                "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
                [article_id],
            )?;
            affected.push(article_id.clone());
        }
    }
    Ok(affected)
}

/// Bulk add a label to multiple articles (by label name).
/// Creates the label if it doesn't exist.
///
/// Returns the IDs of articles that actually received the label. See
/// [`bulk_add_tag_to_articles`] for the partial-application semantics.
pub fn bulk_add_label_to_articles(
    conn: &Connection,
    article_ids: &[String],
    label_name: &str,
) -> Result<Vec<String>, AppError> {
    if article_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Ensure label exists
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
    let mut affected = Vec::new();
    for article_id in article_ids {
        let rows = conn.execute(
            "INSERT OR IGNORE INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
            params![article_id, label_id],
        )?;
        if rows > 0 {
            conn.execute(
                "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
                [article_id],
            )?;
            affected.push(article_id.clone());
        }
    }
    Ok(affected)
}

/// Bulk remove a tag from multiple articles (by tag name).
///
/// Returns the IDs of articles from which the tag was actually removed.
/// If the named tag does not exist in the `tags` table at all, returns an
/// empty vec (nothing to remove). Each article that lost the link has its
/// `changed_at` bumped. This is the bulk counterpart to the remove-half of
/// the single-article `update_article_tags` flow and pairs with the
/// `bulk_add_tag_to_articles` add action so the "Change Tag of N articles"
/// dialog can both add and remove.
pub fn bulk_remove_tag_from_articles(
    conn: &Connection,
    article_ids: &[String],
    tag_name: &str,
) -> Result<Vec<String>, AppError> {
    if article_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Resolve tag id; if it doesn't exist there is nothing to remove.
    let tag_id: Option<String> =
        conn.query_row("SELECT id FROM tags WHERE name = ?1", [tag_name], |row| row.get(0)).ok();
    let Some(tag_id) = tag_id else {
        return Ok(Vec::new());
    };
    let mut affected = Vec::new();
    for article_id in article_ids {
        let rows = conn.execute(
            "DELETE FROM article_tags WHERE article_id = ?1 AND tag_id = ?2",
            params![article_id, tag_id],
        )?;
        if rows > 0 {
            conn.execute(
                "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
                [article_id],
            )?;
            affected.push(article_id.clone());
        }
    }
    Ok(affected)
}

/// Bulk remove a label from multiple articles (by label name).
///
/// Returns the IDs of articles from which the label was actually removed.
/// See [`bulk_remove_tag_from_articles`] for the partial-application and
/// missing-label semantics.
pub fn bulk_remove_label_from_articles(
    conn: &Connection,
    article_ids: &[String],
    label_name: &str,
) -> Result<Vec<String>, AppError> {
    if article_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Resolve label id; if it doesn't exist there is nothing to remove.
    let label_id: Option<String> = conn
        .query_row("SELECT id FROM labels WHERE name = ?1", [label_name], |row| row.get(0))
        .ok();
    let Some(label_id) = label_id else {
        return Ok(Vec::new());
    };
    let mut affected = Vec::new();
    for article_id in article_ids {
        let rows = conn.execute(
            "DELETE FROM article_labels WHERE article_id = ?1 AND label_id = ?2",
            params![article_id, label_id],
        )?;
        if rows > 0 {
            conn.execute(
                "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
                [article_id],
            )?;
            affected.push(article_id.clone());
        }
    }
    Ok(affected)
}
