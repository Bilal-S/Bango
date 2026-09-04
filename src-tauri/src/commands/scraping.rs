//! Tauri commands for the Citation Chaser scraping feature.
//!
//! These commands wrap the pure scraping logic with audit logging and the
//! cancellation-token plumbing.
//!
//! # Cancellation contract
//!
//! [`scrape_citation_chaser_cmd`] creates a fresh [`CancelToken`] per call,
//! stores it in the managed [`ScrapingState`] for the duration of the
//! `spawn_blocking` call, and clears the slot in a `finally`-shaped guard. The
//! frontend's [`cancel_scraping`] command signals the active token; the
//! in-flight scrape returns [`crate::scraping::citation_chaser::ScrapeError::Cancelled`]
//! within ~1s (one `POLL_INTERVAL_MS` tick).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::app_settings_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::scraping::citation_chaser::{
    clean_doi_filename, find_file_case_insensitive, scrape_citation_chaser, CancelToken,
    ScrapeOptions,
};

/// Serializable result returned to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeResultDto {
    pub references_ris: Option<PathBuf>,
    pub citations_ris: Option<PathBuf>,
}

/// Managed state holding the currently-active scrape's [`CancelToken`], if any.
///
/// `Some` while a scrape is in flight; the frontend's [`cancel_scraping`]
/// command calls `.cancel()` on it. The slot is cleared when the scrape
/// returns (success, error, or cancel).
#[derive(Default)]
pub struct ScrapingState {
    active: Mutex<Option<CancelToken>>,
}

impl ScrapingState {
    /// Lock the active-token slot, recovering from poison by taking the inner
    /// guard. A poisoned mutex here means a panic occurred while a prior
    /// `set_active`/`clear_active`/`cancel_active` held the lock; the slot is
    /// still readable/writable, and the cancel contract is best-effort anyway
    /// (the frontend's between-articles flag is the authoritative stop signal),
    /// so we recover rather than propagate.
    fn lock_active(&self) -> std::sync::MutexGuard<'_, Option<CancelToken>> {
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Install `token` as the active scrape token. Returns the previously
    /// active token (if any) so the caller could, in principle, chain them.
    fn set_active(&self, token: CancelToken) -> Option<CancelToken> {
        self.lock_active().replace(token)
    }

    /// Clear the active token slot. Called when the scrape returns.
    fn clear_active(&self) {
        *self.lock_active() = None;
    }

    /// Signal cancellation to the active token, if one is present.
    fn cancel_active(&self) {
        if let Some(token) = self.lock_active().as_ref() {
            token.cancel();
        }
    }
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

/// Classify a rendered `ScrapeError` string as a "skip" (NoData / Cancelled)
/// rather than a true error. The frontend uses the same prefix check to route
/// the toast into the batch's `skipped` counter and show an info toast.
///
/// Kept in sync with `ScrapeError::NoData` ("No data: ...") and
/// `ScrapeError::Cancelled` ("Cancelled").
fn is_skip_message(err_str: &str) -> bool {
    err_str.starts_with("No data:") || err_str == "Cancelled"
}

/// Scrape Citation Chaser for references and/or citations of a given DOI.
///
/// RIS files are saved to `{storage_root}/ris/`. Logs errors to the audit
/// table so they appear in the Audit Timeline.
///
/// Shortcut: if the expected RIS files already exist in the output directory,
/// they are returned immediately without launching the headless browser.
///
/// This command is `async` and runs the heavy scraping work on a blocking
/// thread via `spawn_blocking` so the Tauri main thread stays responsive.
#[tauri::command]
pub async fn scrape_citation_chaser_cmd(
    app: AppHandle,
    doi: String,
    get_citations: Option<bool>,
    get_references: Option<bool>,
) -> Result<ScrapeResultDto, AppError> {
    let get_refs = get_references.unwrap_or(true);
    let get_cits = get_citations.unwrap_or(true);

    // Resolve output directory from app settings. Tolerate a poisoned mutex
    // by falling back to the default directory; scraping will surface its own
    // error if the resolved path is not writable.
    let output_path = if let Some(db_state) = app.try_state::<DbState>() {
        match crate::db::connection::lock_conn(&db_state.conn) {
            Ok(conn) => resolve_ris_dir(&conn),
            Err(_) => compute_ris_output_dir(),
        }
    } else {
        compute_ris_output_dir()
    };

    // ── Shortcut: check if RIS files already exist ──
    // Probe case-insensitively (legacy files may carry mixed-case DOI names)
    // and lowercase the DOI first so non-canonical callers still resolve.
    let safe_doi = clean_doi_filename(&doi.to_lowercase());
    let refs_path = find_file_case_insensitive(&output_path, &format!("{safe_doi}_references.ris"));
    let cits_path = find_file_case_insensitive(&output_path, &format!("{safe_doi}_citations.ris"));

    let refs_exist = get_refs && refs_path.is_some();
    let cits_exist = get_cits && cits_path.is_some();

    if refs_exist && cits_exist {
        // Both needed files already cached - skip scraping entirely.
        return Ok(ScrapeResultDto { references_ris: refs_path, citations_ris: cits_path });
    }

    // ── Install the cancel token for the duration of the scrape ──
    let cancel = CancelToken::new();
    if let Some(state) = app.try_state::<ScrapingState>() {
        state.set_active(cancel.clone());
    }

    // ── Scrape on a blocking thread to keep the UI responsive ──
    let options = ScrapeOptions { get_citations: get_cits, get_references: get_refs };
    let doi_clone = doi.clone();
    let cancel_clone = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        scrape_citation_chaser(&doi_clone, &output_path, &options, &cancel_clone)
    })
    .await
    .map_err(|e| AppError::Scraping(format!("Scraping task panicked: {e}")))?;

    // Always clear the active token slot, regardless of outcome.
    if let Some(state) = app.try_state::<ScrapingState>() {
        state.clear_active();
    }

    match result {
        Ok(scrape_result) => {
            // Log success to audit (best-effort; the scrape itself succeeded).
            if let Some(db_state) = app.try_state::<DbState>() {
                let details = format!(
                    "Citation Chaser success scrape for DOI {}: refs={}, cites={}",
                    doi,
                    scrape_result.references_ris.is_some(),
                    scrape_result.citations_ris.is_some(),
                );
                audit_repo::log_error_best_effort(&db_state.conn, &details);
            }

            Ok(ScrapeResultDto {
                references_ris: scrape_result.references_ris,
                citations_ris: scrape_result.citations_ris,
            })
        }
        Err(err) => {
            let err_str = err.to_string();
            // Log to audit table (best-effort; the real error is returned).
            // Skip-classified outcomes (NoData/Cancelled) are logged so they
            // appear in the Audit Timeline / Diagnostics, but the frontend
            // routes them as info toasts rather than errors.
            if let Some(db_state) = app.try_state::<DbState>() {
                let label = if is_skip_message(&err_str) { "skipped" } else { "failed" };
                let details = format!("Citation Chaser {label} for DOI {doi}: {err_str}",);
                audit_repo::log_error_best_effort(&db_state.conn, &details);
            }
            Err(AppError::Scraping(err_str))
        }
    }
}

/// Cancel any in-flight Citation Chaser scrape.
///
/// Signals the active [`CancelToken`] (if any) so the `spawn_blocking` scrape
/// returns [`ScrapeError::Cancelled`] within ~1s. Safe to call when no scrape
/// is running (no-op).
#[tauri::command]
pub fn cancel_scraping(state: State<'_, ScrapingState>) -> Result<(), AppError> {
    state.cancel_active();
    Ok(())
}
