//! Hard-delete cascade (`delete_article`).
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Body moved
//! VERBATIM; no behavioral change.

use rusqlite::Connection;

use crate::error::AppError;

/// Delete an article and all of its related data.
///
/// This is a hard delete: the `articles` row is removed along with every
/// dependent record. The operation runs in a single transaction so a failure
/// at any step rolls everything back, leaving the article intact.
///
/// ## Cleanup order and rationale
///
/// The schema enables `PRAGMA foreign_keys=ON` on every connection
/// (`db::connection::create_connection_at`), so `ON DELETE CASCADE` handles
/// the bulk of the dependent rows automatically:
/// - `article_tags`, `article_labels` (junction tables)
/// - `audit_entries` (rebuilt with CASCADE in v003/v004)
/// - `article_reference_links` (parent_article_id)
/// - `article_chunks`, `article_original_content`, `article_original_chunks`
///   (v003 translation archive)
/// - `biblio_article_authors`, `biblio_author_affiliations`,
///   `biblio_article_terms`
///
/// Two foreign keys lack an `ON DELETE` clause and would cause a constraint
/// failure if the parent row were deleted first, so they are cleaned up
/// explicitly BEFORE the `DELETE`:
/// 1. `articles.duplicate_of` (self-referencing FK) - any article marked as a
///    duplicate of the one being deleted has `duplicate_of` set to NULL so it
///    is no longer merged-away. It keeps its `'duplicate'` status; the user
///    can re-merge or re-include it as needed.
/// 2. `reference_papers.matched_article_id` (FK to articles) - reference
///    papers that were promoted from / matched to this article have their
///    `matched_article_id` cleared and `match_status` reset to `'unmatched'`
///    so they reappear in the "Articles of Interest" list instead of
///    dangling.
///
/// Reference papers themselves are shared/deduplicated across articles via
/// `article_reference_links`. We reuse `reference_repo::delete_references_for_article`
/// to decrement counters and delete links (the CASCADE on the junction would
/// remove the link rows but NOT decrement the denormalized counts, so the
/// explicit call is required for count correctness). After the article is
/// gone, orphaned reference papers (citation_count + reference_count == 0 AND
/// match_status = 'unmatched') that only this article ever used are deleted;
/// papers still linked to other articles or already promoted are preserved.
///
/// On-disk full-text files in `{storage_root}/fulltext/` are removed when
/// `full_text_file_name` is set. Finally, the biblio + wiki staleness flags
/// are set because the corpus changed.
///
/// # Errors
///
/// Returns [`AppError::Database`] on SQL failure, [`AppError::NotFound`] if
/// the article does not exist, or [`AppError::Import`] if the full-text file
/// cannot be removed from disk.
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

    // Resolve the fulltext storage directory while still outside the
    // transaction (it reads app_settings). The file deletion happens after
    // the transaction commits so a failed file removal never rolls back the
    // DB work - the article is already gone from the user's perspective and
    // a stray PDF on disk is preferable to a half-rolled-back delete that
    // leaves the article row visible but its references/audit already
    // cascade-deleted.
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

    // 2. Clear matched_article_id on reference papers promoted from / matched
    //    to this article. The FK has no ON DELETE clause so this MUST happen
    //    before the DELETE. We clear ONLY matched_article_id here and defer
    //    the match_status reset to step 6 (after the orphan sweep) so that a
    //    matched paper with zero links survives the sweep - it represents a
    //    real publication that was the article's bibliographic twin, not just
    //    a transient reference, and should go back to the unmatched pool for
    //    re-matching rather than being hard-deleted.
    tx.execute(
        "UPDATE reference_papers \
         SET matched_article_id = NULL, updated_at = datetime('now') \
         WHERE matched_article_id = ?1",
        [article_id],
    )?;

    // 3. Decrement reference/citation counters on shared papers and delete
    //    this article's link rows. The CASCADE on the junction would remove
    //    the rows but not decrement the denormalized counts. `Transaction`
    //    derefs to `Connection`, so `&*tx` participates in the transaction.
    crate::db::reference_repo::delete_references_for_article(&tx, article_id)?;

    // 4. Delete the article row. ON DELETE CASCADE handles the dependent
    //    tables (junction, audit, chunks, translation archive, biblio joins).
    tx.execute("DELETE FROM articles WHERE id = ?1", [article_id])?;

    // 5. Delete orphaned reference papers: rows now with zero inbound links
    //    (citation_count + reference_count == 0) that were never promoted
    //    (match_status = 'unmatched'). Papers still linked to other articles
    //    or already promoted to a library article (match_status = 'matched' /
    //    'imported') are preserved. This runs after the article row is gone so
    //    the CASCADE on article_reference_links has already fired.
    tx.execute(
        "DELETE FROM reference_papers \
         WHERE citation_count = 0 AND reference_count = 0 \
         AND match_status = 'unmatched'",
        [],
    )?;

    // 6. NOW reset match_status to 'unmatched' for papers whose
    //    matched_article_id was just cleared (step 2). They survived the
    //    orphan sweep because their match_status was still 'matched' /
    //    'imported' at that point. Resetting here puts them back in the
    //    unmatched pool so they reappear in Articles of Interest and can be
    //    re-matched or re-promoted. The WHERE clause scopes the reset to
    //    papers that HAVE no matched_article_id (just cleared) AND are not
    //    already 'unmatched', so genuinely unmatched papers are untouched.
    tx.execute(
        "UPDATE reference_papers \
         SET match_status = 'unmatched', updated_at = datetime('now') \
         WHERE matched_article_id IS NULL AND match_status != 'unmatched'",
        [],
    )?;

    tx.commit()?;

    // 7. Remove the on-disk full-text file. Non-fatal: the article is already
    //    deleted; a failure here is logged but does not surface as an error
    //    to the user (the DB is the source of truth, not the file cache).
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
