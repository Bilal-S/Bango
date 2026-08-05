//! Hard-delete cascade (`delete_article`).
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Body moved
//! VERBATIM; no behavioral change.

use rusqlite::Connection;

use crate::error::AppError;

/// Delete an article and all dependent data in a single transaction.
///
/// `ON DELETE CASCADE` auto-removes: junction tables, audit entries,
/// article_reference_links, chunks, translation archive, biblio joins.
///
/// Explicitly cleaned BEFORE the DELETE (FKs lack ON DELETE):
/// 1. `articles.duplicate_of` — any duplicate pointing here is un-merged (set NULL).
/// 2. `reference_papers.matched_article_id` — cleared, status reset to unmatched
///    AFTER the orphan sweep so matched papers with zero links survive.
///
/// Post-delete: orphaned unmatched reference papers (zero total links) are deleted;
/// shared/promoted papers preserved. On-disk full-text file removed (non-fatal).
/// Biblio + wiki staleness flags set.
///
/// # Errors
/// Returns [`AppError::Database`], [`AppError::NotFound`], or [`AppError::Import`]
/// (disk file removal failure).
pub fn delete_article(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    use std::path::PathBuf;

    // Verify the article exists so a non-existent id yields a clean NotFound
    // instead of a silent no-op DELETE.
    let (full_text_file_name, article_title): (Option<String>, String) = conn
        .query_row(
            "SELECT full_text_file_name, title FROM articles WHERE id = ?1",
            [article_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Article {} not found", article_id))
            }
            other => AppError::Database(other),
        })?;

    // Resolve fulltext dir while outside the transaction (reads app_settings).
    // File deletion happens after commit so a failed removal never rolls back DB work.
    let storage_dir: Option<PathBuf> = if full_text_file_name.is_some() {
        crate::db::app_settings_repo::get_fulltext_dir(conn).ok().map(PathBuf::from)
    } else {
        None
    };

    let tx = conn.unchecked_transaction()?;

    // 1. Null out duplicate_of pointers from any article marked as a
    //    duplicate of this one. The FK has no ON DELETE clause.
    tx.execute(
        "UPDATE articles SET duplicate_of = NULL, changed_at = datetime('now') \
         WHERE duplicate_of = ?1",
        [article_id],
    )?;

    // 2. Clear matched_article_id on reference papers. FK has no ON DELETE.
    // Deferring match_status reset until after the orphan sweep (step 6) so
    // matched papers with zero links survive — they go back to unmatched pool.
    tx.execute(
        "UPDATE reference_papers \
         SET matched_article_id = NULL, updated_at = datetime('now') \
         WHERE matched_article_id = ?1",
        [article_id],
    )?;

    // 3. Decrement counters + delete link rows. CASCADE removes rows but not denorm counts.
    crate::db::reference_repo::delete_references_for_article(&tx, article_id)?;

    // 4. Delete article. CASCADE handles junction, audit, chunks, translation, biblio joins.
    tx.execute("DELETE FROM articles WHERE id = ?1", [article_id])?;

    // 5. Delete orphaned unmatched reference papers (zero total links).
    tx.execute(
        "DELETE FROM reference_papers \
         WHERE citation_count = 0 AND reference_count = 0 \
         AND match_status = 'unmatched'",
        [],
    )?;

    // 6. Reset match_status to 'unmatched' for papers whose matched_article_id was
    // just cleared (step 2). They survived the orphan sweep with 'matched'/'imported'
    // status; now go back to the unmatched pool.
    tx.execute(
        "UPDATE reference_papers \
         SET match_status = 'unmatched', updated_at = datetime('now') \
         WHERE matched_article_id IS NULL AND match_status != 'unmatched'",
        [],
    )?;

    tx.commit()?;

    // 7. Remove on-disk full-text file. Non-fatal: article already deleted.
    if let (Some(dir), Some(name)) = (storage_dir, full_text_file_name.as_ref()) {
        let file_path = dir.join(name);
        if file_path.exists() {
            if let Err(e) = std::fs::remove_file(&file_path) {
                let _ = crate::db::audit_repo::log_error(
                    conn,
                    &format!(
                        "Article \"{}\" ({}) was deleted but its full-text file \"{}\" \
                         could not be removed from disk: {e}",
                        article_title, article_id, name
                    ),
                );
            }
        }
    }

    // 8. Corpus changed: bibliometrics + LLM Wiki need re-derivation.
    crate::db::app_settings_repo::mark_biblio_needs_refresh(conn);
    crate::db::app_settings_repo::mark_wiki_needs_refresh(conn);

    Ok(())
}
