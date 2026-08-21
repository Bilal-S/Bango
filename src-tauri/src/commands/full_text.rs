use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::connection::DbState;
use crate::db::embedding_repo;
use crate::embedding::director::EmbeddingScope;
use crate::embedding::runner::{
    generate_embeddings_inner, EmbeddingBatchSender, EmbeddingRunReport, HttpEmbeddingBatchSender,
};
use crate::error::AppError;
use crate::scraping::citation_chaser::clean_doi_filename;
use crate::utils::chunking::{chunk_sections, Chunk, DEFAULT_CHUNK_WORDS};
use crate::utils::pdf_extract;
use crate::utils::sections::{extract_captions, extract_sections};

/// Extract sections from an attached full-text file and store as chunks in
/// `article_chunks` (Tier 3 screening evidence). Pure CPU (no LLM).
/// Non-fatal: callers log the error and continue.
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
    /// `true` when the file attached but text extraction failed (empty
    /// `full_text` persisted). Callers use this to skip downstream
    /// text-dependent work (e.g. translation enqueue) that would otherwise
    /// operate on empty content.
    pub extraction_failed: bool,
}

/// Resolve and ensure the fulltext storage directory
/// (`{storage_root}/fulltext/`).
pub fn compute_storage_dir(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let fulltext = app_settings_repo::get_fulltext_dir(conn)?;
    Ok(PathBuf::from(fulltext))
}

/// Compute destination filename for a full-text attachment.
///
/// - DOI present → `{clean_doi_filename(doi)}.{ext}` (matches batch-import +
///   Citation Chaser naming).
/// - No DOI / empty → `{stem}_{article_id}.{ext}` (UUID disambiguates).
///
/// Pure `#[must_use]`; unit-testable in isolation.
#[must_use]
pub fn compute_dest_filename(
    article_doi: Option<&str>,
    source_stem: &str,
    ext: &str,
    article_id: &str,
) -> String {
    let clean = article_doi.map(|d| d.trim()).filter(|d| !d.is_empty()).map(clean_doi_filename);
    match (clean, ext.is_empty()) {
        (Some(doi), true) => doi,
        (Some(doi), false) => format!("{doi}.{ext}"),
        (None, true) => format!("{source_stem}_{article_id}"),
        (None, false) => format!("{source_stem}_{article_id}.{ext}"),
    }
}

/// Place `source_path` in storage as `dest_path`. Hard-link when possible
/// (zero-copy), byte-copy fallback. No-op when source == dest (batch import).
fn place_file_in_storage(source_path: &Path, dest_path: &Path) -> Result<(), AppError> {
    /* Same-file short-circuit: if canonicalized paths match, the file is
    already in storage (batch import feeds files already named
    `{clean_doi}.pdf` inside `fulltext/`). No-op avoids self-copy errors. */
    let same = match (source_path.canonicalize(), dest_path.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        // If canonicalize fails (e.g. dest does not exist yet, which is the
        // common case), fall through to the link/copy path.
        _ => false,
    };
    if same {
        return Ok(());
    }
    /* Prefer hard link (zero-copy, both names same inode). Fall back to byte
    copy on cross-device / unsupported FS / already-exists. */
    match std::fs::hard_link(source_path, dest_path) {
        Ok(()) => Ok(()),
        Err(_) => std::fs::copy(source_path, dest_path)
            .map(|_| ())
            .map_err(|e| AppError::Import(format!("Failed to copy file to storage: {e}"))),
    }
}

/// CPU-bound extraction result, produced with NO DB lock held.
/// [`commit_full_text_to_db`] consumes this under a short lock burst for writes.
#[derive(Debug, Clone)]
pub struct ExtractedFullText {
    /// Extracted full text (empty on failure — soft fallback).
    pub full_text: String,
    /// Word count of `full_text` (cached for the DB-write phase).
    pub word_count: usize,
    /// `true` when figure/table captions were detected.
    pub has_figures_or_tables: bool,
    /// Pre-computed chunks (Tier 3 evidence). Empty on extraction failure.
    pub chunks: Vec<Chunk>,
    /// Destination filename (DOI-aware via `compute_dest_filename`).
    pub dest_filename: String,
    /// Original source file name (for audit messages).
    pub original_name: String,
    /// Extraction error message, when extraction failed (soft fallback). File
    /// still attaches with empty `full_text`.
    pub extraction_error: Option<String>,
}

/// Extract all CPU-bound data from a full-text source file with NO DB access.
/// Lock-free half of the split attach pipeline (PDF/TXT parse, caption
/// detection, section/chunk extraction, filename, file placement).
/// [`commit_full_text_to_db`] does the DB writes under a short lock burst.
///
/// Pure of DB state; safe inside `spawn_blocking`. Returns `Err` only for
/// unsupported extension or missing source file. Soft-fallback (extraction
/// failure → empty `full_text`) returns `Ok` with `extraction_error: Some(..)`.
pub fn extract_full_text_data(
    source_path: &Path,
    article_doi: Option<&str>,
    article_id: &str,
    storage_dir: &Path,
) -> Result<ExtractedFullText, AppError> {
    if !source_path.exists() {
        return Err(AppError::Import(format!("File not found: {}", source_path.display())));
    }

    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // Extract text. On failure, fall back to an empty string (soft fallback).
    let (full_text, extraction_error): (String, Option<String>) = match extension.as_str() {
        "pdf" => match pdf_extract::extract_pdf_text(source_path) {
            Ok(text) => (text, None),
            Err(e) => (String::new(), Some(format!("PDF text extraction failed: {e}"))),
        },
        "txt" => match std::fs::read_to_string(source_path) {
            Ok(content) => (pdf_extract::extract_txt_text(&content), None),
            Err(e) => (String::new(), Some(format!("Failed to read .txt file: {e}"))),
        },
        other => {
            return Err(AppError::Import(format!(
                "Unsupported file type: .{other}. Only .pdf and .txt files are supported."
            )));
        }
    };

    let word_count = full_text.split_whitespace().count();
    let has_figures_or_tables = !extract_captions(&full_text).is_empty();

    /* Section/chunk extraction for Tier 3 screening evidence. Second
    CPU-bound parse via `extract_sections`. On failure, chunks empty (non-fatal). */
    let chunks = match extract_sections(source_path) {
        Ok(sections) => chunk_sections(&sections, DEFAULT_CHUNK_WORDS),
        Err(_) => Vec::new(),
    };

    let original_name =
        source_path.file_name().and_then(|n| n.to_str()).unwrap_or("document").to_string();
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let dest_filename = compute_dest_filename(article_doi, stem, ext, article_id);
    let dest_path = storage_dir.join(&dest_filename);

    // Place the file in storage (hard-link / copy / same-file no-op).
    place_file_in_storage(source_path, &dest_path)?;

    Ok(ExtractedFullText {
        full_text,
        word_count,
        has_figures_or_tables,
        chunks,
        dest_filename,
        original_name,
        extraction_error,
    })
}

