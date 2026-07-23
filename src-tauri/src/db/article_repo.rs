use rusqlite::{params, Connection};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::article::{AiDecision, Article, ArticleStatus, NewArticle};

const MAX_ARTICLES: usize = 10_000;

/// Shared SELECT base for the `articles` table.
///
/// Includes the `tags` and `labels` correlated subqueries as `tags_json` /
/// `labels_json` so every article fetch returns the joined data in one shot.
/// All article read functions (`get_article_by_id`, `get_all_articles`,
/// `get_articles_by_status`, `get_articles_for_export`, `get_duplicate_articles`,
/// `get_working_articles`, `query_articles`) compose their SQL by appending a
/// WHERE / ORDER BY clause to this constant. Keeps the column list in one place
/// so a schema change is a single edit, not ten.
const ARTICLE_SELECT_BASE: &str = "\
SELECT articles.*, \
(SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id \
 WHERE at.article_id = articles.id) AS tags_json, \
(SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id \
 WHERE al.article_id = articles.id) AS labels_json \
FROM articles";

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

fn next_sequence_id(conn: &Connection) -> Result<i64, AppError> {
    let max_id: i64 =
        conn.query_row("SELECT COALESCE(MAX(sequence_id), 0) FROM articles", [], |row| row.get(0))?;
    Ok(max_id + 1)
}

pub fn insert_article(conn: &Connection, article: &NewArticle) -> Result<Article, AppError> {
    let id = Uuid::new_v4().to_string();
    let seq_id = next_sequence_id(conn)?;
    let authors_json = serde_json::to_string(&article.authors)?;
    let keywords_json = serde_json::to_string(&article.keywords)?;
    let ris_extras_json =
        article.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    let data_length = article
        .data_length
        .unwrap_or_else(|| article.title.chars().count() + article.abstract_text.chars().count());
    let token_estimate = article.token_estimate.unwrap_or(data_length / 4);

    conn.execute(
        "INSERT INTO articles (
            id, sequence_id, status, title, abstract_text, authors, publication_year, doi,
            journal, volume, issue, start_page, end_page, keywords, url,
            language, publisher, publisher_city, publisher_address, issn, eissn, journal_index_id,
            reference_type, date, author_address, affiliation, accession_number,
            custom_field3, journal_abbreviation, journal_iso_abbreviation,
            notes, web_of_science_db, ris_extras, import_source,
            data_length, token_estimate,
            num_cited, num_references, has_citation_details, has_reference_details,
            has_full_text, full_text_file_name
        ) VALUES (
            ?1, ?2, 'duplicate', ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21,
            ?22, ?23, ?24, ?25, ?26,
            ?27, ?28, ?29,
            ?30, ?31, ?32, ?33,
            ?34, ?35,
            ?36, ?37, 0, 0,
            ?38, ?39
        )",
        params![
            id,
            seq_id,
            article.title,
            article.abstract_text,
            authors_json,
            article.publication_year,
            article.doi,
            article.journal,
            article.volume,
            article.issue,
            article.start_page,
            article.end_page,
            keywords_json,
            article.url,
            article.language,
            article.publisher,
            article.publisher_city,
            article.publisher_address,
            article.issn,
            article.eissn,
            article.journal_index_id,
            article.reference_type,
            article.date,
            article.author_address,
            article.affiliation,
            article.accession_number,
            article.custom_field3,
            article.journal_abbreviation,
            article.journal_iso_abbreviation,
            article.notes,
            article.web_of_science_db,
            ris_extras_json,
            article.import_source,
            data_length,
            token_estimate,
            article.num_cited,
            article.num_references,
            article.has_full_text,
            article.full_text_file_name,
        ],
    )?;

    get_article_by_id(conn, &id)
}

