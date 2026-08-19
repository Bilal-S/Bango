use rusqlite::Connection;
use serde::Serialize;

use crate::db::saved_report::{self, SavedReportTable};
use crate::error::AppError;

/// persisted Research Gap Analysis report (single-row, mirrors `SavedSummary`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedGapAnalysis {
    pub gap_text: String,
    pub citation_style: String,
    pub generated_at: String,
}

/// Single-row `gap_analysis` table identifiers for the shared saved-report core.
const TABLE: SavedReportTable = SavedReportTable { table: "gap_analysis", text_column: "gap_text" };

/// Upsert the gap report. Single-row table (`id = 1 CHECK`), same shape as
/// `summary_repo::save_summary`.
pub fn save_gap_analysis(
    conn: &Connection,
    gap_text: &str,
    citation_style: &str,
    generated_at: &str,
) -> Result<(), AppError> {
    saved_report::save(conn, &TABLE, gap_text, citation_style, generated_at)
}

pub fn get_gap_analysis(conn: &Connection) -> Result<Option<SavedGapAnalysis>, AppError> {
    Ok(saved_report::get(conn, &TABLE)?.map(|r| SavedGapAnalysis {
        gap_text: r.text,
        citation_style: r.citation_style,
        generated_at: r.generated_at,
    }))
}

pub fn clear_gap_analysis(conn: &Connection) -> Result<(), AppError> {
    saved_report::clear(conn, &TABLE)
}
