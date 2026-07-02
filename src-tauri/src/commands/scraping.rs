//! Tauri commands for the Citation Chaser scraping feature.
//!
//! These commands wrap the pure scraping logic with audit logging.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db::app_settings_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::scraping::citation_chaser::{clean_doi_filename, scrape_citation_chaser, ScrapeOptions};

/// Serializable result returned to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeResultDto {
    pub references_ris: Option<PathBuf>,
    pub citations_ris: Option<PathBuf>,
}

/// The RIS subdirectory name under the storage root.
const RIS_DIR_NAME: &str = "ris";

/// Compute the default RIS output directory: `~/Documents/Bango/ris/`.
fn compute_ris_output_dir() -> PathBuf {
    let docs = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    docs.join("Bango").join(RIS_DIR_NAME)
}

/// Resolve the effective RIS output directory (`{storage_root}/ris/`).
///
/// Delegates to [`app_settings_repo::get_storage_root`] (which performs the
/// one-time lazy migration from the legacy `fulltext_storage_dir` key) and
/// appends the `ris/` subdirectory, ensuring it exists.
fn resolve_ris_dir(conn: &rusqlite::Connection) -> PathBuf {
    let root = app_settings_repo::get_storage_root(conn)
        .map(PathBuf::from)
        .unwrap_or_else(|_| compute_ris_output_dir());
    let ris = root.join(RIS_DIR_NAME);
    // Best-effort creation; scraping will surface its own error if the dir
    // remains unwritable.
    let _ = std::fs::create_dir_all(&ris);
    ris
}

/// Scrape Citation Chaser for references and/or citations of a given DOI.
///
/// RIS files are saved to `{storage_root}/ris/` (default `~/Documents/Bango/ris/`).
/// Logs errors to the audit table so they appear in the Audit Timeline.
///
/// **Shortcut**: If the expected RIS files already exist in the output directory,
/// they are returned immediately without launching the headless browser.
///
/// This command is `async` and runs the heavy scraping work on a blocking thread
/// via `spawn_blocking` so the Tauri main thread stays responsive.
#[tauri::command]
pub async fn scrape_citation_chaser_cmd(
    app: AppHandle,
    doi: String,
    get_citations: Option<bool>,
    get_references: Option<bool>,
) -> Result<ScrapeResultDto, AppError> {
    let get_refs = get_references.unwrap_or(true);
    let get_cits = get_citations.unwrap_or(true);

    // Resolve output directory from app settings.
    let output_path = if let Some(db_state) = app.try_state::<DbState>() {
        if let Ok(conn) = db_state.conn.lock() {
            resolve_ris_dir(&conn)
        } else {
            compute_ris_output_dir()
        }
    } else {
        compute_ris_output_dir()
    };

    // ── Shortcut: check if RIS files already exist ──
    let safe_doi = clean_doi_filename(&doi);
    let refs_path = output_path.join(format!("{safe_doi}_references.ris"));
    let cits_path = output_path.join(format!("{safe_doi}_citations.ris"));

    let refs_exist = get_refs && refs_path.exists();
    let cits_exist = get_cits && cits_path.exists();

    if refs_exist && cits_exist {
        // Both needed files already cached - skip scraping entirely.
        return Ok(ScrapeResultDto {
            references_ris: get_refs.then_some(refs_path),
            citations_ris: get_cits.then_some(cits_path),
        });
    }

    // ── Scrape on a blocking thread to keep the UI responsive ──
    let options = ScrapeOptions { get_citations: get_cits, get_references: get_refs };
    let doi_clone = doi.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        scrape_citation_chaser(&doi_clone, &output_path, &options)
    })
    .await
    .map_err(|e| AppError::Scraping(format!("Scraping task panicked: {e}")))?;

    match result {
        Ok(scrape_result) => {
            // Log success to audit
            if let Some(db_state) = app.try_state::<DbState>() {
                if let Ok(conn) = db_state.conn.lock() {
                    let details = format!(
                        "Citation Chaser success scrape for DOI {}: refs={}, cites={}",
                        doi,
                        scrape_result.references_ris.is_some(),
                        scrape_result.citations_ris.is_some(),
                    );
                    let _ = audit_repo::log_error(&conn, &details);
                }
            }

            Ok(ScrapeResultDto {
                references_ris: scrape_result.references_ris,
                citations_ris: scrape_result.citations_ris,
            })
        }
        Err(err) => {
            // Log error to audit table
            if let Some(db_state) = app.try_state::<DbState>() {
                if let Ok(conn) = db_state.conn.lock() {
                    let _ = audit_repo::log_error(
                        &conn,
                        &format!("Citation Chaser scrape failed for DOI {doi}: {err}"),
                    );
                }
            }
            Err(AppError::Scraping(err.to_string()))
        }
    }
}
