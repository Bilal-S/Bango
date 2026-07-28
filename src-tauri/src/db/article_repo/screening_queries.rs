//! Screening-batch queries + counts on the `articles` table.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::Connection;

use crate::error::AppError;
use crate::models::article::Article;

use super::MAX_ARTICLES;

pub fn count_articles(conn: &Connection) -> Result<usize, AppError> {
    let count: usize = conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))?;
    Ok(count)
}

/// Count unscreened articles in the working list (status = 'working' AND screened_at IS NULL).
pub fn count_unscreened_working(conn: &Connection) -> Result<usize, AppError> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM articles WHERE status = 'working' AND screened_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Count all articles in the working list (status = 'working').
pub fn count_working(conn: &Connection) -> Result<usize, AppError> {
    let count: usize =
        conn.query_row("SELECT COUNT(*) FROM articles WHERE status = 'working'", [], |row| {
            row.get(0)
        })?;
    Ok(count)
}

/// Get the MAX character length (title + abstract) among unscreened working articles.
/// Used for worst-case token estimation without materializing any rows.
/// Uses the pre-computed `data_length` column to avoid per-query LENGTH() calculations.
/// Returns 0 if no unscreened working articles exist.
pub fn max_article_char_len(conn: &Connection) -> Result<usize, AppError> {
    let max_len: usize = conn.query_row(
        "SELECT COALESCE(MAX(data_length), 0) FROM articles \
         WHERE status = 'working' AND screened_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(max_len)
}

/// Fetch a small batch of unscreened working articles.
/// Optimized to fetch only necessary fields for screening.
pub fn get_next_unscreened_working_batch(
    conn: &Connection,
    limit: usize,
    after_sequence_id: Option<i64>,
) -> Result<Vec<Article>, AppError> {
    // The `after_sequence_id` cursor lets the screening engine advance past
    // articles it already attempted in the current run (e.g. a transient LLM
    // error left them unscreened). Without it, the engine would re-fetch the
    // same unscreened batch forever within a single run. A fresh run (new
    // engine instance) starts with `None` so all unscreened articles are
    // eligible again.
    // Use a single SQL with `COALESCE` so we always bind both params (?1=limit, ?2=cursor).
    // When `after_sequence_id` is None (fresh run), bind 0 so `sequence_id > 0` matches all.
    let cursor = after_sequence_id.unwrap_or(0);
    let mut stmt = conn.prepare(
        "SELECT id, sequence_id, title, abstract_text, authors, publication_year, has_full_text \
         FROM articles \
          WHERE status = 'working' AND screened_at IS NULL AND sequence_id > ?2 \
          ORDER BY sequence_id ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit, cursor], |row| {
        Ok(Article {
            id: row.get(0)?,
            sequence_id: row.get(1)?,
            title: row.get(2)?,
            abstract_text: row.get(3)?,
            authors: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
            publication_year: row.get(5)?,
            // Fill other fields with defaults as they aren't needed for the screening prompt
            status: crate::models::article::ArticleStatus::Working,
            screening_error: false,
            doi: None,
            journal: None,
            volume: None,
            issue: None,
            start_page: None,
            end_page: None,
            keywords: vec![],
            url: None,
            language: None,
            publisher: None,
            publisher_city: None,
            publisher_address: None,
            issn: None,
            eissn: None,
            journal_index_id: None,
            reference_type: None,
            date: None,
            author_address: None,
            affiliation: None,
            accession_number: None,
            custom_field3: None,
            journal_abbreviation: None,
            journal_iso_abbreviation: None,
            notes: None,
            web_of_science_db: None,
            user_notes: None,
            ris_extras: None,
            duplicate_of: None,
            ai_decision: None,
            ai_reasoning: None,
            ai_confidence: None,
            matched_inclusion_criteria: vec![],
            matched_exclusion_criteria: vec![],
            tags: vec![],
            labels: vec![],
            manual_override: false,
            import_source: None,
            imported_at: "".to_string(),
            changed_at: "".to_string(),
            screened_at: None,
            data_length: None,
            token_estimate: None,
            actual_tokens: None,
            full_text: None,
            full_text_ai_summary: None,
            num_cited: None,
            num_references: None,
            has_citation_details: false,
            has_reference_details: false,
            // Tier 3: read the real has_full_text flag so the screening engine
            // knows which articles have retrievable full-text evidence chunks.
            has_full_text: row.get::<_, i32>(6)? != 0,
            full_text_file_name: None,
            // Screening does not need the figures/tables flag; default false.
            has_figures_or_tables: false,
            // Translation status is not needed by the screening batch fetch;
            // the translation worker reads it via dedicated queries.
            is_translated: false,
            translation_status: "none".to_string(),
            translation_error: None,
            translated_at: None,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Fetch a specific unscreened working article by its UUID, using the same
/// minimal field set as `get_next_unscreened_working_batch` (only the columns
/// needed by the screening prompt). Returns `None` if the article is not found,
/// not in `working` status, or has already been screened (`screened_at IS NOT
/// NULL`).
///
/// Powers the per-article "Screen" button in the article detail panel: the user
/// clicks Screen on a specific article and the engine screens that exact ID
/// (instead of the next-by-`sequence_id` one the batch path would pick). The
/// `Option` return lets the command layer distinguish "article already
/// screened / not eligible" from "article not found" without a separate
/// existence check.
pub fn get_unscreened_working_article_by_id(
    conn: &Connection,
    article_id: &str,
) -> Result<Option<Article>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, sequence_id, title, abstract_text, authors, publication_year, has_full_text \
         FROM articles \
         WHERE id = ?1 AND status = 'working' AND screened_at IS NULL",
    )?;
    let mut rows = stmt.query_map([article_id], |row| {
        Ok(Article {
            id: row.get(0)?,
            sequence_id: row.get(1)?,
            title: row.get(2)?,
            abstract_text: row.get(3)?,
            authors: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
            publication_year: row.get(5)?,
            status: crate::models::article::ArticleStatus::Working,
            screening_error: false,
            doi: None,
            journal: None,
            volume: None,
            issue: None,
            start_page: None,
            end_page: None,
            keywords: vec![],
            url: None,
            language: None,
            publisher: None,
            publisher_city: None,
            publisher_address: None,
            issn: None,
            eissn: None,
            journal_index_id: None,
            reference_type: None,
            date: None,
            author_address: None,
            affiliation: None,
            accession_number: None,
            custom_field3: None,
            journal_abbreviation: None,
            journal_iso_abbreviation: None,
            notes: None,
            web_of_science_db: None,
            user_notes: None,
            ris_extras: None,
            duplicate_of: None,
            ai_decision: None,
            ai_reasoning: None,
            ai_confidence: None,
            matched_inclusion_criteria: vec![],
            matched_exclusion_criteria: vec![],
            tags: vec![],
            labels: vec![],
            manual_override: false,
            import_source: None,
            imported_at: "".to_string(),
            changed_at: "".to_string(),
            screened_at: None,
            data_length: None,
            token_estimate: None,
            actual_tokens: None,
            full_text: None,
            full_text_ai_summary: None,
            num_cited: None,
            num_references: None,
            has_citation_details: false,
            has_reference_details: false,
            has_full_text: row.get::<_, i32>(6)? != 0,
            full_text_file_name: None,
            has_figures_or_tables: false,
            is_translated: false,
            translation_status: "none".to_string(),
            translation_error: None,
            translated_at: None,
        })
    })?;
    // Take the first row (there can be at most one since `id` is the primary key).
    let article = rows.next().transpose()?;
    Ok(article)
}

pub fn remaining_capacity(conn: &Connection) -> Result<usize, AppError> {
    let count = count_articles(conn)?;
    Ok(MAX_ARTICLES.saturating_sub(count))
}
