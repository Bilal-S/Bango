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
use crate::scraping::citation_chaser::{scrape_citation_chaser, ScrapeOptions};

/// Serializable result returned to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeResultDto {
    pub references_ris: Option<PathBuf>,
    pub citations_ris: Option<PathBuf>,
}

/// Compute the RIS output directory: `~/Documents/Bango/ris/`.
///
/// Follows the same convention as `fulltext_storage_dir` (see `commands/app_settings.rs`).
fn compute_ris_output_dir() -> PathBuf {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    docs.join("Bango").join("ris")
}

/// Resolve the effective RIS output directory.
///
/// If the user has configured a custom `fulltext_storage_dir`, place the `ris/`
/// subfolder next to it (sibling of `fulltext/`). Otherwise use the default
/// `~/Documents/Bango/ris/`.
fn resolve_ris_dir(conn: &rusqlite::Connection) -> PathBuf {
    let base = app_settings_repo::get_setting(conn, "fulltext_storage_dir")
        .ok()
        .flatten()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let docs = dirs::document_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("."));
            docs.join("Bango")
        });

    // If the configured path ends with `/fulltext`, go one level up so `ris/`
    // is a sibling. Otherwise use the base directly.
    let base = if base.file_name().is_some_and(|n| n == "fulltext") {
        base.parent().unwrap_or(&base).to_path_buf()
    } else {
        base
    };

    base.join("ris")
}

/// Scrape Citation Chaser for references and/or citations of a given DOI.
///
/// RIS files are saved to `~/Documents/Bango/ris/` (or the configured Bango
/// directory). Logs errors to the audit table so they appear in the Audit Timeline.
#[tauri::command]
pub fn scrape_citation_chaser_cmd(
    app: AppHandle,
    doi: String,
    get_citations: Option<bool>,
    get_references: Option<bool>,
) -> Result<ScrapeResultDto, AppError> {
    let options = ScrapeOptions {
        get_citations: get_citations.unwrap_or(true),
        get_references: get_references.unwrap_or(true),
    };

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

    match scrape_citation_chaser(&doi, &output_path, &options) {
        Ok(result) => {
            // Log success to audit
            if let Some(db_state) = app.try_state::<DbState>() {
                if let Ok(conn) = db_state.conn.lock() {
                    let details = format!(
                        "Citation Chaser scrape for DOI {}: refs={}, cites={}",
                        doi,
                        result.references_ris.is_some(),
                        result.citations_ris.is_some(),
                    );
                    let _ = audit_repo::log_error(&conn, &details);
                }
            }

            Ok(ScrapeResultDto {
                references_ris: result.references_ris,
                citations_ris: result.citations_ris,
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