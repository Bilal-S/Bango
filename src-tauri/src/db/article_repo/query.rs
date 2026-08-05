//! `ArticleQuery` + `query_articles` + the read-many fns.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::Connection;
use serde::Deserialize;

use crate::error::AppError;
use crate::models::article::Article;

use super::{row_to_article, ARTICLE_SELECT_BASE};

pub fn get_article_by_id(conn: &Connection, id: &str) -> Result<Article, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_article).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Article {} not found", id))
        }
        other => AppError::Database(other),
    })
}

pub fn get_all_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} ORDER BY imported_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_articles_by_status(conn: &Connection, status: &str) -> Result<Vec<Article>, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} WHERE status = ?1 ORDER BY imported_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([status], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Fetch articles for tab-aware export. `status` is bound as `?1` (not interpolated).
/// - `"all"`: all articles. `screening_errors_only`: working + screened.
pub fn get_articles_for_export(
    conn: &Connection,
    status: &str,
    screening_errors_only: bool,
) -> Result<Vec<Article>, AppError> {
    // Three branches: all / screening-errors-only / specific-status.
    // Each composes a parameterized SQL string from `ARTICLE_SELECT_BASE`.
    let (sql, params_boxed): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if status == "all"
        && !screening_errors_only
    {
        (format!("{ARTICLE_SELECT_BASE} ORDER BY imported_at DESC"), vec![])
    } else if screening_errors_only {
        (
                format!(
                    "{ARTICLE_SELECT_BASE} WHERE status = 'working' AND screened_at IS NOT NULL ORDER BY imported_at DESC"
                ),
                vec![],
            )
    } else {
        (
            format!("{ARTICLE_SELECT_BASE} WHERE status = ?1 ORDER BY imported_at DESC"),
            vec![Box::new(status.to_string())],
        )
    };
    let params: Vec<&dyn rusqlite::types::ToSql> =
        params_boxed.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Fetch articles by UUIDs for the "Export Selected" bulk action.
/// Composes `ARTICLE_SELECT_BASE` with parameterized `IN (?,…)` — one `?` per id
/// (no string interpolation). Empty input → empty vec. Unknown ids silently absent.
pub fn get_articles_by_ids(conn: &Connection, ids: &[String]) -> Result<Vec<Article>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql =
        format!("{ARTICLE_SELECT_BASE} WHERE id IN ({placeholders}) ORDER BY imported_at DESC");
    let params: Vec<&dyn rusqlite::types::ToSql> =
        ids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_duplicate_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let sql = format!(
        "{ARTICLE_SELECT_BASE} WHERE status = 'duplicate' AND duplicate_of IS NULL ORDER BY imported_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_working_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let sql = format!(
        "{ARTICLE_SELECT_BASE} WHERE status = 'working' AND duplicate_of IS NULL ORDER BY imported_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleQuery {
    pub status: Option<String>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub manual_override_only: bool,
    pub screening_errors_only: bool,
    pub author: Option<String>,
    pub journal: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Tags the article must NOT have (NOT-filter). Mirrors `tags` but emits `NOT IN`.
    #[serde(default)]
    pub excluded_tags: Vec<String>,
    /// Labels the article must NOT have (NOT-filter). Mirrors `labels` but emits `NOT IN`.
    #[serde(default)]
    pub excluded_labels: Vec<String>,
    /// Case-insensitive partial-match on `doi` (`LOWER(doi) LIKE '%...%'`).
    #[serde(default)]
    pub doi: Option<String>,
    /// When true, restrict to articles with no DOI (`doi IS NULL OR doi = ''`).
    /// Mutually exclusive with `doi`; this wins if both are set.
    #[serde(default)]
    pub doi_empty: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub fn query_articles(conn: &Connection, query: &ArticleQuery) -> Result<Vec<Article>, AppError> {
    let is_duplicate_view = query.status.as_deref() == Some("duplicate");
    let is_all_view = query.status.is_none();
    let base_filter =
        if is_duplicate_view || is_all_view { " WHERE 1=1" } else { " WHERE duplicate_of IS NULL" };
    let mut sql = format!("{ARTICLE_SELECT_BASE}{base_filter}");
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref status) = query.status {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" AND status = ?{idx}"));
        param_values.push(Box::new(status.clone()));
    }

    if let Some(ref search) = query.search {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND (LOWER(title) LIKE ?{idx} OR LOWER(abstract_text) LIKE ?{idx} OR LOWER(COALESCE(user_notes, '')) LIKE ?{idx})"
        ));
        let pattern = format!("%{}%", search.to_lowercase());
        param_values.push(Box::new(pattern));
    }

    if let Some(year_from) = query.year_from {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" AND publication_year >= ?{idx}"));
        param_values.push(Box::new(year_from));
    }

    if let Some(year_to) = query.year_to {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" AND publication_year <= ?{idx}"));
        param_values.push(Box::new(year_to));
    }

    if query.manual_override_only {
        sql.push_str(" AND manual_override = 1");
    }

    if query.screening_errors_only {
        // Error = working article that was screened but didn't get a status change
        sql.push_str(" AND status = 'working' AND screened_at IS NOT NULL");
    }

    if let Some(ref author) = query.author {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" AND LOWER(authors) LIKE ?{idx}"));
        let pattern = format!("%{}%", author.to_lowercase());
        param_values.push(Box::new(pattern));
    }

    if let Some(ref journal) = query.journal {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" AND LOWER(journal) LIKE ?{idx}"));
        let pattern = format!("%{}%", journal.to_lowercase());
        param_values.push(Box::new(pattern));
    }

    // DOI filter. The empty-DOI branch wins if both set (avoids contradictory SQL).
    if query.doi_empty {
        sql.push_str(" AND (doi IS NULL OR doi = '')");
    } else if let Some(ref doi) = query.doi {
        let trimmed = doi.trim();
        if !trimmed.is_empty() {
            let idx = param_values.len() + 1;
            sql.push_str(&format!(" AND LOWER(doi) LIKE ?{idx}"));
            let pattern = format!("%{}%", trimmed.to_lowercase());
            param_values.push(Box::new(pattern));
        }
    }

    for tag in &query.tags {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND articles.id IN (SELECT at.article_id FROM article_tags at JOIN tags t ON at.tag_id = t.id WHERE LOWER(t.name) = ?{idx})"
        ));
        param_values.push(Box::new(tag.to_lowercase()));
    }

    for label in &query.labels {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND articles.id IN (SELECT al.article_id FROM article_labels al JOIN labels l ON al.label_id = l.id WHERE LOWER(l.name) = ?{idx})"
        ));
        param_values.push(Box::new(label.to_lowercase()));
    }

    // NOT-filters: articles must NOT have these tags/labels. Mirrors inclusion loops
    // but emits `NOT IN`. An article with no matching junction row passes.
    for tag in &query.excluded_tags {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND articles.id NOT IN (SELECT at.article_id FROM article_tags at JOIN tags t ON at.tag_id = t.id WHERE LOWER(t.name) = ?{idx})"
        ));
        param_values.push(Box::new(tag.to_lowercase()));
    }

    for label in &query.excluded_labels {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND articles.id NOT IN (SELECT al.article_id FROM article_labels al JOIN labels l ON al.label_id = l.id WHERE LOWER(l.name) = ?{idx})"
        ));
        param_values.push(Box::new(label.to_lowercase()));
    }

    let sort_by = query.sort_by.as_deref().unwrap_or("imported_at");
    let sort_dir = match query.sort_dir.as_deref() {
        Some(d) if d.eq_ignore_ascii_case("asc") => "ASC",
        _ => "DESC",
    };
    let order_clause = match sort_by {
        "index" => format!(" ORDER BY sequence_id {sort_dir}"),
        "title" => format!(" ORDER BY title COLLATE NOCASE {sort_dir}"),
        "authors" => format!(" ORDER BY authors COLLATE NOCASE {sort_dir} NULLS LAST"),
        "journal" => format!(" ORDER BY journal COLLATE NOCASE {sort_dir} NULLS LAST"),
        "publicationYear" => format!(" ORDER BY publication_year {sort_dir} NULLS LAST"),
        "status" => format!(" ORDER BY status COLLATE NOCASE {sort_dir}"),
        "aiConfidence" => format!(" ORDER BY ai_confidence {sort_dir} NULLS LAST"),
        "importedAt" => format!(" ORDER BY imported_at {sort_dir}"),
        "changedAt" => format!(" ORDER BY changed_at {sort_dir}"),
        _ => format!(" ORDER BY changed_at {sort_dir}"),
    };
    sql.push_str(&order_clause);

    // Pagination: LIMIT / OFFSET
    if let Some(limit) = query.limit {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(" LIMIT ?{idx}"));
        param_values.push(Box::new(limit));
        if let Some(offset) = query.offset {
            let idx = param_values.len() + 1;
            sql.push_str(&format!(" OFFSET ?{idx}"));
            param_values.push(Box::new(offset));
        }
    }

    let params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
