use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::article::{AiDecision, Article, ArticleStatus, NewArticle};

const MAX_ARTICLES: usize = 10_000;

pub fn count_articles(conn: &Connection) -> Result<usize, AppError> {
    let count: usize = conn.query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))?;
    Ok(count)
}

pub fn remaining_capacity(conn: &Connection) -> Result<usize, AppError> {
    let count = count_articles(conn)?;
    Ok(MAX_ARTICLES.saturating_sub(count))
}

pub fn insert_article(conn: &Connection, article: &NewArticle) -> Result<Article, AppError> {
    let id = Uuid::new_v4().to_string();
    let authors_json = serde_json::to_string(&article.authors)?;
    let keywords_json = serde_json::to_string(&article.keywords)?;
    let ris_extras_json =
        article.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO articles (
            id, status, title, abstract_text, authors, publication_year, doi,
            journal, volume, issue, start_page, end_page, keywords, url,
            language, publisher, publisher_city, publisher_address, issn,
            reference_type, date, author_address, accession_number,
            custom_field3, journal_abbreviation, journal_iso_abbreviation,
            notes, web_of_science_db, ris_extras, import_source
        ) VALUES (
            ?1, 'imported', ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25,
            ?26, ?27, ?28, ?29
        )",
        params![
            id,
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

    for article in articles {
        let mut article_with_source = article.clone();
        article_with_source.import_source = Some(import_source.to_string());
        let id = Uuid::new_v4().to_string();
        let authors_json = serde_json::to_string(&article_with_source.authors)?;
        let keywords_json = serde_json::to_string(&article_with_source.keywords)?;
        let ris_extras_json = article_with_source
            .ris_extras
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        tx.execute(
            "INSERT INTO articles (
                id, status, title, abstract_text, authors, publication_year, doi,
                journal, volume, issue, start_page, end_page, keywords, url,
                language, publisher, publisher_city, publisher_address, issn,
                reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation,
                notes, web_of_science_db, ris_extras, import_source
            ) VALUES (
                ?1, 'imported', ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22,
                ?23, ?24, ?25,
                ?26, ?27, ?28, ?29
            )",
            params![
                id,
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
    conn.query_row("SELECT * FROM articles WHERE id = ?1", [id], row_to_article).map_err(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Article {} not found", id))
            }
            other => AppError::Database(other),
        },
    )
}

fn get_article_by_id_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Article, AppError> {
    tx.query_row("SELECT * FROM articles WHERE id = ?1", [id], row_to_article).map_err(
        |e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("Article {} not found", id))
            }
            other => AppError::Database(other),
        },
    )
}

pub fn get_all_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM articles ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_articles_by_status(conn: &Connection, status: &str) -> Result<Vec<Article>, AppError> {
    let mut stmt =
        conn.prepare("SELECT * FROM articles WHERE status = ?1 ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([status], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn row_to_article(row: &rusqlite::Row<'_>) -> rusqlite::Result<Article> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "imported" => ArticleStatus::Imported,
        "working" => ArticleStatus::Working,
        "included" => ArticleStatus::Included,
        "rejected" => ArticleStatus::Rejected,
        _ => ArticleStatus::Imported,
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
        tags: vec![],
        labels: vec![],
        manual_override: manual_override_int != 0,
        import_source: row.get("import_source")?,
        imported_at: row.get("imported_at")?,
        screened_at: row.get("screened_at")?,
    })
}
