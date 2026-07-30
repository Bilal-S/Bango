//! Repository for the `article_embeddings` table.
//!
//! Stores per-article, per-chunk embedding vectors for semantic search. The
//! table is created by migration `v007_audit_clear_and_embeddings`. The
//! title+abstract row uses the sentinel `chunk_index = -1`; per-chunk rows use
//! the matching `article_chunks.chunk_index` (`>= 0`).
//!
//! Regenerable derived artifact: excluded from project backups, cleared on
//! `reset_project` (via `rebuild::DROP_TABLES`), and `ON DELETE CASCADE`
//! removes rows when an article is hard-deleted.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// One stored embedding row (read shape). `chunk_index ==
/// TITLE_ABSTRACT_CHUNK_INDEX (-1)` is the title+abstract row; `>= 0` is a
/// per-chunk row.
#[derive(Debug, Clone)]
pub struct EmbeddingRow {
    pub article_id: String,
    pub chunk_index: i32,
    /// The embedding vector (decoded from the little-endian BLOB).
    pub embedding: Vec<f32>,
    pub dimensions: i32,
    pub input_hash: String,
    pub model_name: String,
    pub provider: String,
}

/// Write shape for [`insert_embedding`]. Grouping the fields into a struct
/// keeps the function arity under the clippy `too_many_arguments` threshold
/// and makes call sites self-documenting (`row.article_id` vs. positional
/// `&str`). `embedding.len() as i32` MUST equal `dimensions`.
#[derive(Debug, Clone)]
pub struct NewEmbeddingRow<'a> {
    pub article_id: &'a str,
    /// `TITLE_ABSTRACT_CHUNK_INDEX` (-1) for the title+abstract row, or the
    /// matching `article_chunks.chunk_index` (>= 0) for a chunk row.
    pub chunk_index: i32,
    pub embedding: &'a [f32],
    pub dimensions: i32,
    pub input_hash: &'a str,
    pub model_name: &'a str,
    pub provider: &'a str,
    pub generated_at: i64,
}

/// Insert (or replace) one embedding row. Idempotent on the composite primary
/// key `(article_id, chunk_index)` via `INSERT OR REPLACE`.
///
/// Pass `TITLE_ABSTRACT_CHUNK_INDEX` (-1) for the title+abstract row, or the
/// matching `article_chunks.chunk_index` (>= 0) for a chunk row. The
/// `embedding` slice is serialized to a little-endian f32 byte stream by
/// `embedding::text::serialize_embedding`.
pub fn insert_embedding(conn: &Connection, row: &NewEmbeddingRow<'_>) -> Result<(), AppError> {
    let bytes = crate::embedding::text::serialize_embedding(row.embedding);
    conn.execute(
        "INSERT OR REPLACE INTO article_embeddings \
         (article_id, chunk_index, embedding, dimensions, input_hash, model_name, provider, generated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            row.article_id,
            row.chunk_index,
            bytes,
            row.dimensions,
            row.input_hash,
            row.model_name,
            row.provider,
            row.generated_at,
        ],
    )?;
    Ok(())
}

/// Delete all embedding rows for an article. Called when an article's chunks
/// are rebuilt (so the embeddings are regenerated against the new chunk set)
/// or when an article is hard-deleted (though `ON DELETE CASCADE` already
/// handles the latter).
pub fn delete_embeddings_for_article(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM article_embeddings WHERE article_id = ?1", params![article_id])?;
    Ok(())
}

