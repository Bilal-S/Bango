use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::chunk_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::scraping::citation_chaser::clean_doi_filename;
use crate::utils::chunking::{chunk_sections, Chunk, DEFAULT_CHUNK_WORDS};
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
    /// `true` when the file attached but text extraction failed (empty
    /// `full_text` persisted). Callers use this to skip downstream
    /// text-dependent work (e.g. translation enqueue) that would otherwise
    /// operate on empty content.
    pub extraction_failed: bool,
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

/// Compute the destination filename for a full-text attachment.
///
/// - When the article has a DOI, the destination is
///   `{clean_doi_filename(doi)}.{ext}`. This matches the on-disk batch-import
///   naming convention (`{clean_doi}.pdf` / `.txt`) and the Citation Chaser
///   RIS convention (`{clean_doi}_references.ris`), so batch import no longer
///   produces a redundant `{clean_doi}_{uuid}.{ext}` duplicate of an already
///   uniquely-named file.
/// - When the article has NO DOI (or an empty/whitespace DOI), the destination
///   falls back to `{stem}_{article_id}.{ext}` so the UUID disambiguates files
///   that would otherwise collide (manual uploads, OpenAlex imports of
///   no-DOI works, etc.).
///
/// Pure `#[must_use]` so the naming decision is unit-testable in isolation.
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

/// Resolve `source_path` to `dest_path` inside `storage_dir`, performing a
/// zero-copy hard link when possible and falling back to a byte copy when the
/// hard link fails (cross-device, unsupported filesystem, or already-linked).
///
/// When `source_path` and `dest_path` resolve to the same canonical file
/// (common in batch import where the file is already in `fulltext/` with the
/// correct DOI-based name), this is a no-op: no copy and no link, so the
/// original file is left in place and no duplicate is created.
fn place_file_in_storage(source_path: &Path, dest_path: &Path) -> Result<(), AppError> {
    // Same-file short-circuit: canonicalize both paths and compare. If they
    // resolve to the same inode, the file is already in storage (batch import
    // feeds files already named `{clean_doi}.pdf` living inside `fulltext/`),
    // so there is nothing to copy. This avoids `std::fs::copy` self-copy
    // errors and the redundant duplicate the old `{stem}_{uuid}.{ext}` logic
    // produced.
    let same = match (source_path.canonicalize(), dest_path.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        // If canonicalize fails (e.g. dest does not exist yet, which is the
        // common case), fall through to the link/copy path.
        _ => false,
    };
    if same {
        return Ok(());
    }
    // Prefer a hard link (zero-copy, both names point at the same inode). This
    // is the common case inside a single storage root. Fall back to a byte
    // copy when hard_link fails (cross-device, unsupported FS, or the link
    // already exists from a prior attach).
    match std::fs::hard_link(source_path, dest_path) {
        Ok(()) => Ok(()),
        Err(_) => std::fs::copy(source_path, dest_path)
            .map(|_| ())
            .map_err(|e| AppError::Import(format!("Failed to copy file to storage: {e}"))),
    }
}

/// The CPU-bound extraction results from a full-text source file, produced
/// with NO DB lock held. The companion [`commit_full_text_to_db`] consumes
/// this under a short DB lock burst to perform only the writes (update row,
/// insert chunks, audit entries, staleness flags). Splitting the two phases
/// lets batch-import Phase 1 run the CPU-bound parse on `spawn_blocking`
/// without freezing every other DB-touching IPC command.
#[derive(Debug, Clone)]
pub struct ExtractedFullText {
    /// The extracted full text (empty string on extraction failure - soft
    /// fallback so the file still attaches).
    pub full_text: String,
    /// Word count of `full_text` (cached so the DB-write phase doesn't
    /// recompute it).
    pub word_count: usize,
    /// `true` when figure/table captions were detected (drives the persisted
    /// `has_figures_or_tables` flag).
    pub has_figures_or_tables: bool,
    /// Pre-computed chunks (Tier 3 screening evidence). Empty on extraction
    /// failure or when section extraction fails.
    pub chunks: Vec<Chunk>,
    /// The destination filename (DOI-aware via `compute_dest_filename`).
    pub dest_filename: String,
    /// The original source file name (for audit messages).
    pub original_name: String,
    /// Extraction error message, when extraction failed (soft fallback). The
    /// file still attaches with an empty `full_text`; this is surfaced in the
    /// audit trail so the user knows text-based features are unavailable.
    pub extraction_error: Option<String>,
}

