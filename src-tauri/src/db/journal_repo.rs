use rusqlite::Connection;

use crate::error::AppError;
use crate::models::biblio::{JournalInfo, YearCount};

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
                    "SELECT id FROM journal_index WHERE LOWER(TRIM(journal_title)) = LOWER(TRIM(?1)) LIMIT 1",
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

/// Full metadata + time-series for one `journal_index` row.
///
/// Returns `Ok(None)` when the id is unknown. Aggregates (`article_count`,
/// `first_year`, `last_year`, `pubs_by_year`, `citations_total`) are computed
/// over `included` articles linked to this journal.
pub fn get_journal_info(
    conn: &Connection,
    journal_index_id: &str,
) -> Result<Option<JournalInfo>, AppError> {
    // 1. Base metadata
    let meta = conn.query_row(
        "SELECT id, journal_title, issn, eissn, publisher_name, publisher_address, \
                languages, web_of_science_categories
         FROM journal_index WHERE id = ?1",
        [journal_index_id],
        |row| {
            Ok(JournalInfo {
                id: row.get(0)?,
                journal_title: row.get(1)?,
                issn: row.get(2)?,
                eissn: row.get(3)?,
                publisher_name: row.get(4)?,
                publisher_address: row.get(5)?,
                languages: row.get(6)?,
                web_of_science_categories: row.get(7)?,
                article_count: 0,
                first_year: None,
                last_year: None,
                pubs_by_year: Vec::new(),
                citations_total: 0,
            })
        },
    );
    let mut info = match meta {
        Ok(i) => i,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    // 2. Aggregates over included articles linked to this journal
    let agg: (i32, Option<i32>, Option<i32>, i64) = conn.query_row(
        "SELECT COUNT(*), MIN(publication_year), MAX(publication_year), COALESCE(SUM(num_cited), 0)
         FROM articles
         WHERE status = 'included' AND journal_index_id = ?1",
        [journal_index_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    info.article_count = agg.0;
    info.first_year = agg.1;
    info.last_year = agg.2;
    info.citations_total = agg.3;

    // 3. Yearly counts (ascending by year)
    let mut stmt = conn.prepare(
        "SELECT publication_year, COUNT(*) AS cnt
         FROM articles
         WHERE status = 'included' AND journal_index_id = ?1 AND publication_year IS NOT NULL
         GROUP BY publication_year ORDER BY publication_year ASC",
    )?;
    info.pubs_by_year = stmt
        .query_map([journal_index_id], |row| {
            Ok(YearCount { year: row.get(0)?, count: row.get(1)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(info))
}