pub fn insert_articles_batch(
    conn: &Connection,
    articles: &[NewArticle],
    import_source: &str,
) -> Result<Vec<Article>, AppError> {
    let remaining = remaining_capacity(conn)?;
    if articles.len() > remaining {
        return Err(AppError::Import(format!(
            "File contains {} articles but only {} slots remain ({} of {} limit reached)",
            articles.len(),
            remaining,
            count_articles(conn)?,
            MAX_ARTICLES,
        )));
    }

    let mut inserted = Vec::with_capacity(articles.len());
    let tx = conn.unchecked_transaction()?;

    // Get base sequence_id once, then increment per article
    let base_seq = next_sequence_id(&tx)?;

    for (seq_offset, article) in articles.iter().enumerate() {
        let mut article_with_source = article.clone();
        article_with_source.import_source = Some(import_source.to_string());
        let id = Uuid::new_v4().to_string();
        let authors_json = serde_json::to_string(&article_with_source.authors)?;
        let keywords_json = serde_json::to_string(&article_with_source.keywords)?;
        let ris_extras_json = article_with_source
            .ris_extras
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        let data_length = article_with_source.data_length.unwrap_or_else(|| {
            article_with_source.title.chars().count()
                + article_with_source.abstract_text.chars().count()
        });
        let token_estimate = article_with_source.token_estimate.unwrap_or(data_length / 4);

        tx.execute(
            "INSERT INTO articles (
                id, sequence_id, status, title, abstract_text, authors, publication_year, doi,
                journal, volume, issue, start_page, end_page, keywords, url,
                language, publisher, publisher_city, publisher_address, issn, eissn, journal_index_id,
                reference_type, date, author_address, affiliation, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation,
                notes, web_of_science_db, ris_extras, import_source,
                data_length, token_estimate,
                num_cited, num_references, has_citation_details, has_reference_details,
                has_full_text, full_text_file_name
            ) VALUES (
                ?1, ?2, 'duplicate', ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                ?22, ?23, ?24, ?25, ?26,
                ?27, ?28, ?29,
                ?30, ?31, ?32, ?33,
                ?34, ?35,
                ?36, ?37, 0, 0,
                ?38, ?39
            )",
            params![
                id,
                base_seq + seq_offset as i64,
                article_with_source.title,
                article_with_source.abstract_text,
                authors_json,
                article_with_source.publication_year,
                article_with_source.doi,
                article_with_source.journal,
                article_with_source.volume,
                article_with_source.issue,
                article_with_source.start_page,
                article_with_source.end_page,
                keywords_json,
                article_with_source.url,
                article_with_source.language,
                article_with_source.publisher,
                article_with_source.publisher_city,
                article_with_source.publisher_address,
                article_with_source.issn,
                article_with_source.eissn,
                article_with_source.journal_index_id,
                article_with_source.reference_type,
                article_with_source.date,
                article_with_source.author_address,
                article_with_source.affiliation,
                article_with_source.accession_number,
                article_with_source.custom_field3,
                article_with_source.journal_abbreviation,
                article_with_source.journal_iso_abbreviation,
                article_with_source.notes,
                article_with_source.web_of_science_db,
                ris_extras_json,
                article_with_source.import_source,
                data_length,
                token_estimate,
                article_with_source.num_cited,
                article_with_source.num_references,
                article_with_source.has_full_text,
                article_with_source.full_text_file_name,
            ],
        )?;

        // Insert audit entry for import
        let audit_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'import', ?3, 'system')",
            params![audit_id, id, format!("Imported from {}", import_source)],
        )?;

        inserted.push(get_article_by_id_tx(&tx, &id)?);
    }

    tx.commit()?;
    Ok(inserted)
}

pub fn get_article_by_id(conn: &Connection, id: &str) -> Result<Article, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} WHERE id = ?1");
    conn.query_row(&sql, [id], row_to_article).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Article {} not found", id))
        }
        other => AppError::Database(other),
    })
}