/// Commit an already-extracted full-text attachment to the database.
/// Lock-bound half of the split attach pipeline: writes only (update row,
/// chunk insert, audit entries, staleness flags). Fast (ms-scale) so the
/// caller holds the DB lock for just this portion.
pub fn commit_full_text_to_db(
    conn: &rusqlite::Connection,
    article_id: &str,
    extracted: &ExtractedFullText,
) -> Result<FullTextAttachResult, AppError> {
    let ExtractedFullText {
        full_text,
        word_count,
        has_figures_or_tables,
        chunks,
        dest_filename,
        original_name,
        extraction_error,
    } = extracted;

    // 1. Update the article row (full_text, has_full_text, file name, flag).
    article_repo::update_full_text(
        conn,
        article_id,
        full_text,
        dest_filename,
        *has_figures_or_tables,
    )?;

    // 2. Insert the pre-computed chunks (Tier 3). Non-fatal on failure.
    if !chunks.is_empty() {
        if let Err(e) = chunk_repo::replace_chunks_for_article(conn, article_id, chunks) {
            let _ = crate::db::audit_repo::log_error(
                conn,
                &format!("Chunk extraction failed for article {article_id}: {e}"),
            );
        }
    }

    // 3. Staleness flags.
    app_settings_repo::mark_wiki_needs_refresh(conn);
    app_settings_repo::mark_biblio_needs_refresh(conn);

    // 4. Extraction-failure audit entry (soft fallback).
    if let Some(ref msg) = extraction_error {
        let _ = crate::db::audit_repo::log_error(
            conn,
            &format!(
                "Full text attached to article {article_id} but {msg} (file: {original_name}). \
                 The file is stored; text-based features (screening evidence, AI summary, wiki) \
                 will be unavailable until a valid source file is provided."
            ),
        );
    }

    // 5. Success audit entry (non-fatal on write failure).
    let attach_detail = format!(
        "Full text attached: {original_name}{}",
        extraction_error.as_ref().map(|_| " (text extraction failed)").unwrap_or("")
    );
    if let Err(audit_e) = crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "import",
        None,
        None,
        Some(&attach_detail),
        "user",
    ) {
        let _ = crate::db::audit_repo::log_error(
            conn,
            &format!("Failed to write success audit for article {article_id}: {audit_e}"),
        );
    }

    let extraction_failed = extraction_error.is_some();
    Ok(FullTextAttachResult {
        success: true,
        message: if extraction_failed {
            "Full text attached (text extraction failed)".to_string()
        } else {
            format!("Full text extracted ({word_count} words)")
        },
        word_count: *word_count,
        extraction_failed,
    })
}

/// Split attach pipeline: CPU-bound extraction on `spawn_blocking` (no DB
/// lock), then short lock burst for DB writes. Callers already holding a
/// `&Connection` should call [`extract_full_text_data`] + [`commit_full_text_to_db`]
/// directly. This async helper is for batch-import Phase 1 (`&Mutex<Connection>`).
pub async fn attach_full_text_split(
    conn_mutex: &std::sync::Mutex<rusqlite::Connection>,
    article_id: &str,
    article_doi: Option<&str>,
    source_path: &Path,
    storage_dir: &Path,
) -> Result<FullTextAttachResult, AppError> {
    // Phase 1: lock-free CPU-bound extraction on the blocking pool.
    let source_owned = source_path.to_path_buf();
    let storage_owned = storage_dir.to_path_buf();
    let article_id_owned = article_id.to_string();
    let doi_owned = article_doi.map(str::to_string);
    let extracted = tokio::task::spawn_blocking(move || {
        extract_full_text_data(
            &source_owned,
            doi_owned.as_deref(),
            &article_id_owned,
            &storage_owned,
        )
    })
    .await
    .map_err(|e| AppError::Import(format!("Extraction task panicked: {e}")))??;

    // Phase 2: short DB lock burst for the writes only.
    let conn = crate::db::connection::lock_conn(conn_mutex)?;
    commit_full_text_to_db(&conn, article_id, &extracted)
}