/// Extract all CPU-bound data from a full-text source file with NO DB access.
///
/// This is the lock-free half of the split attach pipeline (Concern 3 gap
/// fix): it does the PDF/TXT parse, caption detection, section/chunk
/// extraction, filename computation, and file placement. The companion
/// [`commit_full_text_to_db`] takes the returned [`ExtractedFullText`] and
/// performs only the DB writes under a short lock burst.
///
/// Pure of DB state; safe to call inside `spawn_blocking`. Returns a hard
/// `Err` only for an unsupported extension or a missing source file - the
/// soft-fallback path (extraction failure -> empty `full_text`) returns
/// `Ok(ExtractedFullText { extraction_error: Some(..), .. })` so the caller
/// can still persist the attachment.
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

    // Section/chunk extraction for Tier 3 screening evidence. This is the
    // second CPU-bound parse; it re-reads the source file via
    // `extract_sections`. On failure, chunks are empty (non-fatal).
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

/// Commit an already-extracted full-text attachment to the database. This is
/// the lock-bound half of the split attach pipeline (Concern 3 gap fix): it
/// performs only the writes (`update_full_text`, chunk insert, audit entries,
/// staleness flags) and is fast (millisecond-scale), so the caller can hold
/// the DB lock for just this portion and run the CPU-bound extraction via
/// [`extract_full_text_data`] outside the lock.
///
/// The `article_id` is threaded separately (not embedded in
/// [`ExtractedFullText`]) so the pure extraction helper stays free of any
/// per-article DB state.
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

/// Split attach pipeline for batch import: runs the CPU-bound extraction on
/// `spawn_blocking` (NO DB lock held), then takes a short lock burst for the
/// DB writes only. Resolves both Concern 3 gaps (PDF parse inside the lock +
/// blocking a tokio worker).
///
/// Callers that already hold a `&Connection` (manual `attach_full_text`
/// command, OpenAlex import) should call [`extract_full_text_data`] +
/// [`commit_full_text_to_db`] directly instead. This async helper is for the
/// batch-import Phase 1 runner, which owns a `&Mutex<Connection>` and must
/// not hold it across the parse.
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