/// Look up a single stored row by `(article_id, chunk_index)` and return its
/// `input_hash` if present. Used by the director to detect staleness without
/// loading the full embedding blob.
///
/// Pass `TITLE_ABSTRACT_CHUNK_INDEX` for the title+abstract row.
pub fn get_input_hash(
    conn: &Connection,
    article_id: &str,
    chunk_index: i32,
) -> Result<Option<String>, AppError> {
    let row: Option<String> = conn
        .query_row(
            "SELECT input_hash FROM article_embeddings WHERE article_id = ?1 AND chunk_index = ?2",
            params![article_id, chunk_index],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    Ok(row.filter(|s| !s.is_empty()))
}

/// The set of `(chunk_index, input_hash)` rows currently stored for an
/// article. The director compares these against the freshly-computed expected
/// hashes to decide which rows need (re)embedding. `chunk_index == -1` is the
/// title+abstract row.
pub fn list_hashes_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<(i32, String)>, AppError> {
    let mut stmt = conn
        .prepare("SELECT chunk_index, input_hash FROM article_embeddings WHERE article_id = ?1")?;
    let rows = stmt.query_map(params![article_id], |row| {
        let ci: i64 = row.get(0)?;
        let hash: String = row.get(1)?;
        Ok((ci as i32, hash))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Count all embedding rows in the table (across all articles). Used by the
/// search-time gate to decide whether semantic search is even possible
/// (empty table => LIKE fallback).
pub fn count_embeddings(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM article_embeddings", [], |row| row.get(0))?;
    Ok(count)
}

/// Count embedding rows for a single article (0 if none).
pub fn count_embeddings_for_article(conn: &Connection, article_id: &str) -> Result<i64, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article_embeddings WHERE article_id = ?1",
        params![article_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Load all embedding rows matching the given dimensionality and an optional
/// article status filter (JOIN `articles`). Used by recall to score the
/// candidate pool.
///
/// Rows whose `dimensions` differ from `dimensions` are excluded so a provider
/// / model switch (which changes the dimension count) does not mix incompatible
/// vectors into the same cosine scoring pass. The user rebuilds to clear the
/// stale-dimension rows.
///
/// The `status_filter` (e.g. `Some("included")`) scopes the candidate pool so
/// recall is bounded by the active corpus rather than the whole DB.
pub fn list_for_recall(
    conn: &Connection,
    dimensions: i32,
    status_filter: Option<&str>,
) -> Result<Vec<EmbeddingRow>, AppError> {
    let sql = match status_filter {
        Some(_) => {
            "SELECT e.article_id, e.chunk_index, e.embedding, e.dimensions, \
             e.input_hash, e.model_name, e.provider \
             FROM article_embeddings e JOIN articles a ON a.id = e.article_id \
             WHERE e.dimensions = ?1 AND a.status = ?2"
        }
        None => {
            "SELECT article_id, chunk_index, embedding, dimensions, input_hash, \
             model_name, provider FROM article_embeddings WHERE dimensions = ?1"
        }
    };
    let mut stmt = conn.prepare(sql)?;
    // The status value is bound as ?2 in the filtered SQL above. Both arms
    // return rows of the same shape, so a single mapper covers them.
    let rows = match status_filter {
        Some(status) => stmt.query_map(params![dimensions, status], row_to_embedding)?,
        None => stmt.query_map(params![dimensions], row_to_embedding)?,
    };
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Row mapper shared by `list_for_recall`. Decodes the little-endian BLOB into
/// `Vec<f32>` via `embedding::text::deserialize_embedding`; a corrupt/short
/// blob is skipped (filtered out) rather than failing the whole query, so one
/// bad row never breaks recall.
fn row_to_embedding(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmbeddingRow> {
    let article_id: String = row.get(0)?;
    let ci: i64 = row.get(1)?;
    let blob: Vec<u8> = row.get(2)?;
    let dimensions: i32 = row.get(3)?;
    let input_hash: String = row.get(4)?;
    let model_name: String = row.get(5)?;
    let provider: String = row.get(6)?;
    // decode; if it fails, this row is corrupt. We map to an empty vec so the
    // caller's cosine scoring treats it as zero-signal (similarity 0.0). The
    // dimension check above guarantees the blob length matches; the
    // deserialize is defense-in-depth.
    let embedding =
        crate::embedding::text::deserialize_embedding(&blob, dimensions).unwrap_or_default();
    Ok(EmbeddingRow {
        article_id,
        chunk_index: ci as i32,
        embedding,
        dimensions,
        input_hash,
        model_name,
        provider,
    })
}

// Re-export the sentinel for callers that reach the title+abstract row through
// the repo. The canonical home is `embedding::text`, but the repo is a common
// import site so re-exporting avoids an extra `use` in every caller.
pub use crate::embedding::text::TITLE_ABSTRACT_CHUNK_INDEX;
