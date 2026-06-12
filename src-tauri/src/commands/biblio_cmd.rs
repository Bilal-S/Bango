use crate::db::biblio_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::biblio::{
    BiblioAuthor, BiblioInstitution, BiblioKpis, BiblioStatus, BiblioTerm, YearCount,
};
// BiblioTerm is re-exported through biblio_repo — no direct use here
use serde::Serialize;
use tauri::Emitter;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeResult {
    pub authors: usize,
    pub terms: usize,
    pub status: BiblioStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalizeProgress {
    step: usize,
    total_steps: usize,
    message: String,
}

/// Run bibliometric normalization: extract and normalize all authors and terms
/// from the articles table into the biblio_* tables.
///
/// Uses a SQLite transaction to batch all writes into a single commit,
/// reducing disk I/O from thousands of individual writes to one batched commit.
#[tauri::command]
pub async fn biblio_normalize(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
) -> Result<NormalizeResult, AppError> {
    let mut conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let tx = conn.transaction()?;

    // Step 0: Start
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 0,
            total_steps: 6,
            message: "Starting normalization...".to_string(),
        },
    );

    // Clear previous normalization data (preserves AI-extracted and user-added terms)
    biblio_repo::clear_regeneratable_biblio(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 1,
            total_steps: 6,
            message: "Cleared stale bibliometric data".to_string(),
        },
    );

    // Extract and normalize authors from all articles
    let authors = biblio_repo::normalize_authors_from_articles(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 2,
            total_steps: 6,
            message: "Normalized author data".to_string(),
        },
    );

    // Parse raw affiliations → institutions + links
    let _affiliations = biblio_repo::normalize_affiliations(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 3,
            total_steps: 6,
            message: "Normalized author affiliations".to_string(),
        },
    );

    // Extract terms from article keywords, titles, and abstracts
    let terms = biblio_repo::normalize_terms_from_articles(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 4,
            total_steps: 6,
            message: "Normalized keywords and terms".to_string(),
        },
    );

    // Compute author metrics (citations, avg year, h-index)
    biblio_repo::compute_author_metrics(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 5,
            total_steps: 6,
            message: "Computed author metrics".to_string(),
        },
    );

    // Build coauthor edges (full counting + fractional counting)
    let _edges = biblio_repo::build_coauthor_edges(&tx)?;
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step: 6,
            total_steps: 6,
            message: "Built co-authorship networks".to_string(),
        },
    );

    let status = biblio_repo::get_biblio_status(&tx)?;

    tx.commit()?;

    Ok(NormalizeResult { authors, terms, status })
}

#[tauri::command]
pub async fn biblio_get_status(
    db_state: tauri::State<'_, DbState>,
) -> Result<BiblioStatus, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_biblio_status(&conn)
}

#[tauri::command]
pub async fn biblio_get_authors(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<BiblioAuthor>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_all_authors(&conn)
}

#[tauri::command]
pub async fn biblio_get_terms(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<BiblioTerm>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_all_terms(&conn)
}

#[tauri::command]
pub async fn biblio_get_coauthor_network(
    db_state: tauri::State<'_, DbState>,
) -> Result<serde_json::Value, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_coauthor_network_json(&conn)
}

#[tauri::command]
pub async fn biblio_get_kpis(db_state: tauri::State<'_, DbState>) -> Result<BiblioKpis, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_biblio_kpis(&conn)
}

#[tauri::command]
pub async fn biblio_get_author_institutions(
    db_state: tauri::State<'_, DbState>,
    author_id: String,
) -> Result<Vec<BiblioInstitution>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_institutions_by_author(&conn, &author_id)
}

#[tauri::command]
pub async fn biblio_get_unmatched_affiliation_count(
    db_state: tauri::State<'_, DbState>,
) -> Result<i32, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::count_unmatched_affiliations(&conn)
}

#[tauri::command]
pub async fn biblio_get_author_pubs_by_year(
    db_state: tauri::State<'_, DbState>,
    author_id: String,
) -> Result<Vec<YearCount>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_author_pubs_by_year(&conn, &author_id)
}
