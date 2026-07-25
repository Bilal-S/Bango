use base64::Engine;
use serde::Deserialize;
use tauri::State;

use std::collections::HashMap;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::rebuild;
use crate::error::AppError;
use crate::export::project;
use crate::export::ris_writer::{articles_to_ris, RisExportArticle};
use crate::wiki::storage;

/// Helper: convert articles into RIS string, resolving criteria labels.
fn articles_to_ris_export(
    articles: &[crate::models::article::Article],
    criteria_map: &HashMap<String, String>,
) -> String {
    let resolve_criteria = |ids: &[String]| -> Vec<String> {
        ids.iter().filter_map(|id| criteria_map.get(id).cloned()).collect()
    };

    let export_articles: Vec<RisExportArticle> = articles
        .iter()
        .map(|a| RisExportArticle {
            reference_type: a.reference_type.clone(),
            title: a.title.clone(),
            abstract_text: a.abstract_text.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
            journal: a.journal.clone(),
            volume: a.volume.clone(),
            issue: a.issue.clone(),
            start_page: a.start_page.clone(),
            end_page: a.end_page.clone(),
            keywords: a.keywords.clone(),
            tags: a.tags.clone(),
            url: a.url.clone(),
            language: a.language.clone(),
            publisher: a.publisher.clone(),
            issn: a.issn.clone(),
            notes: a.notes.clone(),
            ai_reasoning: a.ai_reasoning.clone(),
            user_notes: a.user_notes.clone(),
            ai_decision: a.ai_decision.as_ref().map(|d| d.as_str().to_string()),
            labels: a.labels.clone(),
            matched_inclusion_criteria: resolve_criteria(&a.matched_inclusion_criteria),
            matched_exclusion_criteria: resolve_criteria(&a.matched_exclusion_criteria),
        })
        .collect();

    articles_to_ris(&export_articles)
}

/// Build the criteria lookup map (id -> text).
fn build_criteria_map(conn: &rusqlite::Connection) -> Result<HashMap<String, String>, AppError> {
    Ok(criteria_repo::get_all_criteria(conn)?.into_iter().map(|c| (c.id, c.text)).collect())
}

#[tauri::command]
pub fn export_ris(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let articles = article_repo::get_articles_by_status(&conn, "included")?;
    let criteria_map = build_criteria_map(&conn)?;
    Ok(articles_to_ris_export(&articles, &criteria_map))
}

#[tauri::command]
pub fn export_ris_to_file(db_state: State<'_, DbState>, path: String) -> Result<(), AppError> {
    let content = export_ris(db_state)?;
    std::fs::write(path, content).map_err(AppError::Io)
}

#[tauri::command]
pub fn export_ris_for_tab_to_file(
    db_state: State<'_, DbState>,
    path: String,
    status: String,
    screening_errors_only: bool,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let articles = article_repo::get_articles_for_export(&conn, &status, screening_errors_only)?;
    let criteria_map = build_criteria_map(&conn)?;
    let content = articles_to_ris_export(&articles, &criteria_map);
    std::fs::write(&path, content).map_err(AppError::Io)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProjectRequest {}

#[tauri::command]
pub fn export_project_backup(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    project::export_project(&conn)
}

#[tauri::command]
pub fn export_project_to_file(db_state: State<'_, DbState>, path: String) -> Result<(), AppError> {
    let content = export_project_backup(db_state)?;
    std::fs::write(path, content).map_err(AppError::Io)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProjectRequest {
    pub json_content: String,
}

#[tauri::command]
pub async fn import_project_backup(
    db_state: State<'_, DbState>,
    request: ImportProjectRequest,
) -> Result<(), AppError> {
    eprintln!(
        "[import_project_backup] Command received, content length: {}",
        request.json_content.len()
    );
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    eprintln!("[import_project_backup] DB lock acquired, calling import_project...");
    let result = project::import_project(&conn, &request.json_content);
    match &result {
        Ok(()) => {
            eprintln!("[import_project_backup] Import succeeded");
            // Imported articles affect bibliometrics - mark the corpus as stale so
            // the frontend auto-runs `biblio_normalize` when includedCount > 0.
            // This mirrors the behavior of RIS import (`commands::import`) and
            // status-change commands, which all call `mark_biblio_needs_refresh`.
            // Non-fatal: if the settings write fails, the user can still trigger
            // normalization manually from the bibliometrics dashboard.
            crate::db::app_settings_repo::mark_biblio_needs_refresh(&conn);
        }
        Err(e) => eprintln!("[import_project_backup] Import failed: {:?}", e),
    }
    result
}

#[tauri::command]
pub fn write_text_to_file(path: String, content: String) -> Result<(), AppError> {
    std::fs::write(path, content).map_err(AppError::Io)
}

#[tauri::command]
pub fn write_base64_to_file(path: String, data: String) -> Result<(), AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes).map_err(AppError::Io)
}

