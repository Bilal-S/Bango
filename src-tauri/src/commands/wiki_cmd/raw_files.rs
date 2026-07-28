//! Raw-file add/list/export commands.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{raw_export, storage};

use serde::Serialize;

/// Prepare raw sources: export included articles AND process user-dropped files.
/// Runs both on-ramps in sequence. Idempotent.
#[tauri::command]
pub fn wiki_export_raw(
    db_state: tauri::State<'_, DbState>,
) -> Result<raw_export::RawExportReport, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let report = raw_export::prepare_all(&conn, &root)?;
    Ok(report)
}

/// Add a user-selected file to `raw/` and extract its companion `.md` immediately.
/// Returns the companion `.md` path.
#[tauri::command]
pub fn wiki_add_raw_file(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
) -> Result<String, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let companion = raw_export::add_user_file(&root, std::path::Path::new(&file_path))?;
    Ok(companion.to_string_lossy().to_string())
}

/// Fetch a URL, extract its text content, and add it as a wiki raw source.
#[tauri::command]
pub async fn wiki_add_raw_url(
    db_state: tauri::State<'_, DbState>,
    url: String,
) -> Result<String, AppError> {
    // Derive a title from the last path segment or host.
    let title = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && !s.contains(':'))
        .unwrap_or("web-page")
        .to_string();

    // Fetch the page.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; BangoWiki/1.0)")
        .build()
        .map_err(|e| AppError::Import(format!("HTTP client error: {e}")))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("Failed to fetch '{url}': {e}")))?;
    let html = response
        .text()
        .await
        .map_err(|e| AppError::Import(format!("Failed to read response from '{url}': {e}")))?;

    // Resolve wiki root and add the content.
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let companion = raw_export::add_raw_content(&root, &title, &html, &url)?;
    Ok(companion.to_string_lossy().to_string())
}

/// A raw file entry for the listing command.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFileEntry {
    pub path: String,
    pub title: String,
    pub slug: String,
    pub source_kind: String,
    pub source_file: Option<String>,
    pub status: String,
}

/// List all `.md` files in `raw/` with parsed metadata.
#[tauri::command]
pub fn wiki_list_raw_files(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<RawFileEntry>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let files = raw_export::list_raw_files(&root)?;
    let entries = files
        .into_iter()
        .map(|(path, fm)| RawFileEntry {
            path: path.to_string_lossy().to_string(),
            title: fm.get("title").unwrap_or("Untitled").to_string(),
            slug: fm.get("slug").unwrap_or("").to_string(),
            source_kind: fm.get("source_kind").unwrap_or("").to_string(),
            source_file: fm.get("source_file").map(str::to_string),
            status: fm.get("status").unwrap_or("draft").to_string(),
        })
        .collect();
    Ok(entries)
}
