//! Repository for the `article_original_content` and `article_original_chunks`
//! tables (Plan-A translation originals archive).
//!
//! Populated once at translation time, before the working `articles` row is
//! rewritten to English. `source_language` captures the `articles.language`
//! value at translation time. After translation, re-chunking produces new
//! indices in `article_chunks`; the chunk coordinate spaces must not be
//! compared or joined directly.

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::utils::chunking::Chunk;

/// A row from `article_original_content`.
#[derive(Debug, Clone, Default)]
pub struct OriginalContent {
    pub article_id: String,
    pub original_title: Option<String>,
    pub original_abstract_text: Option<String>,
    pub original_full_text: Option<String>,
    pub source_language: Option<String>,
    pub stored_at: String,
}

/// Persist the original-language content for an article.
///
/// `INSERT OR REPLACE` so the row is (re)written each time a translation runs.
/// Phase 2 callers pass `original_full_text = None`; Phase 3 (full-text
/// translation) populates it.
pub fn insert_original_content(
    conn: &Connection,
    article_id: &str,
    original_title: Option<&str>,
    original_abstract_text: Option<&str>,
    original_full_text: Option<&str>,
    source_language: Option<&str>,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO article_original_content \
             (article_id, original_title, original_abstract_text, original_full_text, source_language, stored_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now')) \
         ON CONFLICT(article_id) DO UPDATE SET \
             original_title = excluded.original_title, \
             original_abstract_text = excluded.original_abstract_text, \
             original_full_text = excluded.original_full_text, \
             source_language = excluded.source_language, \
             stored_at = datetime('now')",
        params![article_id, original_title, original_abstract_text, original_full_text, source_language],
    )?;
    Ok(())
}

/// Read the original-language content row for an article, if any.
pub fn get_original_content(
    conn: &Connection,
    article_id: &str,
) -> Result<Option<OriginalContent>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT article_id, original_title, original_abstract_text, original_full_text, \
         source_language, stored_at FROM article_original_content WHERE article_id = ?1",
    )?;
    let mut rows = stmt.query_map([article_id], |row| {
        Ok(OriginalContent {
            article_id: row.get(0)?,
            original_title: row.get(1)?,
            original_abstract_text: row.get(2)?,
            original_full_text: row.get(3)?,
            source_language: row.get(4)?,
            stored_at: row.get(5)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// Replace all original chunks for one article with the given set. Idempotent:
/// deletes existing rows first, then inserts the new set. Mirrors
/// `chunk_repo::replace_chunks_for_article`.
pub fn replace_original_chunks(
    conn: &Connection,
    article_id: &str,
    chunks: &[Chunk],
) -> Result<usize, AppError> {
    conn.execute("DELETE FROM article_original_chunks WHERE article_id = ?1", params![article_id])?;
    let mut count = 0usize;
    for chunk in chunks {
        let rows = conn.execute(
            "INSERT INTO article_original_chunks \
                 (article_id, chunk_index, section, content, word_count) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                article_id,
                chunk.chunk_index as i64,
                chunk.section,
                chunk.text,
                chunk.word_count as i64,
            ],
        )?;
        count += rows;
    }
    Ok(count)
}

/// Read all original chunks for an article, ordered by `chunk_index`.
pub fn list_original_chunks(conn: &Connection, article_id: &str) -> Result<Vec<Chunk>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT chunk_index, section, content, word_count FROM article_original_chunks \
         WHERE article_id = ?1 ORDER BY chunk_index ASC",
    )?;
    let rows = stmt.query_map(params![article_id], |row| {
        Ok(Chunk {
            chunk_index: row.get::<_, i64>(0)? as usize,
            section: row.get::<_, Option<String>>(1)?,
            text: row.get::<_, String>(2)?,
            word_count: row.get::<_, i64>(3)? as usize,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