/// Reset the project: rebuild the DB schema (dropping all user tables) and
/// delete the on-disk wiki-root directory. `journal_index` is preserved by
/// `rebuild_schema`. The wiki root must be resolved BEFORE the schema rebuild
/// because `app_settings` (which holds `storage_root` and the optional
/// `wiki_root_dir` override) is dropped by `rebuild_schema`.
///
/// Wiki deletion is **non-fatal**: if resolving or deleting the wiki root
/// fails, the DB reset still succeeds (the error is logged to stderr). The
/// user can delete the directory manually if needed.
///
/// After the rebuild, `journal_index` is checked and reloaded from the bundled
/// portal DB if empty (transparently heals the case where the startup auto-load
/// silently failed on Windows). This reload is **blocking**: the schema rebuild
/// has already committed, so a load failure does NOT undo the reset, but the
/// error is written to the audit table and returned to the frontend so the user
/// sees a Toast and knows journal matching is degraded.
///
/// Extracted from the `reset_project` command wrapper so it can be tested with
/// a plain `&mut Connection` (no Tauri state required). The journal-index
/// post-check is in the command wrapper because it needs the `AppHandle` to
/// resolve the bundled resource path.
pub fn reset_project_inner(conn: &mut rusqlite::Connection) -> Result<(), AppError> {
    // 1. Resolve the wiki root while `app_settings` is still available.
    //    `resolve_root` also ensures the dir exists (creating an empty one if
    //    missing), which is harmless: we delete it next.
    let wiki_root = storage::resolve_root(conn).map_err(|e| {
        eprintln!("[reset_project] failed to resolve wiki root: {e}");
        e
    });

    // 2. Rebuild the DB schema (drops all user tables + wiki_pages_fts,
    //    resets user_version, re-runs migrations). journal_index is preserved.
    rebuild::rebuild_schema(conn)?;

    // 3. Delete the on-disk wiki-root directory (non-fatal on failure).
    if let Ok(root) = &wiki_root {
        if let Err(e) = storage::delete_wiki_root(root) {
            eprintln!("[reset_project] failed to delete wiki root '{}': {e}", root.display());
        }
    }

    Ok(())
}

#[tauri::command]
pub fn reset_project(app: tauri::AppHandle, db_state: State<'_, DbState>) -> Result<(), AppError> {
    let mut conn = crate::db::connection::lock_conn(&db_state.conn)?;

    reset_project_inner(&mut conn)?;

    // Post-reset: ensure the journal_index is populated. If the startup
    // auto-load silently failed (e.g. a Windows `resource_dir()` resolution
    // bug), `rebuild_schema` preserved an empty table. Reload it now so the
    // user starts the fresh project with a working journal index. Blocking;
    // a failure writes an audit error and returns Err so the frontend Toasts.
    if let Err(e) = crate::ensure_journal_index_populated(&conn, &app) {
        let msg = format!("Failed to reload journal index after reset: {e}");
        eprintln!("[reset_project] {msg}");
        let _ = crate::db::audit_repo::log_error(&conn, &msg);
        return Err(AppError::Import(msg));
    }

    Ok(())
}