/// Attach a full-text file (PDF or TXT) to an article (monolithic path).
///
/// This is the legacy single-call API retained for the manual
/// `attach_full_text` Tauri command and the OpenAlex import paths, which
/// already hold a `&Connection`. The batch-import Phase 1 runner uses the
/// split [`attach_full_text_split`] pipeline instead so the CPU-bound PDF
/// parse runs on `spawn_blocking` without holding the DB lock.
///
/// # Destination filename contract (Concern 2)
///
/// - When `article_doi` is `Some(non-empty)`, the destination filename is
///   `{clean_doi_filename(doi)}.{ext}`. This matches the on-disk batch-import
///   convention so batch import no longer produces a redundant
///   `{clean_doi}_{uuid}.{ext}` duplicate of an already uniquely-named file.
/// - When `article_doi` is `None` (or empty/whitespace), the destination falls
///   back to `{stem}_{article_id}.{ext}` so the UUID disambiguates files that
///   would otherwise collide (manual uploads, OpenAlex imports of no-DOI
///   works).
/// - When `source_path` already resolves to the destination (same canonical
///   file - common in batch import where the file is already in `fulltext/`
///   with the correct DOI-based name), no copy/link is performed.
///
/// # Arguments
/// * `conn` - A locked SQLite connection.
/// * `article_id` - The article to attach the full text to.
/// * `article_doi` - The article's DOI, if known. When `Some`, drives the
///   DOI-aware destination filename. Callers that already have the DOI in
///   hand (batch import, OpenAlex) pass it directly; the Tauri command
///   wrapper reads it from the DB.
/// * `source_path` - Path to the source PDF/TXT file on disk.
/// * `storage_dir` - The fulltext storage directory (from `compute_storage_dir`).
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

    // Extract text based on file type. On extraction failure, fall back to an
    // empty string so the attachment still persists (the file is copied to
    // storage, `has_full_text` flips, and the in-app reader can still open the
    // raw file). The error is surfaced via the audit table below so the user
    // knows text-based features (screening evidence, AI summary, wiki) will be
    // unavailable until a valid source file is provided. Only an unsupported
    // extension is a hard error (nothing to attach in that case).
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

    // Detect figure/table captions in the extracted text so the persisted
    // `has_figures_or_tables` flag matches the generation path's own
    // precondition (`generate_figure_descriptions` validates via the same
    // `extract_captions` call and errors with "No figure/table captions
    // detected" when the result is empty). Using the same detector here keeps
    // the frontend button gate DRY with the backend generation precondition.
    // On extraction failure `full_text` is empty, so this safely yields false.
    let has_figures_or_tables = !extract_captions(&full_text).is_empty();

    // Build destination filename via the DOI-aware helper (Concern 2).
    let original_name = source_path.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let dest_filename = compute_dest_filename(article_doi, stem, ext, article_id);
    let dest_path = storage_dir.join(&dest_filename);

    // Place the file in storage: zero-copy hard link when possible, byte-copy
    // fallback, and a no-op when source == destination (common in batch import
    // where the file is already in `fulltext/` with the correct DOI name).
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

    // When text extraction failed, surface a general-error audit entry so the
    // degradation is visible in the Audit Timeline (not just the transient
    // attach toast). The attachment itself still succeeded.
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

    // Non-fatal: the attachment + file copy + DB update have already succeeded,
    // so a failure to write the success audit row must not unwind the operation.
    // The chunk-populate and extraction-error audit writes above already use the
    // same `let _ = …` policy; this keeps the three audit paths consistent.
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