/// Attach a full-text file (PDF/TXT) to an article (monolithic path).
///
/// Legacy single-call API retained for the manual `attach_full_text` Tauri
/// command + OpenAlex import. Batch-import Phase 1 uses the split
/// [`attach_full_text_split`] pipeline instead.
///
/// # Destination filename contract
///
/// - DOI present → `{clean_doi_filename(doi)}.{ext}` (matches batch-import
///   convention).
/// - No DOI → `{stem}_{article_id}.{ext}` (UUID disambiguates).
/// - Source already at destination (same canonical path) → no copy/link.
///
/// # Arguments
/// * `article_doi` - When `Some`, drives DOI-aware filename. Callers with the
///   DOI in hand (batch import, OpenAlex) pass it directly; the Tauri command
///   wrapper reads from DB.
pub fn attach_full_text_inner(
    conn: &rusqlite::Connection,
    article_id: &str,
    article_doi: Option<&str>,
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

    /* Extract text based on file type. On extraction failure, fall back to
    empty string (soft fallback: file attaches, `has_full_text` flips, error
    surfaces via audit). Only unsupported extension is a hard error. */
    let (full_text, extraction_error): (String, Option<String>) = match extension.as_str() {
        "pdf" => match pdf_extract::extract_pdf_text(source_path) {
            Ok(text) => (text, None),
            Err(e) => (String::new(), Some(format!("PDF text extraction failed: {e}"))),
        },
        "txt" => match std::fs::read_to_string(source_path) {
            Ok(content) => (pdf_extract::extract_txt_text(&content), None),
            Err(e) => (String::new(), Some(format!("Failed to read .txt file: {e}"))),
        },
        other => {
            return Err(AppError::Import(format!(
                "Unsupported file type: .{other}. Only .pdf and .txt files are supported."
            )));
        }
    };

    let word_count = full_text.split_whitespace().count();

    /* Detect figure/table captions so the persisted
    `has_figures_or_tables` flag matches the generation path's own
    precondition (`generate_figure_descriptions` uses the same
    `extract_captions` detector → DRY). On extraction failure `full_text` is
    empty, safely yielding false. */
    let has_figures_or_tables = !extract_captions(&full_text).is_empty();

    // Build destination filename via the DOI-aware helper (Concern 2).
    let original_name = source_path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let dest_filename = compute_dest_filename(article_doi, stem, ext, article_id);
    let dest_path = storage_dir.join(&dest_filename);

    /* Place the file in storage: hard-link when possible, byte-copy fallback,
    no-op when source == dest (batch import). */
    place_file_in_storage(source_path, &dest_path)?;

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

    /* When text extraction failed, surface an error audit entry so the
    degradation is visible in the Audit Timeline (not just the transient
    attach toast). The attachment itself still succeeded. */
    if let Some(ref msg) = extraction_error {
        let _ = crate::db::audit_repo::log_error(
            conn,
            &format!(
                "Full text attached to article {article_id} but {msg} (file: {original_name}). \
                 The file is stored; text-based features (screening evidence, AI summary, wiki) \
                 will be unavailable until a valid source file is provided."
            ),
        );
    }

    let attach_detail = format!(
        "Full text attached: {original_name}{}",
        extraction_error.as_ref().map(|_| " (text extraction failed)").unwrap_or("")
    );

    /* Non-fatal: attachment + file copy + DB update already succeeded, so a
    failure to write the success audit row must not unwind the operation. */
    if let Err(audit_e) = crate::db::audit_repo::create_entry(
        conn,
        article_id,
        "import",
        None,
        None,
        Some(&attach_detail),
        "user",
    ) {
        let _ = crate::db::audit_repo::log_error(
            conn,
            &format!("Failed to write success audit for article {article_id}: {audit_e}"),
        );
    }

    let extraction_failed = extraction_error.is_some();
    Ok(FullTextAttachResult {
        success: true,
        message: if extraction_failed {
            "Full text attached (text extraction failed)".to_string()
        } else {
            format!("Full text extracted ({word_count} words)")
        },
        word_count,
        extraction_failed,
    })
}

/// Attach a full-text file (PDF/TXT) to an article. Extracts text, stores in
/// DB, copies file to storage. After success, if `auto_translate = true` and
/// non-English, enqueues translation (fire-and-forget, never blocks the response).
#[tauri::command]
pub fn attach_full_text(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
    file_path: String,
) -> Result<FullTextAttachResult, AppError> {
    /* Compute the attach while holding the DB lock, then drop the guard BEFORE
    enqueuing translations (Tier 1a lock hygiene: avoids deadlock). */
    let result = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let storage_dir = compute_storage_dir(&conn)?;
        /* Read article DOI for DOI-aware filename (Concern 2). Same lock as
        storage dir resolution — no extra DB round-trip. */
        let article = article_repo::get_article_by_id(&conn, &article_id)?;
        let source_path = PathBuf::from(&file_path);
        attach_full_text_inner(
            &conn,
            &article_id,
            article.doi.as_deref(),
            &source_path,
            &storage_dir,
        )
    };

    /* Fire-and-forget translation enqueue on success. The helper internally
    checks `auto_translate`, `should_skip_translation`, and `translation_status`.
    Skipped when `extraction_failed` is true: empty `full_text` wastes worker
    effort. Retry manually once a valid source file is provided. */
    if let Ok(ref attach) = result {
        if !attach.extraction_failed {
            crate::commands::translation::try_enqueue_translations_for_import(
                &app_handle,
                &db_state.conn,
                std::slice::from_ref(&article_id),
            );
        }
    }

    result
}

/// Delete the full-text attachment for an article.
/// Removes file from storage and clears DB references.
#[tauri::command]
pub fn delete_full_text(
    db_state: tauri::State<'_, DbState>,
    _app_handle: tauri::AppHandle,
    article_id: String,
) -> Result<bool, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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

    /* Tier 3: clear the article's chunks. The row stays (only `has_full_text`
    flips), so `ON DELETE CASCADE` does not fire — explicit clear needed. */
    if let Err(e) = chunk_repo::delete_chunks_for_article(&conn, &article_id) {
        let _ = crate::db::audit_repo::log_error(
            &conn,
            &format!("Failed to clear chunks for article {article_id}: {e}"),
        );
    }

    /* Clear embeddings alongside chunks. Embeddings are regenerable derived
    artifacts; removing the source full text should remove stale vectors.
    Non-fatal: missing table on older DB is logged and continues. */
    if let Err(e) = crate::db::embedding_repo::delete_embeddings_for_article(&conn, &article_id) {
        let _ = crate::db::audit_repo::log_error(
            &conn,
            &format!("Failed to clear embeddings for article {article_id}: {e}"),
        );
    }

    /* Removing full text downgrades the wiki ingest content source (falls back
    to ai_summary or abstract). Mark stale so the next visit re-ingests. */
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

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

