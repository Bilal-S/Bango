//! Repository for the `article_embeddings` table (v007 migration).
//!
//! Stores per-article, per-chunk embedding vectors for semantic search.
//! Title+abstract row uses sentinel `chunk_index = -1`; per-chunk rows use
//! `article_chunks.chunk_index` (>= 0). Regenerable derived artifact: excluded
//! from backups, cleared on `reset_project`, cascade-deleted with articles.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// One stored embedding row. `chunk_index == TITLE_ABSTRACT_CHUNK_INDEX (-1)` is the
/// title+abstract row; `>= 0` is a per-chunk row.
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

/// Write shape for [`insert_embedding`]. `embedding.len() as i32` MUST equal `dimensions`.
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

/// Insert (or replace) one embedding row. Idempotent on `(article_id, chunk_index)`
/// via `INSERT OR REPLACE`. Pass `TITLE_ABSTRACT_CHUNK_INDEX` (-1) for the
/// title+abstract row.
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

/// Delete all embedding rows for an article. Called on chunk rebuild or hard-delete
/// (though `ON DELETE CASCADE` already handles the latter).
pub fn delete_embeddings_for_article(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM article_embeddings WHERE article_id = ?1", params![article_id])?;
    Ok(())
}

/// Look up `input_hash` for `(article_id, chunk_index)` — used by the director to
/// detect staleness without loading the full embedding blob.
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

/// `(chunk_index, input_hash)` rows currently stored for an article.
/// `chunk_index == -1` is the title+abstract row.
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

/// `(chunk_index, input_hash, model_name)` for an article. Richer than
/// [`list_hashes_for_article`]: detects model mismatches so switching models
/// (e.g. `text-embedding-3-small` → `text-embedding-3-large`) marks all rows stale,
/// avoiding a silent zero-results bug from the dimensions filter.
pub fn list_hashes_and_model_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<(i32, String, String)>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT chunk_index, input_hash, model_name FROM article_embeddings WHERE article_id = ?1",
    )?;
    let rows = stmt.query_map(params![article_id], |row| {
        let ci: i64 = row.get(0)?;
        let hash: String = row.get(1)?;
        let model: String = row.get(2).unwrap_or_default();
        Ok((ci as i32, hash, model))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Count all embedding rows. Gates semantic search: empty table → LIKE fallback.
pub fn count_embeddings(conn: &Connection) -> Result<i64, AppError> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM article_embeddings", [], |row| row.get(0))?;
    Ok(count)
}

/// Distinct `model_name` values across all embedding rows. Used by Citation Finder's
/// model-mismatch detection (`get_embedding_model_mismatch`) so the frontend warns
/// before searching with a different model (which would silently exclude all rows
/// via the dimensions filter). Empty vec when table is empty.
pub fn list_distinct_model_names(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT model_name FROM article_embeddings WHERE model_name IS NOT NULL AND model_name != ''",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Delete ALL embedding rows. Used by the "regenerate on model switch" path:
/// a clean delete + re-embed is clearer than `force=true` because the latter
/// leaves orphans when chunk counts shrink.
pub fn delete_all_embeddings(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM article_embeddings", [])?;
    Ok(())
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

/// Load embedding rows matching the given dimensionality. Optionally scopes to
/// articles in `status_filter` (JOIN `articles`). Rows with mismatched `dimensions`
/// are excluded so a model switch doesn't mix incompatible vectors. `status_filter`
/// empty = no status filter (all articles).
pub fn list_for_recall(
    conn: &Connection,
    dimensions: i32,
    status_filter: &[String],
) -> Result<Vec<EmbeddingRow>, AppError> {
    let mut out = Vec::new();
    if status_filter.is_empty() {
        // No status filter: scan article_embeddings directly (no JOIN needed).
        let sql = "SELECT article_id, chunk_index, embedding, dimensions, \
             input_hash, model_name, provider FROM article_embeddings WHERE dimensions = ?1";
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![dimensions], row_to_embedding)?;
        for row in rows {
            out.push(row?);
        }
    } else {
        // Build `status IN (?, ?, ?)` with one placeholder per status. No
        // string interpolation - every status is bound as a parameter, so
        // arbitrary status strings cannot inject SQL.
        let placeholders: Vec<&str> = (0..status_filter.len()).map(|_| "?").collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT e.article_id, e.chunk_index, e.embedding, e.dimensions, \
             e.input_hash, e.model_name, e.provider \
             FROM article_embeddings e JOIN articles a ON a.id = e.article_id \
             WHERE e.dimensions = ?1 AND a.status IN ({in_clause})"
        );
        let mut stmt = conn.prepare(&sql)?;
        // Bind dimensions first (?1), then each status in order (?2..=?N).
        // rusqlite's `params_from_iter` handles the variable-length tail.
        let dim_string_pairs: Vec<&dyn rusqlite::ToSql> =
            std::iter::once(&dimensions as &dyn rusqlite::ToSql)
                .chain(status_filter.iter().map(|s| s as &dyn rusqlite::ToSql))
                .collect();
        let rows =
            stmt.query_map(rusqlite::params_from_iter(dim_string_pairs.iter()), row_to_embedding)?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Row mapper for `list_for_recall`. Decodes the little-endian BLOB; a corrupt blob
/// falls back to an empty vec (zero-signal) rather than failing the whole query.
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
