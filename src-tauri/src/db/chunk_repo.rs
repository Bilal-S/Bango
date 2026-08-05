//! Repository for the `article_chunks` table (Tier 3 chunk storage).
//!
//! Created by migration v003. Populated at attach time (NOT by the wiki pipeline);
//! consumed by `screening::chunk_retrieval`. Distinct from `wiki::fts` (which indexes
//! wiki pages post-ingest): this holds article-level chunks pre-ingest so screening
//! never triggers PDF parsing.

use crate::error::AppError;
use crate::utils::chunking::Chunk;
use rusqlite::{params, Connection};

/// Replace all chunks for one article. Idempotent: deletes existing rows first, then inserts.
pub fn replace_chunks_for_article(
    conn: &Connection,
    article_id: &str,
    chunks: &[Chunk],
) -> Result<usize, AppError> {
    conn.execute("DELETE FROM article_chunks WHERE article_id = ?1", params![article_id])?;
    let mut count = 0usize;
    for chunk in chunks {
        let rows = conn.execute(
            "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count)
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

/// Delete all chunks for an article. Called by `delete_full_text` (not an article
/// delete, so `ON DELETE CASCADE` does not fire — explicit clear needed).
pub fn delete_chunks_for_article(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM article_chunks WHERE article_id = ?1", params![article_id])?;
    Ok(())
}

/// Read all chunks for an article, ordered by `chunk_index` (contiguous 0..n).
pub fn list_chunks_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<Chunk>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT chunk_index, section, content, word_count FROM article_chunks \
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

/// Count the chunks stored for an article (0 if none / not attached).
pub fn count_chunks_for_article(conn: &Connection, article_id: &str) -> Result<i64, AppError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM article_chunks WHERE article_id = ?1",
            params![article_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}

/// Articles with `has_full_text = 1` but zero rows in `article_chunks`.
/// Used by the screening-start guard (`force=false`). Excludes articles whose
/// `full_text` is NULL/empty (soft-fallback attaches that can never produce chunks),
/// so the guard does not retry the same invalid PDF on every screening run.
pub fn get_articles_with_full_text_missing_chunks(
    conn: &Connection,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.id FROM articles a
         WHERE a.has_full_text = 1
           AND a.full_text IS NOT NULL
           AND a.full_text <> ''
           AND NOT EXISTS (SELECT 1 FROM article_chunks c WHERE c.article_id = a.id)",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Every article id with `has_full_text = 1`, regardless of chunk state.
/// Used by the "Rebuild text chunks" button (`force=true`) so corrupted/partial/
/// outdated chunk sets are repaired, not just empty ones.
pub fn get_articles_with_full_text(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT id FROM articles WHERE has_full_text = 1")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Count articles with full text attached (used by the Settings UI to label
/// the "Rebuild text chunks" button).
pub fn count_articles_with_full_text(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM articles WHERE has_full_text = 1", [], |row| {
            row.get(0)
        })?;
    Ok(count)
}