/// Ensure articles with `has_full_text = 1` have `article_chunks`. Pure CPU.
///
/// - `force=false` (screening start): backfills only zero-chunk articles.
/// - `force=true` (Settings "Rebuild text chunks"): re-chunks everything
///   (idempotent — `replace_chunks_for_article` deletes-then-inserts).
///
/// Errors are collected, not fatal; `RebuildChunksResult` reports counts.
/// Per-article progress callback for diagnostics (Phase B instrumentation).
/// `done` = processed so far, `total` = candidate set size, `article_id` = just
/// processed. Invoked under the caller's lock — must NOT re-enter DB.
pub type ChunkProgressCb<'a> = &'a dyn Fn(usize, usize, &str);

/// Inner loop shared by the two `ensure_chunks` variants. Walks candidate
/// article IDs, resolves on-disk PDF/TXT, parses + chunks, writes chunks.
/// Pure of progress reporting so the no-callback callers stay byte-identical.
fn ensure_chunks_inner(
    conn: &rusqlite::Connection,
    article_ids: &[String],
    storage_dir: &std::path::Path,
    progress_cb: Option<ChunkProgressCb<'_>>,
) -> (usize, usize) {
    let mut chunked = 0usize;
    let mut failed = 0usize;
    let total = article_ids.len();
    for (idx, article_id) in article_ids.iter().enumerate() {
        // Resolve the on-disk attachment path for this article.
        let file_name = match article_repo::get_full_text_file_name(conn, article_id) {
            Ok(Some(name)) => name,
            _ => {
                failed += 1;
                if let Some(cb) = progress_cb {
                    cb(idx + 1, total, article_id);
                }
                continue;
            }
        };
        let path = storage_dir.join(&file_name);
        if !path.exists() {
            failed += 1;
            if let Some(cb) = progress_cb {
                cb(idx + 1, total, article_id);
            }
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
        if let Some(cb) = progress_cb {
            cb(idx + 1, total, article_id);
        }
    }
    (chunked, failed)
}

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
    let (chunked, failed) = ensure_chunks_inner(conn, &article_ids, &storage_dir, None);

    RebuildChunksResult {
        success: true,
        chunked,
        failed,
        skipped: total.saturating_sub(chunked + failed),
        message: format!("Chunked {chunked} article(s); {failed} failed"),
    }
}

/// Diagnostics-only variant: invokes `progress_cb` after each article so the
/// screening task can emit `screening:progress` events + `[screening:diag]`
/// logs. The callback fires under the caller's lock; must NOT re-enter DB.
/// Lock contract unchanged — operates on the caller's `&Connection`, does not
/// acquire its own lock. Diagnostics-only; preserves current locking to measure
/// real production behavior.
pub fn ensure_chunks_for_full_text_articles_with_progress(
    conn: &rusqlite::Connection,
    force: bool,
    progress_cb: ChunkProgressCb<'_>,
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
    // Emit a start line so the log shows the phase entry even if total == 0.
    eprintln!("[screening:diag] phase=preparing:chunking total_missing={total} force={force}");
    let (chunked, failed) =
        ensure_chunks_inner(conn, &article_ids, &storage_dir, Some(progress_cb));

    RebuildChunksResult {
        success: true,
        chunked,
        failed,
        skipped: total.saturating_sub(chunked + failed),
        message: format!("Chunked {chunked} article(s); {failed} failed"),
    }
}

// ── Async chunk-rebuild pipeline (Settings -> Re-processing) ────────────────
//
// Replaces the old sync `rebuild_article_chunks` command, which (a) held the
// DbState mutex for the whole PDF-parse pass (UI freeze), and (b) called
// `tokio::task::spawn` from the sync IPC handler running on the main/GTK
// thread, where no Tokio reactor exists - a guaranteed panic + process abort
// ("there is no reactor running"). The new pipeline mirrors the batch-import
// split pipeline: brief discovery lock, `spawn_blocking` parses with NO lock,
// short lock bursts for writes, live progress events, cancellation.

/// Event channel carrying `RebuildChunksProgress` payloads.
pub const CHUNK_REBUILD_PROGRESS_EVENT: &str = "chunk-rebuild:progress";

/// Progress payload emitted via [`CHUNK_REBUILD_PROGRESS_EVENT`] and returned
/// by `start_rebuild_chunks` / `get_rebuild_chunks_progress`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildChunksProgress {
    /// "idle" | "chunks" | "embeddings" | "done".
    pub phase: String,
    pub is_running: bool,
    pub is_cancelled: bool,
    pub completed: usize,
    pub total: usize,
    pub percent: usize,
    pub chunked: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Translated articles skipped to preserve their English chunk rows.
    pub skipped_translated: usize,
    pub message: String,
    /// Per-article failure messages (surfaced in the Settings widget).
    pub errors: Vec<String>,
    /// Final embedding-cascade outcome line (counts, skip reason, or error).
    pub embedding_summary: Option<String>,
}

impl Default for RebuildChunksProgress {
    fn default() -> Self {
        Self {
            phase: "idle".to_string(),
            is_running: false,
            is_cancelled: false,
            completed: 0,
            total: 0,
            percent: 0,
            chunked: 0,
            failed: 0,
            skipped: 0,
            skipped_translated: 0,
            message: String::new(),
            errors: Vec::new(),
            embedding_summary: None,
        }
    }
}

/// Managed state for the chunk-rebuild background task (mirrors
/// `batch_import::BatchImportState`). The cancel token is an `AtomicBool` so
/// it plugs directly into `generate_embeddings_inner`'s cancel parameter
/// (Cancel also aborts the embedding cascade).
pub struct RebuildChunksState {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<RebuildChunksProgress>>,
}

impl Default for RebuildChunksState {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(RebuildChunksProgress::default())),
        }
    }
}

impl RebuildChunksState {
    /// Cloned cancel token for the background task.
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// Cloned progress handle for the background task.
    pub fn progress_handle(&self) -> Arc<Mutex<RebuildChunksProgress>> {
        Arc::clone(&self.progress)
    }
}