fn get_article_by_id_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Article, AppError> {
    let sql = format!("{ARTICLE_SELECT_BASE} WHERE id = ?1");
    tx.query_row(&sql, [id], row_to_article).map_err(|e| match e {
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

/// Fetch articles for tab-aware export.
/// - `status`: `"all"` for all articles, or a specific status like `"included"`, `"working"`, etc.
/// - `screening_errors_only`: when true, only working articles with screening errors are returned.
///
/// `status` is bound via `?1` (parameterized) rather than interpolated, per CLAUDE.md
/// ("Never interpolate user input into SQL"). The value flows from a `#[tauri::command]`
/// parameter; enum-controlled, but parameterized for rule compliance and defense-in-depth.
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
    /// Tags the article must NOT have (NOT-filter, exclusion). Mirrors `tags`
    /// but emits a `NOT IN` clause so the UI can toggle a pill between
    /// inclusion (`tags`) and exclusion (`excluded_tags`).
    #[serde(default)]
    pub excluded_tags: Vec<String>,
    /// Labels the article must NOT have (NOT-filter, exclusion). Mirrors
    /// `labels` but emits a `NOT IN` clause.
    #[serde(default)]
    pub excluded_labels: Vec<String>,
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

    // NOT-filters: articles must NOT have any of these tags/labels. Mirrors the
    // inclusion loops above but emits `NOT IN` so the UI can toggle a pill
    // between inclusion (`tags`/`labels`) and exclusion
    // (`excluded_tags`/`excluded_labels`). An article with no matching row in
    // the join table is NOT IN the subquery result, so it passes the filter.
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

/// Whitelist of article metadata fields that the UI can edit in-place via the
/// `update_article_metadata` Tauri command. Each variant maps to exactly one
/// validated `articles` column so SQLite column names are **never** derived
/// from user input (per CLAUDE.md "Never interpolate user input into SQL").
///
/// Variants cover the seven fields surfaced in the Article Detail "Metadata"
/// card: Authors, Affiliation, Journal, Year, Lang, DOI, Keywords. Adding a
/// new editable metadata field means adding a variant here AND extending
/// [`ArticleMetaField::column`] + the value-binding arm in
/// [`update_article_metadata_field`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArticleMetaField {
    Authors,
    Affiliation,
    Journal,
    PublicationYear,
    Language,
    Doi,
    Keywords,
}

impl ArticleMetaField {
    /// The validated `articles` column name this field writes to.
    #[must_use]
    pub fn column(self) -> &'static str {
        match self {
            Self::Authors => "authors",
            Self::Affiliation => "affiliation",
            Self::Journal => "journal",
            Self::PublicationYear => "publication_year",
            Self::Language => "language",
            Self::Doi => "doi",
            Self::Keywords => "keywords",
        }
    }

    /// Human-readable label for the audit detail string.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Authors => "Authors",
            Self::Affiliation => "Affiliation",
            Self::Journal => "Journal",
            Self::PublicationYear => "Year",
            Self::Language => "Language",
            Self::Doi => "DOI",
            Self::Keywords => "Keywords",
        }
    }
}

/// Payload for the `update_article_metadata` Tauri command. The scalar fields
/// arrive as a string (empty string means "clear to NULL"); the two JSON-array
/// fields (`authors`, `keywords`) arrive as `Vec<String>`. The frontend always
/// sends the appropriate variant so the `#[serde(untagged)]` deserialization
/// picks the right one without a discriminator field.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ArticleMetaValue {
    /// Scalar string value (Journal, Year, Language, DOI, Affiliation).
    Scalar(Option<String>),
    /// JSON-array value (Authors, Keywords).
    Array(Vec<String>),
}

/// Valid publication-year range for the metadata editor. Years outside this
/// range are rejected (cleared to NULL) as defense-in-depth; the frontend
/// inline editor also blocks invalid commits with a visible error.
const MIN_PUBLICATION_YEAR: i32 = 1800;
const MAX_PUBLICATION_YEAR: i32 = 2100;

