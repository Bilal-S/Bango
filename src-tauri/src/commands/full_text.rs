use std::path::{Path, PathBuf};

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::utils::chunking::{chunk_sections, DEFAULT_CHUNK_WORDS};
use crate::utils::pdf_extract;
use crate::utils::sections::{extract_captions, extract_sections};

/// Extract sections from an attached full-text file and store them as chunks
/// in `article_chunks` (Tier 3 screening evidence). Pure-CPU (no LLM).
///
/// Called by `attach_full_text` after the full-text row is written, and by
/// `rebuild_article_chunks` for the one-shot backfill of already-attached PDFs.
/// Non-fatal: callers log the error and continue (the full text still attaches;
/// screening falls back to abstract-only for this article).
pub fn populate_chunks_for_attached_text(
    conn: &rusqlite::Connection,
    article_id: &str,
    source_path: &Path,
) -> Result<usize, AppError> {
    let sections = extract_sections(source_path).map_err(AppError::Import)?;
    let chunks = chunk_sections(&sections, DEFAULT_CHUNK_WORDS);
    chunk_repo::replace_chunks_for_article(conn, article_id, &chunks)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullTextAttachResult {
    pub success: bool,
    pub message: String,
    pub word_count: usize,
}

/// Resolve the fulltext storage directory (`{storage_root}/fulltext/`).
///
/// Delegates to [`app_settings_repo::get_fulltext_dir`], which derives the
/// root via [`app_settings_repo::get_storage_root`] and ensures both the root
/// and the `fulltext/` subdir exist.
pub fn compute_storage_dir(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let fulltext = app_settings_repo::get_fulltext_dir(conn)?;
    Ok(PathBuf::from(fulltext))
}

/// Attach a full-text file (PDF or TXT) to an article.
/// Extracts text content, stores in DB, and copies file to storage directory.
///
/// This is the reusable core extracted from the Tauri command so the batch
/// import runner can attach files per-article without re-acquiring the
/// `DbState` mutex (the caller already holds a `&Connection`).
///
/// # Arguments
/// * `conn` - A locked SQLite connection.
/// * `article_id` - The article to attach the full text to.
/// * `source_path` - Path to the source PDF/TXT file on disk.
/// * `storage_dir` - The fulltext storage directory (from `compute_storage_dir`).
pub fn attach_full_text_inner(
    conn: &rusqlite::Connection,
    article_id: &str,
    source_path: &Path,
    storage_dir: &Path,
) -> Result<FullTextAttachResult, AppError> {
    if !source_path.exists() {
        return Err(AppError::Import(format!("File not found: {}", source_path.display())));
    }

    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Extract text based on file type
    let full_text = match extension.as_str() {
        "pdf" => pdf_extract::extract_pdf_text(source_path).map_err(AppError::Import),
        "txt" => {
            let content = std::fs::read_to_string(source_path)?;
            Ok(pdf_extract::extract_txt_text(&content))
        }
        other => Err(AppError::Import(format!(
            "Unsupported file type: .{other}. Only .pdf and .txt files are supported."
        ))),
    }?;

    let word_count = full_text.split_whitespace().count();

    // Detect figure/table captions in the extracted text so the persisted
    // `has_figures_or_tables` flag matches the generation path's own
    // precondition (`generate_figure_descriptions` validates via the same
    // `extract_captions` call and errors with "No figure/table captions
    // detected" when the result is empty). Using the same detector here keeps
    // the frontend button gate DRY with the backend generation precondition.
    let has_figures_or_tables = !extract_captions(&full_text).is_empty();

    // Build destination filename: {original_stem}_{article_id}.{ext}
    let original_name = source_path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let dest_filename = if ext.is_empty() {
        format!("{stem}_{article_id}")
    } else {
        format!("{stem}_{article_id}.{ext}")
    };
    let dest_path = storage_dir.join(&dest_filename);

    // Copy file to storage directory
    std::fs::copy(source_path, &dest_path)
        .map_err(|e| AppError::Import(format!("Failed to copy file to storage: {e}")))?;

    // Update database
    article_repo::update_full_text(
        conn,
        article_id,
        &full_text,
        &dest_filename,
        has_figures_or_tables,
    )?;

    // Tier 3: populate `article_chunks` so enhanced/two-stage screening can
    // retrieve "Supporting Evidence from Full Text" without re-parsing the PDF.
    if let Err(e) = populate_chunks_for_attached_text(conn, article_id, source_path) {
        let _ = crate::db::audit_repo::log_error(
            conn,
            &format!("Chunk extraction failed for article {article_id}: {e}"),
        );
    }

    app_settings_repo::mark_wiki_needs_refresh(conn);
    app_settings_repo::mark_biblio_needs_refresh(conn);

    crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "import",
        None,
        None,
        Some(&format!("Full text attached: {original_name}")),
        "user",
    )?;

    Ok(FullTextAttachResult {
        success: true,
        message: format!("Full text extracted ({word_count} words)"),
        word_count,
    })
}

