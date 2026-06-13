use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::biblio::{BiblioArticleAuthor, BiblioAuthor, YearCount};

/// Upsert a normalized author. Returns the author ID.
pub fn upsert_author(
    conn: &Connection,
    normalized_name: &str,
    display_name: &str,
) -> Result<String, AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_authors WHERE normalized_name = ?1",
            rusqlite::params![normalized_name],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        conn.execute(
            "UPDATE biblio_authors SET article_count = article_count + 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(id)
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_authors (id, normalized_name, display_name, first_author_count, article_count) \
             VALUES (?1, ?2, ?3, 0, 1)",
            rusqlite::params![id, normalized_name, display_name],
        )?;
        Ok(id)
    }
}

/// Link an article to an author at a specific order position.
pub fn link_article_author(
    conn: &Connection,
    article_id: &str,
    author_id: &str,
    author_order: i32,
    raw_name: Option<&str>,
    raw_affiliation: Option<&str>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO biblio_article_authors (id, article_id, author_id, author_order, raw_name, raw_affiliation) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, article_id, author_id, author_order, raw_name, raw_affiliation],
    )?;

    // If first author, update first_author_count
    if author_order == 0 {
        conn.execute(
            "UPDATE biblio_authors SET first_author_count = first_author_count + 1 WHERE id = ?1",
            rusqlite::params![author_id],
        )?;
    }
    Ok(())
}

/// Get all authors linked to an article.
pub fn get_authors_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<BiblioArticleAuthor>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, article_id, author_id, author_order, raw_name, raw_affiliation \
         FROM biblio_article_authors \
         WHERE article_id = ?1 \
         ORDER BY author_order",
    )?;
    let links = stmt
        .query_map(rusqlite::params![article_id], |row| {
            Ok(BiblioArticleAuthor {
                id: row.get(0)?,
                article_id: row.get(1)?,
                author_id: row.get(2)?,
                author_order: row.get(3)?,
                raw_name: row.get(4)?,
                raw_affiliation: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(links)
}

/// Compute author metrics: total_citations, avg_year, estimated_h_index.
/// Updates all rows in biblio_authors.
pub fn compute_author_metrics(conn: &Connection) -> Result<(), AppError> {
    // Get all author IDs
    let mut stmt = conn.prepare("SELECT id FROM biblio_authors")?;
    let author_ids: Vec<String> =
        stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;

    for author_id in &author_ids {
        // Total citations: sum of num_cited for all articles by this author
        let total_citations: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(a.num_cited), 0) \
                 FROM articles a \
                 JOIN biblio_article_authors baa ON baa.article_id = a.id \
                 WHERE baa.author_id = ?1",
                rusqlite::params![author_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Average publication year
        let avg_year: Option<f64> = conn
            .query_row(
                "SELECT AVG(a.publication_year) \
                 FROM articles a \
                 JOIN biblio_article_authors baa ON baa.article_id = a.id \
                 WHERE baa.author_id = ?1 AND a.publication_year IS NOT NULL",
                rusqlite::params![author_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        // Programmatic h-index (largest h where h papers have >= h citations each)
        let h_index = compute_h_index(conn, author_id)?;

        conn.execute(
            "UPDATE biblio_authors SET total_citations = ?1, avg_year = ?2, estimated_h_index = ?3 WHERE id = ?4",
            rusqlite::params![total_citations, avg_year, h_index, author_id],
        )?;
    }

    Ok(())
}

/// Compute h-index for a single author programmatically.
pub fn compute_h_index(conn: &Connection, author_id: &str) -> Result<i32, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.num_cited FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 AND a.num_cited IS NOT NULL \
         ORDER BY a.num_cited DESC",
    )?;
    let citations: Vec<i32> = stmt
        .query_map(rusqlite::params![author_id], |row| row.get(0))?
        .collect::<Result<Vec<i32>, _>>()?;

    let h = citations
        .iter()
        .enumerate()
        .take_while(|&(i, &cites)| cites >= (i + 1) as i32)
        .count() as i32;

    Ok(h)
}

/// Get all normalized authors.
pub fn get_all_authors(conn: &Connection) -> Result<Vec<BiblioAuthor>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_name, display_name, first_author_count, article_count, \
                total_citations, avg_year, estimated_h_index, created_at \
         FROM biblio_authors ORDER BY article_count DESC",
    )?;
    let authors = stmt
        .query_map([], |row| {
            Ok(BiblioAuthor {
                id: row.get(0)?,
                normalized_name: row.get(1)?,
                display_name: row.get(2)?,
                first_author_count: row.get(3)?,
                article_count: row.get(4)?,
                total_citations: row.get(5)?,
                avg_year: row.get(6)?,
                estimated_h_index: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(authors)
}

/// Get publications-by-year for a single author.
pub fn get_author_pubs_by_year(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<YearCount>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.publication_year, COUNT(*) AS cnt \
         FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 AND a.publication_year IS NOT NULL \
         GROUP BY a.publication_year \
         ORDER BY a.publication_year ASC",
    )?;
    let rows: Vec<YearCount> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(YearCount { year: row.get(0)?, count: row.get(1)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
