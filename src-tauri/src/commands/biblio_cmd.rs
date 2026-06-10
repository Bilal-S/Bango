use crate::db::biblio_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::biblio::{BiblioAuthor, BiblioKpis, BiblioStatus, BiblioTerm};
// BiblioTerm is re-exported through biblio_repo — no direct use here
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeResult {
    pub authors: usize,
    pub terms: usize,
    pub status: BiblioStatus,
}

/// Run bibliometric normalization: extract and normalize all authors and terms
/// from the articles table into the biblio_* tables.
#[tauri::command]
pub fn biblio_normalize(db_state: tauri::State<'_, DbState>) -> Result<NormalizeResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // Clear previous normalization data
    biblio_repo::clear_all_biblio(&conn)?;

    // Extract and normalize authors from all articles
    let authors = biblio_repo::normalize_authors_from_articles(&conn)?;

    // Extract terms from article keywords, titles, and abstracts
    let terms = biblio_repo::normalize_terms_from_articles(&conn)?;

    // Build coauthor edges
    let _edges = biblio_repo::build_coauthor_edges(&conn)?;

    let status = biblio_repo::get_biblio_status(&conn)?;

    Ok(NormalizeResult { authors, terms, status })
}

#[tauri::command]
pub fn biblio_get_status(db_state: tauri::State<'_, DbState>) -> Result<BiblioStatus, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_biblio_status(&conn)
}

#[tauri::command]
pub fn biblio_get_authors(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<BiblioAuthor>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_all_authors(&conn)
}

#[tauri::command]
pub fn biblio_get_terms(db_state: tauri::State<'_, DbState>) -> Result<Vec<BiblioTerm>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_all_terms(&conn)
}

#[tauri::command]
pub fn biblio_get_coauthor_network(
    db_state: tauri::State<'_, DbState>,
) -> Result<serde_json::Value, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_coauthor_network_json(&conn)
}

#[tauri::command]
pub fn biblio_get_kpis(db_state: tauri::State<'_, DbState>) -> Result<BiblioKpis, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_biblio_kpis(&conn)
}
