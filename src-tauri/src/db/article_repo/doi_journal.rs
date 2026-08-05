//! DOI / journal / counts helpers + `rematch_all_journals`.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::article::Article;

/// Lightweight article info for batch import DOI matching.
#[derive(Debug, Clone)]
pub struct ArticleDoiInfo {
    pub id: String,
    pub doi: String,
    pub has_full_text: bool,
    pub has_reference_details: bool,
    pub has_citation_details: bool,
    /// `Some` if the article already has an AI summary blob, `None` otherwise.
    /// Used by Phase 3 to skip articles that already have a summary.
    pub has_ai_summary: bool,
}

/// Load articles with non-null DOI + full-text/ref/citation/AI-summary flags.
/// Single query for the batch-import DOI match map.
pub fn get_articles_with_doi_info(conn: &Connection) -> Result<Vec<ArticleDoiInfo>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, doi, has_full_text, has_reference_details, has_citation_details, \
         (full_text_ai_summary IS NOT NULL AND full_text_ai_summary != '') AS has_ai_summary \
         FROM articles \
         WHERE doi IS NOT NULL AND TRIM(doi) != '' AND duplicate_of IS NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ArticleDoiInfo {
            id: row.get(0)?,
            doi: row.get::<_, String>(1)?,
            has_full_text: row.get::<_, i64>(2)? != 0,
            has_reference_details: row.get::<_, i64>(3)? != 0,
            has_citation_details: row.get::<_, i64>(4)? != 0,
            has_ai_summary: row.get::<_, i64>(5)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_article_counts(
    conn: &Connection,
) -> Result<crate::models::article::ArticleCounts, AppError> {
    // Count non-duplicate statuses, excluding merged-away articles (duplicate_of IS NOT NULL).
    // This matches the base_filter applied in query_articles for non-duplicate views.
    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) FROM articles WHERE duplicate_of IS NULL AND status != 'duplicate' GROUP BY status"
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?)))?;

    let mut counts = crate::models::article::ArticleCounts {
        all: 0,
        duplicate: 0,
        working: 0,
        included: 0,
        rejected: 0,
        error: 0,
        references: 0,
    };

    for (status, count) in rows.flatten() {
        counts.all += count;
        match status.as_str() {
            "working" => counts.working = count,
            "included" => counts.included = count,
            "rejected" => counts.rejected = count,
            _ => {}
        }
    }

    // Count duplicates: all articles with status = 'duplicate' (no duplicate_of filter,
    // matching the duplicate tab view in query_articles which uses no base_filter).
    let dup_count: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'duplicate'", [], |row| row.get(0))
        .unwrap_or(0);
    counts.duplicate = dup_count;
    counts.all += dup_count;

    // Count screening errors: working articles that were screened but didn't get a status change,
    // excluding merged-away articles.
    let error_count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'working' AND screened_at IS NOT NULL AND duplicate_of IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    counts.error = error_count;

    // Count references: all reference papers
    let ref_count: usize =
        conn.query_row("SELECT COUNT(*) FROM reference_papers", [], |row| row.get(0)).unwrap_or(0);
    counts.references = ref_count;

    Ok(counts)
}

/// Check which DOIs already exist in `articles`. Batched parameterized `IN (...)` query.
/// Returns the subset of DOIs present in the library.
pub fn check_dois_in_library(conn: &Connection, dois: &[String]) -> Result<Vec<String>, AppError> {
    if dois.is_empty() {
        return Ok(Vec::new());
    }

    // Build a parameterized IN clause: `WHERE doi IN (?1, ?2, ?3, ...)`
    let placeholders: Vec<String> = (1..=dois.len()).map(|i| format!("?{i}")).collect();
    let placeholder_str = placeholders.join(", ");
    let sql = format!("SELECT DISTINCT doi FROM articles WHERE doi IN ({placeholder_str})");

    let params: Vec<&dyn rusqlite::types::ToSql> =
        dois.iter().map(|d| d as &dyn rusqlite::types::ToSql).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
    let found: Vec<String> = rows.filter_map(|r| r.ok()).collect();

    Ok(found)
}

/// Post-import: resolve `journal_index_id` for articles with ISSN/eISSN/journal
/// but no journal link. Non-fatal.
pub fn resolve_journal_links(conn: &Connection, articles: &[Article]) -> usize {
    let mut resolved = 0usize;
    for article in articles {
        if article.journal_index_id.is_some() {
            continue;
        }
        // Only attempt journal matching for journal articles
        if article.reference_type.as_deref() != Some("JOUR") {
            continue;
        }
        let journal_id = crate::db::journal_repo::resolve_journal_id(
            conn,
            article.issn.as_deref(),
            article.eissn.as_deref(),
            article.journal.as_deref(),
        );
        if let Some(ref id) = journal_id {
            let _ = conn.execute(
                "UPDATE articles SET journal_index_id = ?1 WHERE id = ?2",
                params![id, article.id],
            );
            resolved += 1;
        }
    }
    resolved
}

/// Bulk rematch: find all articles with `journal_index_id IS NULL` and `reference_type = 'JOUR'`,
/// attempt to resolve their journal link, and return the count of newly resolved articles.
pub fn rematch_all_journals(conn: &Connection) -> Result<usize, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, issn, eissn, journal FROM articles
         WHERE journal_index_id IS NULL
         AND reference_type = 'JOUR'
         AND (issn IS NOT NULL AND issn != ''
              OR eissn IS NOT NULL AND eissn != ''
              OR journal IS NOT NULL AND journal != '')",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("id")?,
            row.get::<_, Option<String>>("issn")?,
            row.get::<_, Option<String>>("eissn")?,
            row.get::<_, Option<String>>("journal")?,
        ))
    })?;

    let mut resolved = 0usize;
    for row in rows {
        let (id, issn, eissn, journal) = row?;
        if let Some(journal_id) = crate::db::journal_repo::resolve_journal_id(
            conn,
            issn.as_deref(),
            eissn.as_deref(),
            journal.as_deref(),
        ) {
            conn.execute(
                "UPDATE articles SET journal_index_id = ?1 WHERE id = ?2",
                params![journal_id, id],
            )?;
            resolved += 1;
        }
    }

    Ok(resolved)
}
