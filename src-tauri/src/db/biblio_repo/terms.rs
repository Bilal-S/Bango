use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::biblio::{BiblioTerm, TermSource, TermType};

/// Upsert a term: insert if new normalized_term+term_type combo, otherwise increment article_count.
/// When an AI-extracted term already exists, metadata normalisation reuses it instead of creating a duplicate.
/// Returns the term ID.
pub fn upsert_term(
    conn: &Connection,
    raw_term: &str,
    normalized_term: &str,
    term_type: &TermType,
    source: &TermSource,
) -> Result<String, AppError> {
    let type_str = term_type.to_string();
    let source_str = source.to_string();

    // Try to find existing (by normalized_term + term_type, regardless of source)
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_terms WHERE normalized_term = ?1 AND term_type = ?2",
            rusqlite::params![normalized_term, type_str],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        // Increment article_count
        conn.execute(
            "UPDATE biblio_terms SET article_count = article_count + 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(id)
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_terms (id, normalized_term, raw_term, term_type, source, article_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            rusqlite::params![id, normalized_term, raw_term, type_str, source_str],
        )?;
        Ok(id)
    }
}

/// Link an article to a term. Creates or increments frequency.
pub fn link_article_term(
    conn: &Connection,
    article_id: &str,
    term_id: &str,
) -> Result<(), AppError> {
    // Check if link exists
    let existing: Option<i32> = conn
        .query_row(
            "SELECT frequency FROM biblio_article_terms WHERE article_id = ?1 AND term_id = ?2",
            rusqlite::params![article_id, term_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(freq) = existing {
        conn.execute(
            "UPDATE biblio_article_terms SET frequency = ?1 WHERE article_id = ?2 AND term_id = ?3",
            rusqlite::params![freq + 1, article_id, term_id],
        )?;
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_article_terms (id, article_id, term_id, frequency) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![id, article_id, term_id],
        )?;
    }
    Ok(())
}

/// Get all terms linked to an article.
pub fn get_terms_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<BiblioTerm>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.normalized_term, t.raw_term, t.term_type, t.article_count, t.created_at, t.source \
         FROM biblio_terms t \
         JOIN biblio_article_terms bat ON t.id = bat.term_id \
         WHERE bat.article_id = ?1 \
         ORDER BY t.normalized_term",
    )?;
    let terms = stmt
        .query_map(rusqlite::params![article_id], map_row_to_term)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(terms)
}

/// Save extracted terms (from LLM noun-phrase extraction or keywords) for an article.
pub fn save_article_terms(
    conn: &Connection,
    article_id: &str,
    terms: &[(String, TermType, TermSource)],
) -> Result<(), AppError> {
    for (raw_term, term_type, source) in terms {
        let normalized = crate::biblio::normalizer::normalize_term(raw_term);
        if normalized.is_empty() {
            continue;
        }
        let term_id = upsert_term(conn, raw_term, &normalized, term_type, source)?;
        let _ = link_article_term(conn, article_id, &term_id);
    }
    Ok(())
}

/// Get all terms.
pub fn get_all_terms(conn: &Connection) -> Result<Vec<BiblioTerm>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_term, raw_term, term_type, article_count, created_at, source \
         FROM biblio_terms ORDER BY article_count DESC",
    )?;
    let terms = stmt.query_map([], map_row_to_term)?.collect::<Result<Vec<_>, _>>()?;
    Ok(terms)
}

/// Helper mapping database rows to `BiblioTerm`.
fn map_row_to_term(row: &rusqlite::Row) -> Result<BiblioTerm, rusqlite::Error> {
    let type_str: String = row.get(3)?;
    let term_type =
        if type_str == "noun_phrase" { TermType::NounPhrase } else { TermType::Keyword };
    let source_str: String = row.get(6)?;
    let source = match source_str.as_str() {
        "ai_extracted" => TermSource::AiExtracted,
        "user_added" => TermSource::UserAdded,
        _ => TermSource::Metadata,
    };
    Ok(BiblioTerm {
        id: row.get(0)?,
        normalized_term: row.get(1)?,
        raw_term: row.get(2)?,
        term_type,
        source,
        article_count: row.get(4)?,
        created_at: row.get(5)?,
    })
}
