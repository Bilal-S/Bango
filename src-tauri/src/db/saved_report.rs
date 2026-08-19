//! Shared single-row saved-report repo core.
//!
//! `summary` and `gap_analysis` are separate tables sharing one contract
//! (spec §10.2: single-row, wiped by `reset_project`, excluded from
//! `ProjectBackup`). The table/column identifiers below are compile-time
//! constants supplied by the repo wrappers, never user input, so composing
//! the SQL strings here is safe; all values stay bound parameters.

use rusqlite::{params, Connection};

use crate::error::AppError;

/// Identifiers of one single-row report table (`id = 1`).
pub struct SavedReportTable {
    pub table: &'static str,
    /// Column holding the report text (`summary_text` / `gap_text`).
    pub text_column: &'static str,
}

/// Raw payload of one saved report row.
#[derive(Debug, Clone)]
pub struct SavedReport {
    pub text: String,
    pub citation_style: String,
    pub generated_at: String,
}

/// Upsert the `id = 1` row (update when present, insert otherwise).
///
/// Single atomic `INSERT ... ON CONFLICT` statement: no read-then-write race
/// and no silent error swallowing, with the same observable behavior as the
/// historical COUNT-then-UPDATE/INSERT pair (pinned by the upsert tests).
pub fn save(
    conn: &Connection,
    table: &SavedReportTable,
    text: &str,
    citation_style: &str,
    generated_at: &str,
) -> Result<(), AppError> {
    conn.execute(
        &format!(
            "INSERT INTO {} (id, {}, citation_style, generated_at) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT (id) DO UPDATE SET
             {} = excluded.{}, citation_style = excluded.citation_style,
             generated_at = excluded.generated_at",
            table.table, table.text_column, table.text_column, table.text_column
        ),
        params![text, citation_style, generated_at],
    )?;

    Ok(())
}

/// Fetch the `id = 1` row, or `None` when absent.
pub fn get(conn: &Connection, table: &SavedReportTable) -> Result<Option<SavedReport>, AppError> {
    let result = conn.query_row(
        &format!(
            "SELECT {}, citation_style, generated_at FROM {} WHERE id = 1",
            table.text_column, table.table
        ),
        [],
        |row| {
            Ok(SavedReport {
                text: row.get(0)?,
                citation_style: row.get(1)?,
                generated_at: row.get(2)?,
            })
        },
    );

    match result {
        Ok(report) => Ok(Some(report)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Delete the `id = 1` row.
pub fn clear(conn: &Connection, table: &SavedReportTable) -> Result<(), AppError> {
    conn.execute(&format!("DELETE FROM {} WHERE id = 1", table.table), [])?;
    Ok(())
}