/// Attach a full-text file (PDF or TXT) to an article.
/// Extracts text content, stores in DB, and copies file to storage directory.
#[tauri::command]
pub fn attach_full_text(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    article_id: String,
    file_path: String,
) -> Result<FullTextAttachResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let storage_dir = compute_storage_dir(&conn)?;
    let source_path = PathBuf::from(&file_path);
    attach_full_text_inner(&conn, &article_id, &source_path, &storage_dir)
}

/// Delete the full-text attachment for an article.
/// Removes file from storage and clears DB references.
#[tauri::command]
pub fn delete_full_text(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<bool, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // Get the file name before clearing
    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    if let Some(ref name) = file_name {
        let storage_dir = compute_storage_dir(&conn)?;
        let file_path = storage_dir.join(name);

        // Delete the file
        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .map_err(|e| AppError::Import(format!("Failed to delete file: {e}")))?;
        }
    }

    // Clear DB references
    article_repo::clear_full_text(&conn, &article_id)?;

    // Tier 3: clear the article's chunks. The article row is NOT deleted on
    // full-text removal (only `has_full_text` flips), so the `ON DELETE
    // CASCADE` foreign key does not fire and an explicit clear is needed.
    if let Err(e) = chunk_repo::delete_chunks_for_article(&conn, &article_id) {
        let _ = crate::db::audit_repo::log_error(
            &conn,
            &format!("Failed to clear chunks for article {article_id}: {e}"),
        );
    }

    // Removing the full text downgrades the content source for the wiki ingest
    // (falls back to ai_summary or abstract). Mark stale so the next visit
    // re-ingests with the correct content.
    app_settings_repo::mark_wiki_needs_refresh(&conn);
    app_settings_repo::mark_biblio_needs_refresh(&conn);

    // Create audit entry
    crate::db::audit_repo::create_entry(
        &conn,
        &article_id,
        "import",
        None,
        None,
        Some("Full text attachment removed"),
        "user",
    )?;

    Ok(true)
}

/// Read the full-text content for an article from the database.
#[tauri::command]
pub fn read_full_text(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let article = article_repo::get_article_by_id(&conn, &article_id)?;
    Ok(article.full_text)
}

/// Read the bytes of a full-text attachment file.
/// Used by the frontend to create Blob URLs for inline PDF viewing.
#[tauri::command]
pub fn read_full_text_file_bytes(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<Vec<u8>>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    match file_name {
        Some(name) => {
            let storage_dir = compute_storage_dir(&conn)?;
            let file_path = storage_dir.join(&name);
            if !file_path.exists() {
                return Ok(None);
            }
            let bytes = std::fs::read(&file_path)?;
            Ok(Some(bytes))
        }
        None => Ok(None),
    }
}

