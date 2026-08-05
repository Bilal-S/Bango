use rusqlite::Connection;

use crate::error::AppError;
use crate::models::biblio::{JournalInfo, YearCount};

/// A `journal_index` hit for the interactive journal autocomplete.
/// Distinct from `JournalInfo` (full bibliometric aggregates).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalIndexMatch {
    pub id: String,
    pub journal_title: String,
    pub issn: Option<String>,
    pub eissn: Option<String>,
    pub publisher_name: Option<String>,
}

/// Normalize an ISSN string for lookup. Returns empty for non-valid ISSNs.
///
/// Handles dirty shapes from RIS/BibTeX: `"(ISSN)"` suffix, semicolon suffix,
/// unhyphenated 8-digit, ISBN garbage rejected. Pure, `#[must_use]`.
#[must_use]
pub fn normalize_issn(raw: &str) -> String {
    // 1. Trim.
    let mut s = raw.trim();
    // 2. Truncate at the first `(` or `;`.
    if let Some(pos) = s.find(['(', ';']) {
        s = &s[..pos];
    }
    // 3. Strip a trailing `)` if present; trim again.
    let s = s.trim_end_matches(')').trim();
    if s.is_empty() {
        return String::new();
    }
    // 4. If exactly 8 ASCII alphanumerics with no `-`, insert `-` at position 4.
    let candidate =
        if s.len() == 8 && !s.contains('-') && s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            let (a, b) = s.split_at(4);
            format!("{a}-{b}")
        } else {
            s.to_string()
        };
    // 5. Return only if it matches `^\d{4}-[\dXx]$`. Canonicalize the trailing
    //    check digit to uppercase X (ISSN spec form) so `1234567x` and
    //    `1234-567X` produce the same key and match deterministically.
    if is_valid_issn(&candidate) {
        canonicalize_issn(&candidate)
    } else {
        String::new()
    }
}

/// Uppercase a trailing lowercase `x` check digit to canonical spec form (`X`).
/// Digits and already-uppercase `X` pass through.
fn canonicalize_issn(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() == 9 && (bytes[8] == b'x') {
        let mut out = s.to_string();
        // SAFETY: byte 8 is ascii lowercase `x`; replacing with `X` keeps UTF-8 validity.
        out.replace_range(8..9, "X");
        out
    } else {
        s.to_string()
    }
}

/// Validate an already-cleaned ISSN candidate against `^\d{4}-[\dXx]$`.
fn is_valid_issn(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 9
        && bytes[0..4].iter().all(|b| b.is_ascii_digit())
        && bytes[4] == b'-'
        && (bytes[5..9].iter().all(|b| b.is_ascii_digit())
            || (bytes[5..8].iter().all(|b| b.is_ascii_digit())
                && (bytes[8] == b'X' || bytes[8] == b'x')))
}

