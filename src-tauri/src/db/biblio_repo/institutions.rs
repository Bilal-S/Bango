use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::biblio::BiblioInstitution;

/// Upsert an institution. Returns the institution ID and a boolean indicating if it was newly created.
pub fn upsert_institution(
    conn: &Connection,
    normalized_name: &str,
    country: Option<&str>,
    city: Option<&str>,
) -> Result<(String, bool), AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_institutions WHERE normalized_name = ?1",
            rusqlite::params![normalized_name],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        Ok((id, false))
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_institutions (id, normalized_name, country, city) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, normalized_name, country, city],
        )?;
        Ok((id, true))
    }
}

/// Link an author (per-article) to an institution. Uses INSERT OR IGNORE to avoid duplicates.
pub fn insert_author_affiliation(
    conn: &Connection,
    article_author_id: &str,
    institution_id: &str,
) -> Result<(), AppError> {
    let (article_id, author_id): (String, String) = conn.query_row(
        "SELECT article_id, author_id FROM biblio_article_authors WHERE id = ?1",
        rusqlite::params![article_author_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO biblio_author_affiliations (id, article_id, author_id, institution_id) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), article_id, author_id, institution_id],
    )?;
    Ok(())
}

/// Get all institutions linked to an author (across all their articles).
pub fn get_institutions_by_author(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<BiblioInstitution>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.normalized_name, i.country, i.city, i.created_at \
         FROM biblio_institutions i \
         JOIN biblio_author_affiliations baa ON baa.institution_id = i.id \
         LEFT JOIN articles a ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 \
         GROUP BY i.id, i.normalized_name, i.country, i.city, i.created_at \
         ORDER BY MAX(COALESCE(a.publication_year, 0)) DESC, i.normalized_name ASC",
    )?;
    let institutions = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(BiblioInstitution {
                id: row.get(0)?,
                normalized_name: row.get(1)?,
                country: row.get(2)?,
                city: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(institutions)
}

/// Count article-author rows that have a raw_affiliation but no linked institution.
/// These are candidates for LLM-based affiliation normalization.
pub fn count_unmatched_affiliations(conn: &Connection) -> Result<i32, AppError> {
    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_article_authors baa \
             LEFT JOIN biblio_author_affiliations baf ON baf.article_id = baa.article_id AND baf.author_id = baa.author_id \
             WHERE baa.raw_affiliation IS NOT NULL AND baa.raw_affiliation != '' \
             AND baf.id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}
