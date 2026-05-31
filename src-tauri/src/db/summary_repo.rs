use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSummary {
    pub summary_text: String,
    pub citation_style: String,
    pub generated_at: String,
}

pub fn save_summary(
    conn: &Connection,
    summary_text: &str,
    citation_style: &str,
    generated_at: &str,
) -> Result<(), AppError> {
    // Upsert: if row exists, update; otherwise insert
    let exists: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM summary WHERE id = 1", [], |row| row.get(0))
        .unwrap_or(false);

    if exists {
        conn.execute(
            "UPDATE summary SET summary_text = ?1, citation_style = ?2, generated_at = ?3 WHERE id = 1",
            rusqlite::params![summary_text, citation_style, generated_at],
        )?;
    } else {
        conn.execute(
            "INSERT INTO summary (id, summary_text, citation_style, generated_at) VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![summary_text, citation_style, generated_at],
        )?;
    }

    Ok(())
}

pub fn get_summary(conn: &Connection) -> Result<Option<SavedSummary>, AppError> {
    let mut stmt = conn
        .prepare("SELECT summary_text, citation_style, generated_at FROM summary WHERE id = 1")?;

    let result = stmt.query_row([], |row| {
        Ok(SavedSummary {
            summary_text: row.get(0)?,
            citation_style: row.get(1)?,
            generated_at: row.get(2)?,
        })
    });

    match result {
        Ok(summary) => Ok(Some(summary)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::Database(e)),
    }
}

pub fn clear_summary(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM summary WHERE id = 1", [])?;
    Ok(())
}
