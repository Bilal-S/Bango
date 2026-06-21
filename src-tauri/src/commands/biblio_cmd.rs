use crate::db::app_settings_repo;
use crate::db::biblio_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::biblio::{
    AuthorDetail, AuthorProductivityKpis, AuthorRank, BiblioAuthor, BiblioInstitution, BiblioKpis,
    BiblioStatus, BiblioTerm, YearCount,
};
// BiblioTerm is re-exported through biblio_repo - no direct use here
use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Total number of work steps reported by `biblio_normalize` via the
/// `biblio:progress` event. Kept in sync with the emit calls below.
const BIBLIO_NORMALIZE_TOTAL_STEPS: usize = 8;

/// Parameters for the co-citation network command.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CocitationParams {
    /// Scope: "included" (default) or "all".
    pub scope: Option<String>,
    /// Normalization mode: "raw", "cosine" (default), "jaccard", or "pearson".
    pub normalization: Option<String>,
    /// Minimum times a paper must be cited to be included. Default 2.
    pub min_citation_count: Option<i32>,
    /// Minimum co-citation count for an edge. Default 2.
    pub min_co_citation: Option<i32>,
}

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

/// Emit a `biblio:progress` event with the canonical total step count.
fn emit_progress(app_handle: &tauri::AppHandle, step: usize, message: &str) {
    let _ = app_handle.emit(
        "biblio:progress",
        NormalizeProgress {
            step,
            total_steps: BIBLIO_NORMALIZE_TOTAL_STEPS,
            message: message.to_string(),
        },
    );
}

/// Run bibliometric normalization: extract and normalize all authors and terms
/// from the articles table into the biblio_* tables.
///
/// Delegates the DB work to `biblio_repo::run_full_normalization` (which wraps
/// the 8-step pipeline in a single transaction) and emits progress events
/// around it. The shared pure function is also called by the wiki ingest path
/// so multi-batch wiki ingest does not depend on the user having visited the
/// Bibliometrics dashboard first.
///
/// Clears the `biblio_needs_refresh` flag once the transaction commits
/// successfully.
#[tauri::command]
pub async fn biblio_normalize(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
) -> Result<NormalizeResult, AppError> {
    let mut conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    emit_progress(&app_handle, 0, "Starting normalization...");
    let (authors, terms) = biblio_repo::run_full_normalization(&mut conn)?;
    emit_progress(&app_handle, BIBLIO_NORMALIZE_TOTAL_STEPS, "Built citation network");

    let status = biblio_repo::get_biblio_status(&conn)?;

    // Only clear the refresh flag after the transaction commits successfully.
    app_settings_repo::clear_biblio_needs_refresh(&conn);

    Ok(NormalizeResult { authors, terms, status })
}

/// Whether bibliometric data is stale and should be re-normalized on the next
/// visit to the Bibliometrics dashboard.
#[tauri::command]
pub async fn biblio_get_needs_refresh(
    db_state: tauri::State<'_, DbState>,
) -> Result<bool, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    app_settings_repo::get_biblio_needs_refresh(&conn)
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

#[tauri::command]
pub async fn biblio_get_citation_network(
    db_state: tauri::State<'_, DbState>,
    include_unmatched: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_citation_network_json(&conn, include_unmatched.unwrap_or(false))
}

#[tauri::command]
pub async fn biblio_get_keyword_network(
    db_state: tauri::State<'_, DbState>,
    sources: Vec<String>,
    min_occurrences: Option<i32>,
    min_cooccurrence: Option<i32>,
) -> Result<serde_json::Value, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_keyword_network_json(
        &conn,
        &sources,
        min_occurrences.unwrap_or(1),
        min_cooccurrence.unwrap_or(1),
    )
}

#[tauri::command]
pub async fn biblio_get_author_rankings(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<AuthorRank>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_author_rankings(&conn)
}

#[tauri::command]
pub async fn biblio_get_author_detail(
    db_state: tauri::State<'_, DbState>,
    author_id: String,
) -> Result<AuthorDetail, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_author_detail(&conn, &author_id)
}

#[tauri::command]
pub async fn biblio_get_author_productivity_kpis(
    db_state: tauri::State<'_, DbState>,
) -> Result<AuthorProductivityKpis, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    biblio_repo::get_author_productivity_kpis(&conn)
}

/// Get the co-citation network as JSON for graph rendering.
///
/// Computes co-citation on-demand from `article_reference_links` (type=1).
/// All four normalization modes (raw, cosine, jaccard, pearson) are computed
/// and returned in each edge; the frontend selects which to visualize.
#[tauri::command]
pub async fn biblio_get_cocitation_network(
    db_state: tauri::State<'_, DbState>,
    params: Option<CocitationParams>,
) -> Result<serde_json::Value, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let p = params.unwrap_or_default();
    let scope = match p.scope.as_deref().unwrap_or("included") {
        "all" => biblio_repo::CocitationScope::AllArticles,
        _ => biblio_repo::CocitationScope::IncludedArticles,
    };
    let normalization = match p.normalization.as_deref().unwrap_or("cosine") {
        "raw" => biblio_repo::CocitationNormalization::Raw,
        "jaccard" => biblio_repo::CocitationNormalization::Jaccard,
        "pearson" => biblio_repo::CocitationNormalization::Pearson,
        _ => biblio_repo::CocitationNormalization::Cosine,
    };

    biblio_repo::get_cocitation_network_json(
        &conn,
        scope,
        normalization,
        p.min_citation_count.unwrap_or(2),
        p.min_co_citation.unwrap_or(2),
    )
}