/// Normalize a journal title for **equality comparison** (not substring search).
/// Folds common symbol variants: `&`↔`and`, `:`/`-`→space, strips parenthetical
/// ISSN/edition suffixes, lowercases, collapses whitespace. Pure, `#[must_use]`.
#[must_use]
pub fn normalize_journal_name(raw: &str) -> String {
    let mut s = raw.to_lowercase();
    // Strip trailing parenthetical suffixes repeatedly.
    loop {
        let trimmed = s.trim_end();
        if let Some(open) = trimmed.rfind('(') {
            if trimmed.ends_with(')') && trimmed[open..].chars().filter(|&c| c == '(').count() == 1
            {
                s = trimmed[..open].to_string();
                continue;
            }
        }
        break;
    }
    let s = s.replace('&', "and");
    let s = s.replace([':', '-'], " ");
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

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

/// Look up a `journal_index` row by ISSN, eISSN, or journal name.
/// **Sole automatic matching function** — every caller routes here.
///
/// Search order (first hit wins): (1) `issn` = article-ISSN, (2) `eissn` =
/// article-ISSN (cross-check), (3) `eissn` = article-eISSN, (4) `issn` =
/// article-eISSN (cross-check), (5) normalized-name equality.
///
/// Intentionally **no LIKE/substring**: silent auto-linking must not pick the
/// wrong journal among similar names. Substring matching is confined to
/// `search_journal_index` (user-driven autocomplete).
pub fn match_journal(
    conn: &Connection,
    issn: Option<&str>,
    eissn: Option<&str>,
    journal_name: Option<&str>,
) -> Result<Option<String>, AppError> {
    // Normalize the article's ISSN once; reuse across both column checks.
    let norm_article_issn = issn.map(normalize_issn).filter(|s| !s.is_empty());
    let norm_article_eissn = eissn.map(normalize_issn).filter(|s| !s.is_empty());

    // Tiers 1 + 2: try the article's print ISSN against both columns.
    if let Some(ref v) = norm_article_issn {
        if let Some(id) = find_by_column(conn, "issn", v)? {
            return Ok(Some(id));
        }
        if let Some(id) = find_by_column(conn, "eissn", v)? {
            return Ok(Some(id));
        }
    }

    // Tiers 3 + 4: try the article's electronic ISSN against both columns.
    if let Some(ref v) = norm_article_eissn {
        if let Some(id) = find_by_column(conn, "eissn", v)? {
            return Ok(Some(id));
        }
        if let Some(id) = find_by_column(conn, "issn", v)? {
            return Ok(Some(id));
        }
    }

    // Tier 5: symbol-insensitive name equality.
    if let Some(name) = journal_name {
        let normalized = normalize_journal_name(name);
        if !normalized.is_empty() {
            // SQLite cannot compute normalize_journal_name server-side, so
            // narrow with a cheap LIKE on the first token and confirm equality in Rust.
            let first_token = normalized.split_whitespace().next().unwrap_or(&normalized);
            let pattern = format!("%{first_token}%");
            let mut stmt = conn.prepare(
                "SELECT id, journal_title FROM journal_index
                 WHERE LOWER(journal_title) LIKE ?1",
            )?;
            let mut rows = stmt.query_map([&pattern], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            while let Some(Ok((id, title))) = rows.next() {
                if normalize_journal_name(&title) == normalized {
                    return Ok(Some(id));
                }
            }
        }
    }

    Ok(None)
}

/// Look up a journal_index id where `column` (`issn` or `eissn`) equals `value`.
/// Returns `Ok(None)` when no row matches (never errors on "no rows").
fn find_by_column(
    conn: &Connection,
    column: &str,
    value: &str,
) -> Result<Option<String>, AppError> {
    // `column` is a static literal in this module, not user input.
    let sql = format!("SELECT id FROM journal_index WHERE {column} = ?1 LIMIT 1");
    let result: Option<String> = conn.query_row(&sql, [value], |row| row.get(0)).ok();
    Ok(result)
}

/// Minimum non-whitespace length before the LIKE tier of `search_journal_index` fires.
const MIN_LIKE_QUERY_LEN: usize = 4;

/// Default cap on autocomplete candidates. Keeps the dropdown readable.
const DEFAULT_SEARCH_LIMIT: i64 = 25;

/// Interactive journal search for the article-metadata autocomplete. Substring LIKE
/// is safe here because the user reviews candidates.
///
/// Search tiers (first non-empty tier returns):
/// 1. ISSN match.
/// 2. Exact name match.
/// 3. LIKE substring (min `MIN_LIKE_QUERY_LEN` chars; shortest title first).
///
/// Returns empty vec for short non-ISSN queries or no hits.
pub fn search_journal_index(
    conn: &Connection,
    query: &str,
    limit: Option<i64>,
) -> Result<Vec<JournalIndexMatch>, AppError> {
    // Capping in Rust (via `take`) instead of a `LIMIT ?` bind keeps the SQL
    // single-param and sidesteps rusqlite's two-placeholder binding.
    let cap = limit.unwrap_or(DEFAULT_SEARCH_LIMIT).max(1) as usize;
    let trimmed = query.trim();

    // Tier 1: ISSN.
    let norm_issn = normalize_issn(trimmed);
    if !norm_issn.is_empty() {
        let rows = collect_matches(
            conn,
            "SELECT id, journal_title, issn, eissn, publisher_name
             FROM journal_index
             WHERE issn = ?1 OR eissn = ?1
             ORDER BY LENGTH(journal_title) ASC",
            &norm_issn,
            cap,
        )?;
        if !rows.is_empty() {
            return Ok(rows);
        }
    }

    // Tier 2: exact name.
    if !trimmed.is_empty() {
        let lower = trimmed.to_lowercase();
        let rows = collect_matches(
            conn,
            "SELECT id, journal_title, issn, eissn, publisher_name
             FROM journal_index
             WHERE LOWER(TRIM(journal_title)) = ?1",
            &lower,
            cap,
        )?;
        if !rows.is_empty() {
            return Ok(rows);
        }
    }

    // Tier 3: LIKE substring (min length guard).
    if trimmed.chars().count() >= MIN_LIKE_QUERY_LEN {
        let pattern = format!("%{}%", trimmed.to_lowercase());
        let rows = collect_matches(
            conn,
            "SELECT id, journal_title, issn, eissn, publisher_name
             FROM journal_index
             WHERE LOWER(journal_title) LIKE ?1
             ORDER BY LENGTH(journal_title) ASC",
            &pattern,
            cap,
        )?;
        return Ok(rows);
    }

    Ok(Vec::new())
}

/// Runs a single-bound-parameter `journal_index` lookup and maps up to `cap`
/// rows into `JournalIndexMatch`. The caller-provided SQL must use exactly one
/// `?1` placeholder; capping is done in Rust via `take(cap)`.
fn collect_matches(
    conn: &Connection,
    sql: &str,
    param: &str,
    cap: usize,
) -> Result<Vec<JournalIndexMatch>, AppError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([param], |row| {
        Ok(JournalIndexMatch {
            id: row.get(0)?,
            journal_title: row.get(1)?,
            issn: row.get(2)?,
            eissn: row.get(3)?,
            publisher_name: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows.take(cap) {
        out.push(row?);
    }
    Ok(out)
}

/// Full metadata + time-series for one `journal_index` row.
/// Returns `Ok(None)` for unknown id. Aggregates over `included` articles.
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