/// 0-100% avoiding division by zero (100 when the work set is empty).
#[must_use]
fn rebuild_percent(done: usize, total: usize) -> usize {
    done.saturating_mul(100).checked_div(total).unwrap_or(100)
}

/// Hard cap on `RebuildChunksProgress.errors`. The list is cloned +
/// serialized into every `chunk-rebuild:progress` event; unbounded growth is
/// O(n^2) IPC at the 10k-article cap. The `failed` counter always reports the
/// true total; overflow is summarized at finalize time (audit rows carry all).
const MAX_PROGRESS_ERRORS: usize = 50;

/// Lock the progress snapshot, apply `f`, recompute `percent`, return the
/// cloned payload for event emission.
fn update_rebuild_progress(
    progress: &Arc<Mutex<RebuildChunksProgress>>,
    f: impl FnOnce(&mut RebuildChunksProgress),
) -> Result<RebuildChunksProgress, AppError> {
    let mut guard = progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    f(&mut guard);
    guard.percent = rebuild_percent(guard.completed, guard.total);
    Ok(guard.clone())
}

/// Emit one progress event. `app_handle: None` = test mode (no events).
fn emit_rebuild_progress(app_handle: Option<&tauri::AppHandle>, payload: &RebuildChunksProgress) {
    if let Some(handle) = app_handle {
        let _ = handle.emit(CHUNK_REBUILD_PROGRESS_EVENT, payload);
    }
}

