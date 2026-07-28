//! Full text + AI summary helpers.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// Get the full text and title for an article (for AI summary generation).
pub fn get_full_text_for_summary(
    conn: &Connection,
    article_id: &str,
) -> Result<(String, String), AppError> {
    let (title, full_text): (String, Option<String>) = conn
        .query_row("SELECT title, full_text FROM articles WHERE id = ?1", [article_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(AppError::Database)?;
    let text = full_text.unwrap_or_default();
    if text.is_empty() {
        return Err(AppError::Validation(format!(
            "No full text available for article {article_id}"
        )));
    }
    Ok((title, text))
}

/// Store the AI-generated summary JSON for an article.
pub fn set_ai_summary(
    conn: &Connection,
    article_id: &str,
    summary_json: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET full_text_ai_summary = ?1, changed_at = datetime('now') WHERE id = ?2",
        params![summary_json, article_id],
    )?;
    Ok(())
}

/// Update the full text and file attachment info for an article.
///
/// `has_figures_or_tables` is computed at attach time by
/// `commands::full_text::attach_full_text_inner` via
/// `utils::sections::extract_captions` (the same detector
/// `generate_figure_descriptions` validates against), so the persisted flag
/// matches the generation path's own precondition.
pub fn update_full_text(
    conn: &Connection,
    article_id: &str,
    full_text: &str,
    file_name: &str,
    has_figures_or_tables: bool,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET full_text = ?1, has_full_text = 1, full_text_file_name = ?2, has_figures_or_tables = ?3, changed_at = datetime('now') WHERE id = ?4",
        params![full_text, file_name, if has_figures_or_tables { 1 } else { 0 }, article_id],
    )?;
    Ok(())
}

/// Clear the full text and file attachment info for an article.
pub fn clear_full_text(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET full_text = NULL, has_full_text = 0, full_text_file_name = NULL, has_figures_or_tables = 0, changed_at = datetime('now') WHERE id = ?1",
        params![article_id],
    )?;
    Ok(())
}

/// Get the full text file name for an article (if any).
pub fn get_full_text_file_name(
    conn: &Connection,
    article_id: &str,
) -> Result<Option<String>, AppError> {
    let file_name: Option<String> = conn.query_row(
        "SELECT full_text_file_name FROM articles WHERE id = ?1",
        [article_id],
        |row| row.get(0),
    )?;
    Ok(file_name)
}
