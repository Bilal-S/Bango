//! Translation-status helpers.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.
//!
//! The DB-backed translation progress record lives on the `articles` row (there
//! is no `translation_jobs` table). These helpers are the single write-path for
//! `translation_status` / `is_translated` / `translation_error` / `translated_at`.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// Snapshot of the translation status fields for one article.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationStatusInfo {
    pub article_id: String,
    pub is_translated: bool,
    pub translation_status: String,
    pub translation_error: Option<String>,
    pub translated_at: Option<String>,
}

/// Write `translation_status` (and clear `translation_error` when leaving a
/// failed state). Used by the enqueue path (`queued`) and the worker (`running`
/// / `succeeded`).
pub fn update_translation_status(
    conn: &Connection,
    article_id: &str,
    status: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET translation_status = ?1, \
             translation_error = CASE WHEN ?1 = 'failed' THEN translation_error ELSE NULL END, \
             changed_at = datetime('now') \
         WHERE id = ?2",
        params![status, article_id],
    )?;
    Ok(())
}

/// Mark a translation job as failed with the given error message.
pub fn update_translation_status_failed(
    conn: &Connection,
    article_id: &str,
    error_msg: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET translation_status = 'failed', translation_error = ?1, \
             changed_at = datetime('now') WHERE id = ?2",
        params![error_msg, article_id],
    )?;
    Ok(())
}

/// Reset an article for re-translation: `translation_status = 'none'`,
/// `is_translated = 0`, clear error. Used by `retry_translation_job`.
pub fn reset_translation_status(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET translation_status = 'none', is_translated = 0, \
             translation_error = NULL, translated_at = NULL, \
             changed_at = datetime('now') WHERE id = ?1",
        params![article_id],
    )?;
    Ok(())
}

/// Read the translation status snapshot for one article.
pub fn get_translation_status(
    conn: &Connection,
    article_id: &str,
) -> Result<TranslationStatusInfo, AppError> {
    conn.query_row(
        "SELECT id, is_translated, translation_status, translation_error, translated_at \
         FROM articles WHERE id = ?1",
        [article_id],
        |row| {
            Ok(TranslationStatusInfo {
                article_id: row.get(0)?,
                is_translated: row.get::<_, i32>(1)? != 0,
                translation_status: row.get(2)?,
                translation_error: row.get(3)?,
                translated_at: row.get(4)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Article {} not found", article_id))
        }
        other => AppError::Database(other),
    })
}

/// Articles stranded in `queued` or `running` (crash recovery on startup).
///
/// Returns `(id, has_full_text)` per stranded article so the caller can choose
/// the correct `TranslationJobKind` (`FullText` when `has_full_text`, else
/// `MetadataOnly`). Re-enqueuing a stranded full-text job as `MetadataOnly`
/// would leave the full text + chunks in the original language while marking
/// the article `is_translated = 1`.
pub fn get_stranded_translation_articles(
    conn: &Connection,
) -> Result<Vec<(String, bool)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, has_full_text FROM articles \
         WHERE translation_status IN ('queued', 'running') AND is_translated = 0",
    )?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// IDs of unscreened working articles (`status = 'working' AND screened_at IS NULL`).
/// Used by the pre-screening translation step (Tier 3 decision b) to find the
/// candidate set for `MetadataOnly` translation before the screening LLM runs.
pub fn get_unscreened_working_ids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt =
        conn.prepare("SELECT id FROM articles WHERE status = 'working' AND screened_at IS NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Articles from `candidate_ids` that are eligible for translation enqueue:
/// `is_translated = 0` AND `translation_status IN ('none','failed')`.
///
/// Returns `(id, language, has_full_text)` per article so the caller can apply
/// the non-English `should_skip_translation` gate (Tier 1b) and choose the job
/// kind (`FullText` vs `MetadataOnly`). One filtered query replaces the
/// previous per-article `get_article_by_id` + `get_translation_status`
/// round-trip that ran inside the import lock.
///
/// `language` is included so the caller applies the skip gate without a second
/// DB read. Articles with NULL/blank language are returned; the caller filters
/// them via `should_skip_translation`.
pub fn get_translatable_import_ids(
    conn: &Connection,
    candidate_ids: &[String],
) -> Result<Vec<(String, Option<String>, bool)>, AppError> {
    if candidate_ids.is_empty() {
        return Ok(Vec::new());
    }
    // SQLite parameter limit is 999; chunk to stay well under it.
    const CHUNK: usize = 500;
    let mut out = Vec::new();
    for chunk in candidate_ids.chunks(CHUNK) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, language, has_full_text FROM articles \
             WHERE id IN ({placeholders}) \
             AND is_translated = 0 \
             AND translation_status IN ('none', 'failed')"
        );
        let params: Vec<&dyn rusqlite::types::ToSql> =
            chunk.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Bulk-write `translation_status = 'queued'` for the given ids in one
/// filtered UPDATE. Only rows still in `('none','failed')` AND
/// `is_translated = 0` are touched, so a concurrent enqueue cannot re-queue a
/// job that already started. Returns the number of rows actually updated.
pub fn mark_translation_queued_batch(
    conn: &Connection,
    article_ids: &[String],
) -> Result<usize, AppError> {
    if article_ids.is_empty() {
        return Ok(0);
    }
    const CHUNK: usize = 500;
    let mut count = 0usize;
    for chunk in article_ids.chunks(CHUNK) {
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE articles SET translation_status = 'queued', \
             translation_error = NULL, changed_at = datetime('now') \
             WHERE id IN ({placeholders}) \
             AND is_translated = 0 \
             AND translation_status IN ('none', 'failed')"
        );
        let params: Vec<&dyn rusqlite::types::ToSql> =
            chunk.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        count += conn.execute(&sql, params.as_slice())?;
    }
    Ok(count)
}

/// Mark a set of stranded articles as `failed` with a cap-exceeded audit note.
/// Used by `reenqueue_stranded_on_startup` when the stranded count exceeds the
/// startup cap so capped rows are not silently lost; they surface in the Audit
/// Timeline as a retryable failure instead of staying perpetually `queued`.
pub fn mark_stranded_capped_failed(
    conn: &Connection,
    article_ids: &[String],
    note: &str,
) -> Result<usize, AppError> {
    if article_ids.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    for id in article_ids {
        let rows = conn.execute(
            "UPDATE articles SET translation_status = 'failed', translation_error = ?1, \
             changed_at = datetime('now') \
             WHERE id = ?2 AND translation_status IN ('queued', 'running')",
            params![note, id],
        )?;
        if rows > 0 {
            let _ = crate::db::audit_repo::create_entry(
                conn,
                id,
                "translation_error",
                None,
                None,
                Some(note),
                "system",
            );
            count += rows;
        }
    }
    Ok(count)
}