/// Update a single metadata field on an article. The `field` enum validates
/// the column name (no string interpolation); `value` is bound as a parameter.
/// `authors` and `keywords` are serialized to JSON; `publication_year` parses
/// to `Option<i32>` (empty/invalid/out-of-range -> NULL).
///
/// When the `Journal` field changes, `journal_index_id` is re-resolved via
/// `journal_repo::resolve_journal_id` (using the article's existing ISSN/eISSN
/// and the new journal name) so the bibliometric pipelines stay in sync
/// without a manual "Rematch Journals" round-trip. An unrecognized journal
/// name clears `journal_index_id` to `NULL`.
pub fn update_article_metadata_field(
    conn: &Connection,
    article_id: &str,
    field: ArticleMetaField,
    value: ArticleMetaValue,
) -> Result<(), AppError> {
    let col = field.column();
    let sql = format!("UPDATE articles SET {col} = ?1, changed_at = datetime('now') WHERE id = ?2");

    match (field, value) {
        (ArticleMetaField::Authors, ArticleMetaValue::Array(arr)) => {
            let json = serde_json::to_string(&arr)?;
            conn.execute(&sql, params![json, article_id])?;
        }
        (ArticleMetaField::Keywords, ArticleMetaValue::Array(arr)) => {
            let json = serde_json::to_string(&arr)?;
            conn.execute(&sql, params![json, article_id])?;
        }
        (ArticleMetaField::PublicationYear, ArticleMetaValue::Scalar(s)) => {
            // Parse + range-check. Empty/invalid/out-of-range -> NULL.
            let year: Option<i32> = s.and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    trimmed.parse::<i32>().ok()
                }
            });
            let in_range =
                year.is_some_and(|y| (MIN_PUBLICATION_YEAR..=MAX_PUBLICATION_YEAR).contains(&y));
            let bounded = if in_range { year } else { None };
            conn.execute(&sql, params![bounded, article_id])?;
        }
        (ArticleMetaField::Journal, ArticleMetaValue::Scalar(s)) => {
            let bound: Option<&str> =
                s.as_deref().and_then(|v| if v.trim().is_empty() { None } else { Some(v) });
            conn.execute(&sql, params![bound, article_id])?;
            // Re-resolve journal_index_id using ONLY the new journal name (not
            // the article's existing ISSN/eISSN). When the user manually edits
            // the journal name, the old ISSN belongs to the OLD journal — using
            // it to resolve the new name would keep the stale link alive even
            // for a completely different journal. Matching on the typed name
            // only means an unrecognized name correctly clears the link to NULL.
            let journal_id = crate::db::journal_repo::resolve_journal_id(conn, None, None, bound);
            conn.execute(
                "UPDATE articles SET journal_index_id = ?1, changed_at = datetime('now') \
                 WHERE id = ?2",
                params![journal_id, article_id],
            )?;
        }
        (_, ArticleMetaValue::Scalar(s)) => {
            // Empty string -> NULL so "clear the field" sets it to NULL rather
            // than an empty string, matching how RIS import treats absent fields.
            let bound: Option<&str> =
                s.as_deref().and_then(|v| if v.trim().is_empty() { None } else { Some(v) });
            conn.execute(&sql, params![bound, article_id])?;
        }
        // A scalar field sent as an array (or vice versa) is a frontend bug;
        // treat it as a no-op rather than crashing so a malformed payload does
        // not corrupt the row.
        (_, ArticleMetaValue::Array(_)) => {}
    }
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

