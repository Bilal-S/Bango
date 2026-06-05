use rusqlite::{params, Connection};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::article::{AiDecision, Article, ArticleStatus, NewArticle};

const MAX_ARTICLES: usize = 10_000;

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
) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, sequence_id, title, abstract_text, authors, publication_year FROM articles \
          WHERE status = 'working' AND screened_at IS NULL \
          ORDER BY sequence_id ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
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
            reference_type: None,
            date: None,
            author_address: None,
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
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
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
            language, publisher, publisher_city, publisher_address, issn,
            reference_type, date, author_address, accession_number,
            custom_field3, journal_abbreviation, journal_iso_abbreviation,
            notes, web_of_science_db, ris_extras, import_source,
            data_length, token_estimate
        ) VALUES (
            ?1, ?2, 'duplicate', ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19,
            ?20, ?21, ?22, ?23,
            ?24, ?25, ?26,
            ?27, ?28, ?29, ?30,
            ?31, ?32
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
            article.reference_type,
            article.date,
            article.author_address,
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
                language, publisher, publisher_city, publisher_address, issn,
                reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation,
                notes, web_of_science_db, ris_extras, import_source,
                data_length, token_estimate
            ) VALUES (
                ?1, ?2, 'duplicate', ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19,
                ?20, ?21, ?22, ?23,
                ?24, ?25, ?26,
                ?27, ?28, ?29, ?30,
                ?31, ?32
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
                article_with_source.reference_type,
                article_with_source.date,
                article_with_source.author_address,
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
    conn.query_row("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles WHERE id = ?1", [id], row_to_article).map_err(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Article {} not found", id))
            }
            other => AppError::Database(other),
        },
    )
}

fn get_article_by_id_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Article, AppError> {
    tx.query_row("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles WHERE id = ?1", [id], row_to_article).map_err(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Article {} not found", id))
            }
            other => AppError::Database(other),
        },
    )
}

pub fn get_all_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_articles_by_status(conn: &Connection, status: &str) -> Result<Vec<Article>, AppError> {
    let mut stmt =
        conn.prepare("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles WHERE status = ?1 ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([status], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_duplicate_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles WHERE status = 'duplicate' AND duplicate_of IS NULL ORDER BY imported_at DESC"
    )?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_working_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt =
        conn.prepare("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles WHERE status = 'working' AND duplicate_of IS NULL ORDER BY imported_at DESC")?;
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
    conn.execute("UPDATE articles SET status = 'working', changed_at = datetime('now') WHERE id = ?1", params![article_id])?;
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
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub fn query_articles(conn: &Connection, query: &ArticleQuery) -> Result<Vec<Article>, AppError> {
    let is_duplicate_view = query.status.as_deref() == Some("duplicate");
    let is_all_view = query.status.is_none();
    let base_filter = if is_duplicate_view || is_all_view { "" } else { " WHERE duplicate_of IS NULL" };
    let mut sql = format!("SELECT articles.*, (SELECT json_group_array(t.name) FROM tags t JOIN article_tags at ON t.id = at.tag_id WHERE at.article_id = articles.id) AS tags_json, (SELECT json_group_array(l.name) FROM labels l JOIN article_labels al ON l.id = al.label_id WHERE al.article_id = articles.id) AS labels_json FROM articles{base_filter}");
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref status) = query.status {
        let idx = param_values.len() + 1;
        if is_duplicate_view {
            sql.push_str(&format!(" WHERE status = ?{idx}"));
        } else {
            sql.push_str(&format!(" AND status = ?{idx}"));
        }
        param_values.push(Box::new(status.clone()));
    }

    if let Some(ref search) = query.search {
        let idx = param_values.len() + 1;
        sql.push_str(&format!(
            " AND (LOWER(title) LIKE ?{idx} OR LOWER(abstract_text) LIKE ?{idx})"
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

    conn.execute(
        "UPDATE articles SET status = ?1, manual_override = 1, changed_at = datetime('now') WHERE id = ?2",
        params![new_status, article_id],
    )?;

    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "status_change",
        Some(&old_status),
        Some(new_status),
        Some("Manual status change"),
        "user",
    )?;

    Ok(())
}

pub fn update_article_tags(
    conn: &Connection,
    article_id: &str,
    tag_names: &[String],
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
        [article_id],
    )?;
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
    conn.execute(
        "UPDATE articles SET changed_at = datetime('now') WHERE id = ?1",
        [article_id],
    )?;
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
    conn.execute("UPDATE articles SET user_notes = ?1, changed_at = datetime('now') WHERE id = ?2", params![notes, article_id])?;
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
        reference_type: row.get("reference_type")?,
        date: row.get("date")?,
        author_address: row.get("author_address")?,
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

/// Reset the working list: clear `screened_at` and `screening_error` for all working articles
/// that have been previously screened, so they can be re-screened.
pub fn reset_working_list(conn: &Connection) -> Result<usize, AppError> {
    let rows = conn.execute(
        "UPDATE articles SET screened_at = NULL, screening_error = 0, changed_at = datetime('now') \
         WHERE status = 'working' AND screened_at IS NOT NULL",
        [],
    )?;
    Ok(rows)
}

/// Bulk update status for multiple articles in a single transaction.
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
        let rows = conn.execute(
            "UPDATE articles SET status = ?1, manual_override = 1, changed_at = datetime('now') WHERE id = ?2",
            params![new_status, id],
        )?;
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

    Ok(counts)
}