/// Get the absolute file path for a full-text attachment.
/// Returns None if no file is attached.
#[tauri::command]
pub fn get_full_text_file_path(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Option<String>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let file_name = article_repo::get_full_text_file_name(&conn, &article_id)?;

    match file_name {
        Some(name) => {
            let storage_dir = compute_storage_dir(&conn)?;
            let file_path = storage_dir.join(&name);
            Ok(Some(file_path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Result of a one-shot chunk rebuild for already-attached full texts.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildChunksResult {
    pub success: bool,
    pub chunked: usize,
    pub failed: usize,
    pub skipped: usize,
    pub message: String,
}

/// Ensure articles with `has_full_text = 1` have rows in `article_chunks`.
///
/// Pure CPU (no LLM). Two modes:
/// - `force=false` (used at screening start): backfills only articles that
///   have zero chunks, so previously-attached PDFs without chunks are
///   transparently repaired and `enhanced`/`two_stage` modes never silently
///   fall back to abstract-only.
/// - `force=true` (used by the Settings "Rebuild text chunks" button):
///   re-chunks *every* article with full text, including ones that already
///   have chunks. Used to repair corrupted/partial/outdated chunk sets or
///   pick up chunking-algorithm updates. `replace_chunks_for_article`
///   deletes-then-inserts per article, so re-chunking is idempotent.
///
/// Errors are collected, not fatal: the returned `RebuildChunksResult`
/// reports how many succeeded/failed.
pub fn ensure_chunks_for_full_text_articles(
    conn: &rusqlite::Connection,
    force: bool,
) -> RebuildChunksResult {
    let storage_dir = match compute_storage_dir(conn) {
        Ok(d) => d,
        Err(e) => {
            return RebuildChunksResult {
                success: false,
                chunked: 0,
                failed: 0,
                skipped: 0,
                message: format!("Failed to resolve storage dir: {e}"),
            };
        }
    };

    let article_ids = if force {
        match chunk_repo::get_articles_with_full_text(conn) {
            Ok(ids) => ids,
            Err(e) => {
                return RebuildChunksResult {
                    success: false,
                    chunked: 0,
                    failed: 0,
                    skipped: 0,
                    message: format!("Failed to query articles with full text: {e}"),
                };
            }
        }
    } else {
        match chunk_repo::get_articles_with_full_text_missing_chunks(conn) {
            Ok(ids) => ids,
            Err(e) => {
                return RebuildChunksResult {
                    success: false,
                    chunked: 0,
                    failed: 0,
                    skipped: 0,
                    message: format!("Failed to query articles missing chunks: {e}"),
                };
            }
        }
    };

    let total = article_ids.len();
    let mut chunked = 0usize;
    let mut failed = 0usize;
    for article_id in &article_ids {
        // Resolve the on-disk attachment path for this article.
        let file_name = match article_repo::get_full_text_file_name(conn, article_id) {
            Ok(Some(name)) => name,
            _ => {
                failed += 1;
                continue;
            }
        };
        let path = storage_dir.join(&file_name);
        if !path.exists() {
            failed += 1;
            continue;
        }
        match populate_chunks_for_attached_text(conn, article_id, &path) {
            Ok(_) => chunked += 1,
            Err(e) => {
                let _ = crate::db::audit_repo::log_error(
                    conn,
                    &format!("ensure_chunks: failed for article {article_id}: {e}"),
                );
                failed += 1;
            }
        }
    }

    RebuildChunksResult {
        success: true,
        chunked,
        failed,
        skipped: total.saturating_sub(chunked + failed),
        message: format!("Chunked {chunked} article(s); {failed} failed"),
    }
}

/// One-shot rebuild: (re)build `article_chunks` for *every* article that has
/// full text attached (including ones that already have chunks). Wired to the
/// Settings -> Full-Text Storage "Rebuild text chunks" button so a user can
/// repair a corrupted/partial/outdated chunk set or pick up chunking-algorithm
/// updates. `force=true` selects `get_articles_with_full_text` (all rows) rather
/// than the missing-chunks-only query used by the screening-start guard.
#[tauri::command]
pub fn rebuild_article_chunks(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
) -> Result<RebuildChunksResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    Ok(ensure_chunks_for_full_text_articles(&conn, true))
}

/// Count articles with full text attached (drives the "Rebuild text chunks"
/// button label in Settings -> Full-Text Storage).
#[tauri::command]
pub fn count_articles_with_full_text(db_state: tauri::State<'_, DbState>) -> Result<i64, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    chunk_repo::count_articles_with_full_text(&conn)
}
