use chrono::Datelike;
use rusqlite::Connection;
use std::collections::HashMap;

use crate::error::AppError;
use crate::models::biblio::{BiblioKpis, YearCount};

const CITATION_DECAY: [f64; 6] = [0.02, 0.08, 0.13, 0.17, 0.15, 0.11];

/// Compute KPI aggregates for the bibliometric dashboard.
pub fn get_biblio_kpis(conn: &Connection) -> Result<BiblioKpis, AppError> {
    // Included article count
    let included_count: i32 = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |r| r.get(0))
        .unwrap_or(0);

    // Total citations (num_cited may be NULL)
    let total_citations: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(num_cited), 0) FROM articles WHERE status = 'included'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Unique normalized authors — always from biblio_authors (populated by normalization)
    let unique_authors: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_authors", [], |r| r.get(0)).unwrap_or(0);

    // Publications by year — single query groups included articles by publication_year
    let mut stmt = conn.prepare(
        "SELECT publication_year, COUNT(*) AS cnt \
         FROM articles \
         WHERE status = 'included' AND publication_year IS NOT NULL \
         GROUP BY publication_year \
         ORDER BY publication_year ASC",
    )?;
    let pubs_by_year: Vec<YearCount> = stmt
        .query_map([], |row| Ok(YearCount { year: row.get(0)?, count: row.get(1)? }))?
        .collect::<Result<Vec<_>, _>>()?;

    // Derive year range from pubs_by_year (first & last element)
    let year_from = pubs_by_year.first().map(|yc| yc.year);
    let year_to = pubs_by_year.last().map(|yc| yc.year);

    // Pubs per year: total included articles with a year / number of distinct years
    let total_with_year: i32 = pubs_by_year.iter().map(|yc| yc.count).sum();
    let pubs_per_year = if !pubs_by_year.is_empty() {
        Some(total_with_year as f64 / pubs_by_year.len() as f64)
    } else {
        None
    };

    // Average growth rate: mean of all consecutive year-pair growth rates
    let avg_growth_rate = if pubs_by_year.len() >= 2 {
        let mut rates: Vec<f64> = Vec::new();
        for window in pubs_by_year.windows(2) {
            let prev = window[0].count;
            let curr = window[1].count;
            if prev > 0 {
                rates.push(((curr - prev) as f64 / prev as f64) * 100.0);
            }
        }
        if !rates.is_empty() {
            Some(rates.iter().sum::<f64>() / rates.len() as f64)
        } else {
            None
        }
    } else {
        None
    };

    // Reference counts of included articles, grouped by publication year
    let mut refs_stmt = conn.prepare(
        "SELECT publication_year, COALESCE(SUM(num_references), 0) AS cnt \
         FROM articles \
         WHERE status = 'included' AND publication_year IS NOT NULL \
         GROUP BY publication_year \
         ORDER BY publication_year ASC",
    )?;
    let refs_by_year: Vec<YearCount> = refs_stmt
        .query_map([], |row| Ok(YearCount { year: row.get(0)?, count: row.get(1)? }))?
        .collect::<Result<Vec<_>, _>>()?;

    // ── Normalized Citations by Year ──────────────────────────────
    let citations_by_year = compute_citations_by_year(conn)?;

    Ok(BiblioKpis {
        included_count,
        total_citations,
        unique_authors,
        year_from,
        year_to,
        pubs_per_year,
        pubs_by_year,
        avg_growth_rate,
        refs_by_year,
        citations_by_year,
    })
}

/// group actual linked reference papers by their `publication_year`.
///
/// For articles **without** detail records, spread `num_cited` across years
/// using the decay distribution, with undistributed remainder going to the
/// current year.
fn compute_citations_by_year(conn: &Connection) -> Result<Vec<YearCount>, AppError> {
    let current_year = chrono::Utc::now().year();
    let mut map: HashMap<i32, i32> = HashMap::new();

    // ── 1. Articles WITH citation detail records ─────────────────
    // Each linked reference paper (type = 0 = citation) counts as 1 citation
    // in the reference paper's publication_year.
    let mut detail_stmt = conn.prepare(
        "SELECT rp.publication_year \
         FROM article_reference_links arl \
         JOIN reference_papers rp ON rp.id = arl.reference_paper_id \
         JOIN articles a ON a.id = arl.parent_article_id \
         WHERE a.status = 'included' \
           AND arl.type = 0 \
           AND rp.publication_year IS NOT NULL",
    )?;

    let rows = detail_stmt.query_map([], |row| {
        let year: i32 = row.get(0)?;
        Ok(year)
    })?;
    for year in rows.flatten() {
        *map.entry(year).or_insert(0) += 1;
    }

    // ── 2. Articles WITHOUT citation detail records ──────────────
    let mut no_detail_stmt = conn.prepare(
        "SELECT publication_year, num_cited \
         FROM articles \
         WHERE status = 'included' \
           AND has_citation_details = 0 \
           AND publication_year IS NOT NULL \
           AND num_cited IS NOT NULL AND num_cited > 0",
    )?;

    let rows = no_detail_stmt.query_map([], |row| {
        let year: i32 = row.get(0)?;
        let cited: i32 = row.get(1)?;
        Ok((year, cited))
    })?;
    for row in rows.flatten() {
        distribute_citations(&mut map, row.0, row.1, current_year);
    }

    Ok(sort_year_map(map))
}

/// Distribute `num_cited` citations across years using the decay curve,
/// starting from `pub_year`. Remainder goes to `current_year`.
fn distribute_citations(
    map: &mut HashMap<i32, i32>,
    pub_year: i32,
    num_cited: i32,
    current_year: i32,
) {
    let mut distributed: i32 = 0;
    for (offset, weight) in CITATION_DECAY.iter().enumerate() {
        let year = pub_year + offset as i32;
        if year > current_year {
            break;
        }
        let count = (*weight * num_cited as f64).round() as i32;
        if count > 0 {
            *map.entry(year).or_insert(0) += count;
            distributed += count;
        }
    }
    // Remainder goes to current year
    let remainder = num_cited - distributed;
    if remainder > 0 {
        *map.entry(current_year).or_insert(0) += remainder;
    }
}

/// Sort a year→count map into ascending YearCount vec.
fn sort_year_map(map: HashMap<i32, i32>) -> Vec<YearCount> {
    let mut v: Vec<YearCount> =
        map.into_iter().map(|(year, count)| YearCount { year, count }).collect();
    v.sort_by_key(|yc| yc.year);
    v
}
