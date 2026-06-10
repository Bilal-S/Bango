use rusqlite::Connection;

use crate::error::AppError;

/// Convenience wrapper: resolve a journal_index id from ISSN/eISSN/name.
/// Returns `None` if no match found (never errors).
pub fn resolve_journal_id(
    conn: &Connection,
    issn: Option<&str>,
    eissn: Option<&str>,
    journal_name: Option<&str>,
) -> Option<String> {
    match_journal(conn, issn, eissn, journal_name).ok().flatten()
}

/// Look up a journal_index row by ISSN, eISSN, or journal name.
/// Returns the journal_index `id` if a match is found.
///
/// Search order:
/// 1. Exact ISSN match
/// 2. Exact eISSN match
/// 3. Case-insensitive journal name match (trimmed)
pub fn match_journal(
    conn: &Connection,
    issn: Option<&str>,
    eissn: Option<&str>,
    journal_name: Option<&str>,
) -> Result<Option<String>, AppError> {
    // 1. Try ISSN
    if let Some(issn_val) = issn {
        if !issn_val.is_empty() {
            let result: Option<String> = conn
                .query_row(
                    "SELECT id FROM journal_index WHERE issn = ?1 LIMIT 1",
                    [issn_val],
                    |row| row.get(0),
                )
                .ok();
            if result.is_some() {
                return Ok(result);
            }
        }
    }

    // 2. Try eISSN
    if let Some(eissn_val) = eissn {
        if !eissn_val.is_empty() {
            let result: Option<String> = conn
                .query_row(
                    "SELECT id FROM journal_index WHERE eissn = ?1 LIMIT 1",
                    [eissn_val],
                    |row| row.get(0),
                )
                .ok();
            if result.is_some() {
                return Ok(result);
            }
        }
    }

    // 3. Try journal name (case-insensitive, trimmed)
    if let Some(name) = journal_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let result: Option<String> = conn
                .query_row(
                    "SELECT id FROM journal_index WHERE LOWER(TRIM(title)) = LOWER(TRIM(?1)) LIMIT 1",
                    [trimmed],
                    |row| row.get(0),
                )
                .ok();
            if result.is_some() {
                return Ok(result);
            }
        }
    }

    Ok(None)
}