fn row_to_article(row: &rusqlite::Row<'_>) -> rusqlite::Result<Article> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "duplicate" => ArticleStatus::Duplicate,
        "working" => ArticleStatus::Working,
        "included" => ArticleStatus::Included,
        "rejected" => ArticleStatus::Rejected,
        _ => ArticleStatus::Duplicate,
    };

    let ai_decision_str: Option<String> = row.get("ai_decision")?;
    let ai_decision = ai_decision_str.map(|d| match d.as_str() {
        "include" => AiDecision::Include,
        _ => AiDecision::Exclude,
    });

    let authors_str: String = row.get("authors")?;
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let keywords_str: Option<String> = row.get("keywords")?;
    let keywords: Vec<String> =
        keywords_str.and_then(|k| serde_json::from_str(&k).ok()).unwrap_or_default();

    let matched_inc_str: Option<String> = row.get("matched_inclusion_criteria")?;
    let matched_inclusion: Vec<String> =
        matched_inc_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let matched_exc_str: Option<String> = row.get("matched_exclusion_criteria")?;
    let matched_exclusion: Vec<String> =
        matched_exc_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let ris_extras_str: Option<String> = row.get("ris_extras")?;
    let ris_extras: Option<serde_json::Value> =
        ris_extras_str.and_then(|s| serde_json::from_str(&s).ok());

    let screening_error_int: i32 = row.get("screening_error")?;
    let manual_override_int: i32 = row.get("manual_override")?;

    Ok(Article {
        id: row.get("id")?,
        sequence_id: row.get("sequence_id")?,
        status,
        screening_error: screening_error_int != 0,
        title: row.get("title")?,
        abstract_text: row.get("abstract_text")?,
        authors,
        publication_year: row.get("publication_year")?,
        doi: row.get("doi")?,
        journal: row.get("journal")?,
        volume: row.get("volume")?,
        issue: row.get("issue")?,
        start_page: row.get("start_page")?,
        end_page: row.get("end_page")?,
        keywords,
        url: row.get("url")?,
        language: row.get("language")?,
        publisher: row.get("publisher")?,
        publisher_city: row.get("publisher_city")?,
        publisher_address: row.get("publisher_address")?,
        issn: row.get("issn")?,
        eissn: row.get("eissn")?,
        journal_index_id: row.get("journal_index_id")?,
        reference_type: row.get("reference_type")?,
        date: row.get("date")?,
        author_address: row.get("author_address")?,
        affiliation: row.get("affiliation")?,
        accession_number: row.get("accession_number")?,
        custom_field3: row.get("custom_field3")?,
        journal_abbreviation: row.get("journal_abbreviation")?,
        journal_iso_abbreviation: row.get("journal_iso_abbreviation")?,
        notes: row.get("notes")?,
        web_of_science_db: row.get("web_of_science_db")?,
        user_notes: row.get("user_notes")?,
        ris_extras,
        duplicate_of: row.get("duplicate_of")?,
        ai_decision,
        ai_reasoning: row.get("ai_reasoning")?,
        ai_confidence: row.get("ai_confidence")?,
        matched_inclusion_criteria: matched_inclusion,
        matched_exclusion_criteria: matched_exclusion,
        tags: row
            .get::<_, Option<String>>("tags_json")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        labels: row
            .get::<_, Option<String>>("labels_json")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        manual_override: manual_override_int != 0,
        import_source: row.get("import_source")?,
        imported_at: row.get("imported_at")?,
        changed_at: row.get("changed_at")?,
        screened_at: row.get("screened_at")?,
        data_length: row.get("data_length")?,
        token_estimate: row.get("token_estimate")?,
        actual_tokens: row.get("actual_tokens")?,
        full_text: row.get("full_text")?,
        full_text_ai_summary: row.get("full_text_ai_summary")?,
        num_cited: row.get("num_cited")?,
        num_references: row.get("num_references")?,
        has_citation_details: row.get::<_, i32>("has_citation_details")? != 0,
        has_reference_details: row.get::<_, i32>("has_reference_details")? != 0,
        has_full_text: row.get::<_, i32>("has_full_text")? != 0,
        full_text_file_name: row.get("full_text_file_name")?,
        has_figures_or_tables: row.get::<_, i32>("has_figures_or_tables")? != 0,
        is_translated: row.get::<_, i32>("is_translated")? != 0,
        translation_status: row.get("translation_status")?,
        translation_error: row.get("translation_error")?,
        translated_at: row.get("translated_at")?,
    })
}

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
pub fn bulk_add_tag_to_articles(
    conn: &Connection,
    article_ids: &[String],
    tag_name: &str,
) -> Result<usize, AppError> {
    if article_ids.is_empty() {
        return Ok(0);
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
    let mut count = 0usize;
    for article_id in article_ids {
        let rows = conn.execute(
            "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
            params![article_id, tag_id],
        )?;
        count += rows;
    }
    Ok(count)
}

/// Bulk add a label to multiple articles (by label name).
/// Creates the label if it doesn't exist.
pub fn bulk_add_label_to_articles(
    conn: &Connection,
    article_ids: &[String],
    label_name: &str,
) -> Result<usize, AppError> {
    if article_ids.is_empty() {
        return Ok(0);
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
    let mut count = 0usize;
    for article_id in article_ids {
        let rows = conn.execute(
            "INSERT OR IGNORE INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
            params![article_id, label_id],
        )?;
        count += rows;
    }
    Ok(count)
}

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

// ── Translation status helpers ──────────────────────────────────────────────
//
// The DB-backed translation progress record lives on the `articles` row (there
// is no `translation_jobs` table). These helpers are the single write-path for
// `translation_status` / `is_translated` / `translation_error` / `translated_at`.

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

/// Lightweight article info for batch import DOI matching.
/// Used by the batch-import phases to match files on disk to articles by DOI
/// and to skip articles that already have full text / references / citations.
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

/// Load all articles that have a non-null, non-empty DOI with their full-text /
/// reference / citation / AI-summary flags. Used by the batch-import runner to
/// build the DOI match map in a single query instead of one lookup per file.
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

/// Check which DOIs from the input list already exist in the `articles` table.
/// Uses a single batched query with a dynamically-built parameterized `IN (...)`
/// clause. Returns the subset of DOIs that are present in the library.
///
/// Used by the OpenAlex search integration to grey out the "Add" button for
/// works whose DOI already matches an article in the library.
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

/// Post-import step: resolve `journal_index_id` for articles that have ISSN/eISSN/journal name
/// but no journal link yet. Non-fatal - errors are silently ignored.
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
