//! `insert_article` + `insert_articles_batch`.
//!
//! Extracted from the pre-split `article_repo.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::article::{Article, NewArticle};

use super::{
    get_article_by_id, get_article_by_id_tx, next_sequence_id, screening_queries::*, MAX_ARTICLES,
};

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