/// Attach a full-text file (PDF or TXT) to an article.
/// Extracts text content, stores in DB, and copies file to storage directory.
///
/// After a successful attach, if `auto_translate = true` and the article's
/// `language` is non-English, enqueues a translation job. The attach response
/// is never blocked on translation (fire-and-forget via the worker channel).
#[tauri::command]
pub fn attach_full_text(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
    article_id: String,
    file_path: String,
) -> Result<FullTextAttachResult, AppError> {
    // Compute the attach result while holding the DB lock, then drop the guard
    // BEFORE enqueuing translations so the (re-locking) batch enqueue helper
    // does not deadlock / serialize against the attach transaction
    // (Tier 1a lock hygiene).
    let result = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let storage_dir = compute_storage_dir(&conn)?;
        // Read the article's DOI so the DOI-aware destination filename is used
        // (Concern 2). The article row is already loaded here under the same
        // lock that computes the storage dir, so no extra DB round-trip.
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

    // Fire-and-forget translation enqueue on successful attach. The enqueue
    // gate inside the helper checks `auto_translate`, `should_skip_translation`
    // (English OR absent/blank language), and the article's
    // `translation_status` so existing already-translated / queued / English
    // articles are skipped silently.
    //
    // Skipped when `extraction_failed` is true: a corrupt/empty PDF yields an
    // empty `full_text`, so a `FullText` translation job would translate
    // nothing and waste worker effort. The user can retry translation
    // manually once a valid source file is provided.
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
/// Per-article progress callback for `ensure_chunks_for_full_text_articles`.
///
/// `done` is the number of articles processed so far (chunked + failed +
/// skipped), `total` is the size of the missing-chunks (or all-full-text, when
/// `force`) candidate set, `article_id` is the article just processed.
///
/// The callback is invoked **under the same `&Connection` lock** the caller
/// holds — it must NOT re-enter the DB. It is purely for emitting diagnostic
/// progress events + log lines so the UI + stderr show the chunk-backfill
/// phase advancing instead of a silent freeze. Diagnostics-only (Phase B
/// instrumentation); carries no behavioral contract.
pub type ChunkProgressCb<'a> = &'a dyn Fn(usize, usize, &str);

/// Inner loop shared by `ensure_chunks_for_full_text_articles` and the
/// progress-emitting variant. Walks the candidate article-id list, resolves
/// each on-disk PDF/TXT, parses + chunks it, and writes the chunks. Pure of
/// progress reporting so the original (no-callback) callers stay byte-identical.
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

/// Diagnostics-only variant: same as `ensure_chunks_for_full_text_articles` but
/// invokes `progress_cb` after each article so the screening task can emit
/// `screening:progress` events with `phase = "preparing:chunking"` + a
/// `[screening:diag] chunk_progress` log line. The callback fires under the
/// caller's lock and must not re-enter the DB.
///
/// **Lock contract: unchanged from `ensure_chunks_for_full_text_articles`.**
/// This function still acquires no lock of its own; it operates on the
/// `&Connection` the caller already holds. The screening task holds the DbState
/// mutex for the full pass exactly as today — the per-article callback only
/// emits events between articles, it does NOT release/re-acquire the lock.
/// Layer 2 (deferred) will refactor the lock scope; this diagnostics-only
/// addition intentionally preserves the current locking to measure the real
/// production behavior first.
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

/// One-shot rebuild: (re)build `article_chunks` for *every* article that has
/// full text attached (including ones that already have chunks). Wired to the
/// Settings -> Full-Text Storage "Rebuild text chunks" button so a user can
/// repair a corrupted/partial/outdated chunk set or pick up chunking-algorithm
/// updates. `force=true` selects `get_articles_with_full_text` (all rows) rather
/// than the missing-chunks-only query used by the screening-start guard.
#[tauri::command]
pub fn rebuild_article_chunks(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
) -> Result<RebuildChunksResult, AppError> {
    let result = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        ensure_chunks_for_full_text_articles(&conn, true)
    };
    // Cascade: re-chunking changes the chunk body text, which invalidates the
    // `input_hash` for every chunk row in `article_embeddings`. Fire-and-forget
    // an embedding regeneration pass (`force=true` so every row is re-embedded
    // regardless of the stale hash). Non-blocking: the rebuild result is
    // returned immediately, and the embedding runs on a detached task that
    // respects the orchestrator's concurrency + rate limits. Embeddings are
    // best-effort here: a failure is logged inside the runner and never
    // surfaces to the rebuild caller.
    if result.success && result.chunked > 0 {
        let handle = app_handle.clone();
        tokio::task::spawn(async move {
            let db = handle.state::<crate::db::connection::DbState>();
            let orch = handle.state::<std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>();
            // Wrap the orchestrator into the v2 HttpEmbeddingBatchSender.
            let sender: std::sync::Arc<dyn crate::embedding::runner::EmbeddingBatchSender> =
                std::sync::Arc::new(crate::embedding::runner::HttpEmbeddingBatchSender::new(
                    std::sync::Arc::clone(&orch),
                ));
            let scope = crate::embedding::director::EmbeddingScope {
                article_ids: None,
                status_filter: Some("included".to_string()),
                force: true,
            };
            let _ = crate::embedding::runner::generate_embeddings_inner(
                &db,
                sender,
                scope,
                Some(&handle),
                false,
                None,
            )
            .await;
        });
    }
    Ok(result)
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
