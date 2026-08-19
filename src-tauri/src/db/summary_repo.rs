use rusqlite::Connection;
use serde::Serialize;

use crate::db::saved_report::{self, SavedReportTable};
use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSummary {
    pub summary_text: String,
    pub citation_style: String,
    pub generated_at: String,
}

/// Single-row `summary` table identifiers for the shared saved-report core.
const TABLE: SavedReportTable = SavedReportTable { table: "summary", text_column: "summary_text" };

pub fn save_summary(
    conn: &Connection,
    summary_text: &str,
    citation_style: &str,
    generated_at: &str,
) -> Result<(), AppError> {
    saved_report::save(conn, &TABLE, summary_text, citation_style, generated_at)
}

pub fn get_summary(conn: &Connection) -> Result<Option<SavedSummary>, AppError> {
    Ok(saved_report::get(conn, &TABLE)?.map(|r| SavedSummary {
        summary_text: r.text,
        citation_style: r.citation_style,
        generated_at: r.generated_at,
    }))
}

pub fn clear_summary(conn: &Connection) -> Result<(), AppError> {
    saved_report::clear(conn, &TABLE)
}
