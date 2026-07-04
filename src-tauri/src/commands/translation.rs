//! Tauri commands for the translation pipeline.
//!
//! - `enqueue_article_translation` - the enqueue gate (read status; `none`/
//!   `failed` → write `queued` + send; else skip).
//! - `get_translation_status` - read the status snapshot.
//! - `retry_translation_job` - reset to `none`/`is_translated=0` then enqueue.
//!
//! Also exports `enqueue_article_translation_inner` which the import + full-text
//! trigger paths call directly (they already hold the DB lock and need the
//! non-Tauri signature).

use tauri::{Manager, State};

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::translation::language::should_skip_translation;
use crate::translation::worker::{TranslationJob, TranslationJobKind, TranslationWorkerHandle};

/// Choose the translation job kind based on whether the article has full text
/// attached. `FullText` when `has_full_text`, else `MetadataOnly`. Mirrors the
/// production rule used by `try_enqueue_translations_for_import`.
///
/// Returns `MetadataOnly` if the article cannot be read (non-fatal: the worker
/// still translates the metadata, which is better than dropping the job).
fn choose_job_kind(conn: &rusqlite::Connection, article_id: &str) -> TranslationJobKind {
    match article_repo::get_article_by_id(conn, article_id) {
        Ok(a) if a.has_full_text => TranslationJobKind::FullText,
        _ => TranslationJobKind::MetadataOnly,
    }
}

/// Enqueue gate logic, callable from non-command paths that already hold the DB
/// lock (import trigger, full-text attach trigger).
///
/// Per plan §Enqueueing:
/// - `is_translated = 1` → skip.
/// - `translation_status` in `queued`/`running`/`succeeded` → skip.
/// - `translation_status` in `none`/`failed` → write `queued` then send.
///
/// The `kind` is chosen by the caller: `MetadataOnly` for imports without full
/// text, `FullText` when full text is attached.
///
/// When `require_non_english` is true, the skip-policy gate
/// (`should_skip_translation`) is applied so English OR absent/blank language
/// articles are never enqueued. This is the safety gate the import/attach
/// triggers rely on; the manual command wrapper does NOT auto-check so the
/// user can retry if they believe the language metadata is wrong.
///
/// Returns `true` if the job was enqueued, `false` if skipped.
pub fn enqueue_article_translation_inner(
    conn: &rusqlite::Connection,
    worker: &TranslationWorkerHandle,
    article_id: &str,
    kind: TranslationJobKind,
    require_non_english: bool,
) -> Result<bool, AppError> {
    let status = article_repo::get_translation_status(conn, article_id)?;
    if status.is_translated {
        return Ok(false);
    }
    match status.translation_status.as_str() {
        "queued" | "running" | "succeeded" => return Ok(false),
        "none" | "failed" => {}
        _ => return Ok(false),
    }
    if require_non_english {
        let article = article_repo::get_article_by_id(conn, article_id)?;
        if should_skip_translation(article.language.as_deref()) {
            return Ok(false);
        }
    }
    article_repo::update_translation_status(conn, article_id, "queued")?;
    worker.try_send(TranslationJob { article_id: article_id.to_string(), kind })?;
    Ok(true)
}

/// Enqueue a translation job for an article.
///
/// The `trigger_source` parameter is recorded for diagnostics only ("manual",
/// "retry", "import", "attach"). The enqueue gate is applied: only `none`/
/// `failed` articles with `is_translated = 0` are enqueued; everything else is
/// silently skipped.
///
/// The job kind is chosen from the article's `has_full_text` flag so a manual
/// translate click on an article with full text attached translates the full
/// text + chunks, not just the metadata.
#[tauri::command]
pub fn enqueue_article_translation(
    db_state: State<'_, DbState>,
    worker: State<'_, TranslationWorkerHandle>,
    article_id: String,
    trigger_source: String,
) -> Result<bool, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    eprintln!("[translation] enqueue triggered by '{trigger_source}' for article {article_id}");
    let kind = choose_job_kind(&conn, &article_id);
    enqueue_article_translation_inner(&conn, worker.inner(), &article_id, kind, false)
}

/// Read the translation status snapshot for an article.
#[tauri::command]
pub fn get_translation_status(
    db_state: State<'_, DbState>,
    article_id: String,
) -> Result<article_repo::TranslationStatusInfo, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_translation_status(&conn, &article_id)
}

/// Retry a translation job: reset the article to `none`/`is_translated=0` then
/// enqueue. The enqueue gate sees `translation_status = 'none'` and sends the
/// job normally.
///
/// The job kind is chosen from the article's `has_full_text` flag so a retry
/// on an article with full text attached translates the full text + chunks.
#[tauri::command]
pub fn retry_translation_job(
    db_state: State<'_, DbState>,
    worker: State<'_, TranslationWorkerHandle>,
    article_id: String,
) -> Result<bool, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::reset_translation_status(&conn, &article_id)?;
    let kind = choose_job_kind(&conn, &article_id);
    enqueue_article_translation_inner(&conn, worker.inner(), &article_id, kind, false)
}

/// Shared helper for import + attach triggers: enqueue translation jobs for
/// every article whose `language` is non-English AND `auto_translate = true`
/// AND the enqueue gate accepts it. Non-fatal - errors are logged.
///
/// Chooses `FullText` when the article has full text attached, else
/// `MetadataOnly`.
pub fn try_enqueue_translations_for_import(
    app: &tauri::AppHandle,
    conn: &rusqlite::Connection,
    article_ids: &[String],
) {
    let auto = app_settings_repo::get_auto_translate(conn).unwrap_or(true);
    if !auto {
        return;
    }
    let worker = match app.try_state::<TranslationWorkerHandle>() {
        Some(w) => w,
        None => return,
    };
    for article_id in article_ids {
        // Choose FullText when the article has full text attached, else
        // MetadataOnly.
        let kind = match article_repo::get_article_by_id(conn, article_id) {
            Ok(a) if a.has_full_text => TranslationJobKind::FullText,
            _ => TranslationJobKind::MetadataOnly,
        };
        if let Err(e) =
            enqueue_article_translation_inner(conn, worker.inner(), article_id, kind, true)
        {
            eprintln!("[translation] failed to enqueue job for {article_id}: {e}");
        }
    }
}
