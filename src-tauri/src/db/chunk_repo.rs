//! Repository for the `article_chunks` table (Tier 3 chunk storage).
//!
//! The `article_chunks` table is created by migration `v003_fts_sections.rs`
//! alongside the FTS5 drop, but it is NOT populated by the wiki pipeline. It
//! holds the per-article chunks that Tier 3 screening retrieves as
//! "Supporting Evidence from Full Text". It is filled at attach time by
//! `commands::full_text::attach_full_text` (via `utils::sections::extract_sections`
//! + `utils::chunking::chunk_sections`) and cleared on detach.
//!
//! Distinct from `wiki::fts` (which indexes wiki *pages* post-ingest): this
//! table holds article-level chunks at attach time, pre-ingest, so screening
//! never triggers PDF parsing.

use crate::error::AppError;
use crate::utils::chunking::Chunk;
use rusqlite::{params, Connection};

/// Replace all chunks for one article with the given set. Idempotent + re-attach
/// safe: deletes existing rows for the article first, then inserts the new set.
/// Callers pass the output of
/// `chunk_sections(extract_sections(path), DEFAULT_CHUNK_WORDS)`.
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

/// Delete all chunks for an article. Called by `delete_full_text` (the article
/// row isn't deleted on full-text removal - only `has_full_text` flips - so the
/// `ON DELETE CASCADE` foreign key does not fire and an explicit clear is needed).
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

/// Article rows that have `has_full_text = 1` but zero rows in `article_chunks`.
/// Used by the `ensure_chunks_for_full_text_articles` guard (runs at screening
/// start with `force=false`) so previously-attached PDFs without chunks are
/// backfilled transparently.
pub fn get_articles_with_full_text_missing_chunks(
    conn: &Connection,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.id FROM articles a
         WHERE a.has_full_text = 1
           AND NOT EXISTS (SELECT 1 FROM article_chunks c WHERE c.article_id = a.id)",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Every article id with `has_full_text = 1`, regardless of whether it already
/// has chunks. Used by `ensure_chunks_for_full_text_articles(force=true)` so the
/// Settings "Rebuild text chunks" button repairs corrupted/partial/outdated
/// chunk sets, not just empty ones. `replace_chunks_for_article` deletes-then-
/// inserts per article, so re-chunking is idempotent.
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
