use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppError;

/// persisted Research Gap Analysis report (single-row, mirrors `SavedSummary`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedGapAnalysis {
    pub gap_text: String,
    pub citation_style: String,
    pub generated_at: String,
}

/// Upsert the gap report. Single-row table (`id = 1 CHECK`), same shape as
/// `summary_repo::save_summary`.
pub fn save_gap_analysis(
    conn: &Connection,
    gap_text: &str,
    citation_style: &str,
    generated_at: &str,
) -> Result<(), AppError> {
    let exists: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM gap_analysis WHERE id = 1", [], |row| row.get(0))
        .unwrap_or(false);

    if exists {
        conn.execute(
            "UPDATE gap_analysis SET gap_text = ?1, citation_style = ?2, generated_at = ?3 WHERE id = 1",
            rusqlite::params![gap_text, citation_style, generated_at],
        )?;
    } else {
        conn.execute(
            "INSERT INTO gap_analysis (id, gap_text, citation_style, generated_at) VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![gap_text, citation_style, generated_at],
        )?;
    }

    Ok(())
}

pub fn get_gap_analysis(conn: &Connection) -> Result<Option<SavedGapAnalysis>, AppError> {
    let mut stmt = conn
        .prepare("SELECT gap_text, citation_style, generated_at FROM gap_analysis WHERE id = 1")?;

    let result = stmt.query_row([], |row| {
        Ok(SavedGapAnalysis {
            gap_text: row.get(0)?,
            citation_style: row.get(1)?,
            generated_at: row.get(2)?,
        })
    });

    match result {
        Ok(gap) => Ok(Some(gap)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn clear_gap_analysis(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM gap_analysis WHERE id = 1", [])?;
    Ok(())
}