/// Atomically claim the run slot: if a rebuild is already running, return
/// `false` (caller surfaces the live snapshot). Otherwise reset the cancel
/// token + snapshot and mark running, all in ONE critical section so two
/// overlapping `start_rebuild_chunks` invokes can never both spawn a task
/// (the previous check-then-act pair had that race).
pub fn claim_run_slot(
    progress: &Arc<Mutex<RebuildChunksProgress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<bool, AppError> {
    let mut guard = progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    if guard.is_running {
        return Ok(false);
    }
    cancel.store(false, Ordering::Relaxed);
    *guard = RebuildChunksProgress::default();
    guard.is_running = true;
    guard.phase = "chunks".to_string();
    Ok(true)
}

/// Release a claimed-but-unstarted slot (discovery error path).
pub fn release_run_slot(progress: &Arc<Mutex<RebuildChunksProgress>>) -> Result<(), AppError> {
    let mut guard = progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    guard.is_running = false;
    Ok(())
}

/// Terminal epilogue shared by the background task. Sets `is_cancelled` from
/// the token (covers a cancel that landed during the embedding cascade, which
/// the loop's pre-article check never sees), appends the error-truncation
/// tail, and computes the final summary. `skipped` stays exactly what the
/// loop counted (== translated skips) - never recomputed here.
pub fn finalize_rebuild_progress(
    progress: &Arc<Mutex<RebuildChunksProgress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<RebuildChunksProgress, AppError> {
    let mut guard = progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    if cancel.load(Ordering::Relaxed) {
        guard.is_cancelled = true;
    }
    if guard.failed > guard.errors.len() {
        let more = guard.failed - guard.errors.len();
        guard.errors.push(format!("... and {more} more failures (see Diagnostics)"));
    }
    guard.is_running = false;
    guard.phase = "done".to_string();
    guard.message = rebuild_summary_message(
        guard.chunked,
        guard.failed,
        guard.skipped_translated,
        guard.is_cancelled,
    );
    guard.percent = rebuild_percent(guard.completed, guard.total);
    Ok(guard.clone())
}

/// What the rebuild loop did; consumed by the embedding cascade.
#[derive(Debug, Default)]
pub struct RebuildOutcome {
    /// Articles whose chunks were regenerated this run (their embedding rows
    /// were deleted; the cascade regenerates them).
    pub chunked_ids: Vec<String>,
    /// Translated articles skipped this run (English chunks preserved; the
    /// cascade only BACKFILLS their missing embedding rows).
    pub translated_ids: Vec<String>,
}

/// Pure: final one-line summary for the progress widget.
#[must_use]
pub fn rebuild_summary_message(
    chunked: usize,
    failed: usize,
    skipped_translated: usize,
    cancelled: bool,
) -> String {
    let mut parts = vec![format!("{chunked} chunked"), format!("{failed} failed")];
    if skipped_translated > 0 {
        parts.push(format!("{skipped_translated} skipped (translated, English chunks preserved)"));
    }
    let prefix = if cancelled { "Cancelled after " } else { "" };
    format!("{prefix}{}", parts.join(", "))
}

/// Pure: map the two id sets into embedding scopes. `None` when a set is
/// empty (that cascade call is skipped entirely). Both scopes use
/// `force = false`:
/// - the regenerate scope's articles just had their embedding rows deleted,
///   so "missing row" alone drives full regeneration (orphan-free);
/// - the backfill scope (translated articles) must only generate rows that do
///   not exist and never re-embed fresh English rows.
#[must_use]
pub fn embedding_cascade_scopes(
    chunked_ids: &[String],
    translated_ids: &[String],
) -> (Option<EmbeddingScope>, Option<EmbeddingScope>) {
    let make = |ids: &[String]| {
        (!ids.is_empty()).then(|| EmbeddingScope {
            article_ids: Some(ids.to_vec()),
            status_filter: None,
            force: false,
        })
    };
    (make(chunked_ids), make(translated_ids))
}

/// Pure: human-readable one-line summary of the cascade outcome. Skip reasons
/// (from `generate_embeddings_inner` / the director) map into friendly
/// wording; otherwise per-scope counts are joined.
#[must_use]
pub fn embedding_summary_line(
    regen: Option<&EmbeddingRunReport>,
    backfill: Option<&EmbeddingRunReport>,
) -> Option<String> {
    let friendly = |report: &EmbeddingRunReport| -> Option<String> {
        let reason = report.skip_reason.as_deref()?;
        let text = match reason {
            "LlmNotConfigured" => "LLM not configured".to_string(),
            "Disabled" => "provider does not support embeddings".to_string(),
            other => other.to_string(),
        };
        Some(format!("Embeddings skipped: {text}"))
    };
    /* A skip gate (LLM not configured / provider cannot embed) dominates:
    both calls skip for the same reason, so one line suffices. */
    if let Some(line) = regen.and_then(friendly).or_else(|| backfill.and_then(friendly)) {
        return Some(line);
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(r) = regen {
        parts.push(format!("{} regenerated", r.generated));
        if r.errors > 0 {
            parts.push(format!("{} errors", r.errors));
        }
    }
    if let Some(r) = backfill {
        parts.push(format!("{} backfilled", r.generated));
        if r.errors > 0 {
            parts.push(format!("{} errors", r.errors));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let model = regen
        .or(backfill)
        .map(|r| r.model.as_str())
        .filter(|m| !m.is_empty())
        .map(|m| format!(" ({m})"))
        .unwrap_or_default();
    Some(format!("Embeddings: {}{model}", parts.join(", ")))
}

/// Record one per-article failure: `failed++`, message into `errors`, plus a
/// system audit row (article id embedded in the message) so the Diagnostics
/// feed shows the cause. Fixes the two previously-silent failure modes
/// (missing file name, missing on-disk file).
async fn record_rebuild_failure(
    conn_mutex: &Mutex<rusqlite::Connection>,
    progress: &Arc<Mutex<RebuildChunksProgress>>,
    app_handle: Option<&tauri::AppHandle>,
    message: String,
) -> Result<(), AppError> {
    {
        let conn = crate::db::connection::lock_conn(conn_mutex)?;
        let _ = crate::db::audit_repo::log_error(&conn, &format!("ensure_chunks: {message}"));
    }
    let payload = update_rebuild_progress(progress, |p| {
        p.completed += 1;
        p.failed += 1;
        if p.errors.len() < MAX_PROGRESS_ERRORS {
            p.errors.push(message);
        }
    })?;
    emit_rebuild_progress(app_handle, &payload);
    tokio::task::yield_now().await;
    Ok(())
}

/// Core chunk-rebuild loop (testable: plain `&Mutex<Connection>`, no Tauri
/// state). Split-pipeline lock discipline: `spawn_blocking` parses with NO DB
/// lock; short lock bursts only for the chunk write + embedding
/// invalidation; progress event + `yield_now()` after every article.
///
/// Translated guard: `is_translated = 1` candidates are NEVER re-chunked
/// (the working chunk rows hold the English translation; parsing the on-disk
/// PDF would replace them with original-language text).
///
/// `app_handle: None` = test mode (progress struct updates, no events).
pub async fn rebuild_chunks_loop(
    conn_mutex: &Mutex<rusqlite::Connection>,
    storage_dir: &Path,
    candidates: &[chunk_repo::FullTextChunkCandidate],
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<RebuildChunksProgress>>,
    app_handle: Option<&tauri::AppHandle>,
) -> Result<RebuildOutcome, AppError> {
    let total = candidates.len();
    let mut outcome = RebuildOutcome::default();
    {
        let payload = update_rebuild_progress(progress, |p| {
            p.phase = "chunks".to_string();
            p.total = total;
            p.message = if total == 0 {
                "No articles with full text".to_string()
            } else {
                "Rebuilding text chunks...".to_string()
            };
        })?;
        emit_rebuild_progress(app_handle, &payload);
    }

    for candidate in candidates {
        // Cancel takes effect before the next article (never mid-parse).
        if cancel.load(Ordering::Relaxed) {
            let payload = update_rebuild_progress(progress, |p| {
                p.is_cancelled = true;
                p.message = "Cancelled".to_string();
            })?;
            emit_rebuild_progress(app_handle, &payload);
            break;
        }

        // Translated articles: preserve the English chunk rows; the cascade
        // later backfills only their MISSING embedding rows.
        if candidate.is_translated {
            outcome.translated_ids.push(candidate.id.clone());
            let payload = update_rebuild_progress(progress, |p| {
                p.completed += 1;
                p.skipped += 1;
                p.skipped_translated += 1;
            })?;
            emit_rebuild_progress(app_handle, &payload);
            tokio::task::yield_now().await;
            continue;
        }

        // Failure mode 1: no stored file name.
        let Some(file_name) = candidate.file_name.clone() else {
            record_rebuild_failure(
                conn_mutex,
                progress,
                app_handle,
                format!("Article {} has no full_text_file_name", candidate.id),
            )
            .await?;
            continue;
        };

        // Failure mode 2: file missing from the fulltext storage dir.
        let path = storage_dir.join(&file_name);
        if !path.exists() {
            record_rebuild_failure(
                conn_mutex,
                progress,
                app_handle,
                format!("File not found for article {}: {}", candidate.id, path.display()),
            )
            .await?;
            continue;
        }

        // Lock-free CPU-bound parse on the blocking pool. Failure mode 3:
        // extraction error (or a panicked blocking task).
        let parse_path = path.clone();
        let parsed = tokio::task::spawn_blocking(move || {
            extract_sections(&parse_path)
                .map(|sections| chunk_sections(&sections, DEFAULT_CHUNK_WORDS))
                .map_err(AppError::Import)
        })
        .await;
        let chunks = match parsed {
            Ok(Ok(chunks)) => chunks,
            Ok(Err(e)) => {
                record_rebuild_failure(
                    conn_mutex,
                    progress,
                    app_handle,
                    format!("Chunk extraction failed for article {}: {e}", candidate.id),
                )
                .await?;
                continue;
            }
            Err(join_err) => {
                record_rebuild_failure(
                    conn_mutex,
                    progress,
                    app_handle,
                    format!(
                        "Chunk extraction task panicked for article {}: {join_err}",
                        candidate.id
                    ),
                )
                .await?;
                continue;
            }
        };

        // Short lock burst: replace chunk rows + invalidate the article's
        // embedding rows in ONE transaction. Re-chunked text makes stored
        // vectors stale and the runner only INSERTs (an explicit delete also
        // prunes orphaned high-index rows); transactional so a DELETE failure
        // after a successful REPLACE rolls back instead of leaving chunks and
        // vectors divergent while the article is counted "failed".
        let write = match crate::db::connection::lock_conn(conn_mutex) {
            Ok(conn) => conn.unchecked_transaction().map_err(AppError::from).and_then(|tx| {
                chunk_repo::replace_chunks_for_article(&tx, &candidate.id, &chunks)?;
                embedding_repo::delete_embeddings_for_article(&tx, &candidate.id)?;
                tx.commit().map_err(AppError::from)
            }),
            Err(e) => Err(e),
        };
        if let Err(e) = write {
            record_rebuild_failure(
                conn_mutex,
                progress,
                app_handle,
                format!("Failed to write chunks for article {}: {e}", candidate.id),
            )
            .await?;
            continue;
        }

        outcome.chunked_ids.push(candidate.id.clone());
        let payload = update_rebuild_progress(progress, |p| {
            p.completed += 1;
            p.chunked += 1;
        })?;
        emit_rebuild_progress(app_handle, &payload);
        tokio::task::yield_now().await;
    }

    Ok(outcome)
}

/// Regenerate + backfill embeddings after the chunk loop. Runs INSIDE the
/// spawned background task - the valid Tokio context. (The old sync command's
/// `tokio::task::spawn` from the main thread is exactly what crashed the app.)
///
/// Gating (LLM configured, provider can embed) is owned by
/// `generate_embeddings_inner` + the director: `LlmNotConfigured` and
/// `Disabled` return skipped reports, `unknown` triggers a live probe.
async fn run_embedding_cascade(
    db_state: &tauri::State<'_, DbState>,
    app_handle: &tauri::AppHandle,
    outcome: &RebuildOutcome,
    cancel: Arc<AtomicBool>,
    progress: &Arc<Mutex<RebuildChunksProgress>>,
) -> Result<(), AppError> {
    let (regen_scope, backfill_scope) =
        embedding_cascade_scopes(&outcome.chunked_ids, &outcome.translated_ids);
    if regen_scope.is_none() && backfill_scope.is_none() {
        return Ok(());
    }

    {
        let payload = update_rebuild_progress(progress, |p| {
            p.phase = "embeddings".to_string();
            p.message = "Updating embeddings...".to_string();
        })?;
        emit_rebuild_progress(Some(app_handle), &payload);
    }

    let orchestrator =
        app_handle.state::<Arc<crate::llm::orchestrator::LlmOrchestrator>>().inner().clone();
    let mut regen_report: Option<EmbeddingRunReport> = None;
    let mut backfill_report: Option<EmbeddingRunReport> = None;
    let mut failure: Option<String> = None;

    for (scope, slot) in [(regen_scope, &mut regen_report), (backfill_scope, &mut backfill_report)]
    {
        let Some(scope) = scope else { continue };
        let sender: Arc<dyn EmbeddingBatchSender> =
            Arc::new(HttpEmbeddingBatchSender::new(Arc::clone(&orchestrator)));
        /* `emit_events = true`: the runner emits per-article
        `embedding:progress` + final `embedding:done` for live sub-progress.
        The shared cancel token lets Cancel abort this cascade too. */
        match generate_embeddings_inner(
            db_state,
            sender,
            scope,
            Some(app_handle),
            true,
            Some(Arc::clone(&cancel)),
        )
        .await
        {
            Ok(report) => *slot = Some(report),
            Err(e) => {
                failure = Some(format!("Embeddings failed: {e}"));
                break;
            }
        }
    }

    let summary =
        failure.or_else(|| embedding_summary_line(regen_report.as_ref(), backfill_report.as_ref()));
    let payload = update_rebuild_progress(progress, |p| {
        p.embedding_summary = summary.clone();
    })?;
    emit_rebuild_progress(Some(app_handle), &payload);
    Ok(())
}

/// Start the async chunk-rebuild background task (Settings -> Re-processing
/// "Rebuild text chunks" button). Returns the initial progress snapshot;
/// live updates arrive via `chunk-rebuild:progress`; cancel via
/// `cancel_rebuild_chunks`. Never blocks: the parse loop runs on the spawned
/// task with short lock bursts, so the UI stays responsive.
#[tauri::command]
pub async fn start_rebuild_chunks(
    app_handle: tauri::AppHandle,
    db_state: tauri::State<'_, DbState>,
    rebuild_state: tauri::State<'_, RebuildChunksState>,
) -> Result<RebuildChunksProgress, AppError> {
    // Atomic run-slot claim: an in-flight rebuild returns its live snapshot;
    // a fresh claim resets the token + snapshot in the SAME critical section
    // (no double-spawn race between overlapping start invokes).
    if !claim_run_slot(&rebuild_state.progress, &rebuild_state.cancel_handle())? {
        let guard =
            rebuild_state.progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
        return Ok(guard.clone());
    }

    let cancel = rebuild_state.cancel_handle();
    let progress = rebuild_state.progress_handle();

    /* Brief discovery lock: storage dir + full candidate work list, then
    release before any parsing (split-pipeline lock hygiene). A failure here
    must release the claimed slot so a later start is not rejected forever. */
    let (storage_dir, candidates) = {
        let discovery: Result<(PathBuf, Vec<chunk_repo::FullTextChunkCandidate>), AppError> =
            (|| {
                let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                let dir = compute_storage_dir(&conn)?;
                let list = chunk_repo::get_full_text_chunk_candidates(&conn)?;
                Ok((dir, list))
            })();
        match discovery {
            Ok(found) => found,
            Err(e) => {
                release_run_slot(&progress)?;
                return Err(e);
            }
        }
    };

    let total = candidates.len();
    update_rebuild_progress(&progress, |p| {
        p.total = total;
    })?;

    /* Spawn the background task (we are on the async runtime, so
    `tokio::spawn` is valid here - unlike the old sync command). The task
    runs the loop, then the gated embedding cascade, then emits the final
    snapshot. */
    let task_handle = app_handle.clone();
    let task_cancel = Arc::clone(&cancel);
    let task_progress = Arc::clone(&progress);
    tokio::spawn(async move {
        let db = task_handle.state::<DbState>();
        let outcome = match rebuild_chunks_loop(
            &db.conn,
            &storage_dir,
            &candidates,
            &task_cancel,
            &task_progress,
            Some(&task_handle),
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(e) => {
                /* Loop-level failure (lock poison / audit write): finalize
                first (cancel flag, error tail, done state), then override the
                message with the failure cause. */
                if finalize_rebuild_progress(&task_progress, &task_cancel).is_ok() {
                    if let Ok(payload) = update_rebuild_progress(&task_progress, |p| {
                        p.message = format!("Chunk rebuild failed: {e}");
                    }) {
                        emit_rebuild_progress(Some(&task_handle), &payload);
                    }
                }
                return;
            }
        };

        // Cascade only on a clean (non-cancelled) run.
        if !task_cancel.load(Ordering::Relaxed) {
            let _ = run_embedding_cascade(
                &db,
                &task_handle,
                &outcome,
                Arc::clone(&task_cancel),
                &task_progress,
            )
            .await;
        }

        // Terminal epilogue: picks up a cancel that landed during the cascade,
        // appends the error-truncation tail, keeps `skipped` == translated
        // skips, computes the summary.
        if let Ok(payload) = finalize_rebuild_progress(&task_progress, &task_cancel) {
            emit_rebuild_progress(Some(&task_handle), &payload);
        }
    });

    let guard = progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    Ok(guard.clone())
}

/// Cancel a running chunk rebuild. Takes effect before the next article; a
/// shared token also aborts the embedding cascade mid-run.
#[tauri::command]
pub async fn cancel_rebuild_chunks(
    rebuild_state: tauri::State<'_, RebuildChunksState>,
) -> Result<(), AppError> {
    rebuild_state.cancel_handle().store(true, Ordering::Relaxed);
    Ok(())
}

/// Current chunk-rebuild progress snapshot (used by the Settings UI to
/// restore the progress bar after navigating away and back).
#[tauri::command]
pub async fn get_rebuild_chunks_progress(
    rebuild_state: tauri::State<'_, RebuildChunksState>,
) -> Result<RebuildChunksProgress, AppError> {
    let guard = rebuild_state.progress.lock().map_err(|e| AppError::LockPoisoned(e.to_string()))?;
    Ok(guard.clone())
}

/// Count articles with full text attached (drives the "Rebuild text chunks"
/// button label in Settings -> Full-Text Storage).
#[tauri::command]
pub fn count_articles_with_full_text(db_state: tauri::State<'_, DbState>) -> Result<i64, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    chunk_repo::count_articles_with_full_text(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_filename_uses_clean_doi_when_present() {
        // DOI present -> destination is `{clean_doi}.{ext}` with no UUID.
        assert_eq!(
            compute_dest_filename(
                Some("10.1016/j.jand.2021.06.013"),
                "ignored-stem",
                "pdf",
                "art-uuid"
            ),
            "10.1016_j.jand.2021.06.013.pdf"
        );
    }

    #[test]
    fn dest_filename_uses_clean_doi_with_no_extension() {
        assert_eq!(
            compute_dest_filename(Some("10.1001/foo"), "stem", "", "art-uuid"),
            "10.1001_foo"
        );
    }

    #[test]
    fn dest_filename_trims_whitespace_in_doi() {
        assert_eq!(
            compute_dest_filename(Some("  10.1001/foo  "), "stem", "pdf", "art-uuid"),
            "10.1001_foo.pdf"
        );
    }

    #[test]
    fn dest_filename_falls_back_to_uuid_when_doi_absent() {
        assert_eq!(
            compute_dest_filename(None, "my-paper", "pdf", "art-uuid"),
            "my-paper_art-uuid.pdf"
        );
    }

    #[test]
    fn dest_filename_falls_back_to_uuid_when_doi_empty() {
        // Empty / whitespace-only DOI is treated as absent.
        assert_eq!(
            compute_dest_filename(Some("   "), "my-paper", "pdf", "art-uuid"),
            "my-paper_art-uuid.pdf"
        );
        assert_eq!(
            compute_dest_filename(Some(""), "my-paper", "pdf", "art-uuid"),
            "my-paper_art-uuid.pdf"
        );
    }

    #[test]
    fn dest_filename_uuid_fallback_without_extension() {
        assert_eq!(compute_dest_filename(None, "stem", "", "art-uuid"), "stem_art-uuid");
    }

    #[test]
    fn place_file_no_op_when_source_equals_dest() {
        // Same canonical path: the helper must short-circuit and not attempt
        // a self-copy (which would truncate the file on some platforms).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("paper.pdf");
        std::fs::write(&path, b"hello").expect("write");
        place_file_in_storage(&path, &path).expect("same-file is a no-op");
        assert_eq!(std::fs::read(&path).unwrap(), b"hello", "content must be unchanged");
    }

    #[test]
    fn place_file_hard_links_or_copies_to_new_dest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src.pdf");
        let dest = dir.path().join("dest.pdf");
        std::fs::write(&src, b"hello").expect("write");
        place_file_in_storage(&src, &dest).expect("place succeeds");
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello", "dest must have the content");
    }

    #[test]
    fn place_file_copy_fallback_overwrites_existing_dest() {
        // A prior attach may have created the destination already; the copy
        // fallback must overwrite it rather than fail.
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("src.pdf");
        let dest = dir.path().join("dest.pdf");
        std::fs::write(&src, b"new").expect("write src");
        std::fs::write(&dest, b"old").expect("write dest");
        place_file_in_storage(&src, &dest).expect("place succeeds via copy fallback");
        assert_eq!(std::fs::read(&dest).unwrap(), b"new", "dest must be overwritten");
    }
}
