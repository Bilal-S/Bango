use rusqlite::Connection;
use std::collections::HashMap;

use crate::error::AppError;
use crate::models::biblio::{
    AuthorCollaborator, AuthorDetail, AuthorPaper, AuthorProductivityKpis, AuthorRank,
};

use super::authors::get_author_pubs_by_year;
use super::institutions::get_institutions_by_author;

/// Get all authors with derived productivity metrics.
///
/// Issues a single SQL query for base + scalar metrics, then computes
/// `g_index`, `avg_citations_per_paper`, and `productivity_rate` in Rust
/// (these require per-author paper citation lists).
///
/// All metrics are scoped to `articles.status != 'duplicate'`.
pub fn get_author_rankings(conn: &Connection) -> Result<Vec<AuthorRank>, AppError> {
    // ── Base + scalar metrics in one query ─────────────────────────
    let mut stmt = conn.prepare(
        "SELECT \
            ba.id, ba.display_name, ba.normalized_name, \
            ba.article_count, ba.first_author_count, ba.total_citations, \
            COALESCE(ba.estimated_h_index, 0), COALESCE(ba.avg_year, 0.0), \
            (SELECT COUNT(*) FROM articles a \
             JOIN biblio_article_authors baa ON baa.article_id = a.id \
             WHERE baa.author_id = ba.id AND a.status != 'duplicate' \
               AND a.num_cited IS NOT NULL AND a.num_cited >= 10) AS i10_index, \
            (SELECT COUNT(*) FROM biblio_article_authors baa1 \
             WHERE baa1.author_id = ba.id \
               AND baa1.author_order = ( \
                 SELECT MAX(author_order) FROM biblio_article_authors baa2 \
                 WHERE baa2.article_id = baa1.article_id \
               ) \
               AND EXISTS (SELECT 1 FROM articles a \
                           WHERE a.id = baa1.article_id AND a.status != 'duplicate')) AS last_author_count, \
            (SELECT COUNT(*) FROM ( \
               SELECT baa3.article_id FROM biblio_article_authors baa3 \
               JOIN articles a ON a.id = baa3.article_id \
               WHERE a.status != 'duplicate' \
               GROUP BY baa3.article_id HAVING COUNT(*) = 1 \
             ) solo \
             JOIN biblio_article_authors baa4 ON baa4.article_id = solo.article_id \
             WHERE baa4.author_id = ba.id) AS solo_paper_count, \
            (SELECT COUNT(*) FROM articles a \
             JOIN biblio_article_authors baa ON baa.article_id = a.id \
             WHERE baa.author_id = ba.id AND a.status != 'duplicate' \
               AND a.publication_year IS NOT NULL \
               AND a.publication_year >= CAST(strftime('%Y', 'now') AS INTEGER) - 5) AS recent_paper_count, \
            (SELECT MAX(a.publication_year) - MIN(a.publication_year) \
             FROM articles a \
             JOIN biblio_article_authors baa ON baa.article_id = a.id \
             WHERE baa.author_id = ba.id AND a.status != 'duplicate' \
               AND a.publication_year IS NOT NULL) AS years_active \
         FROM biblio_authors ba \
         WHERE EXISTS (SELECT 1 FROM biblio_article_authors baa \
                       JOIN articles a ON a.id = baa.article_id \
                       WHERE baa.author_id = ba.id AND a.status != 'duplicate') \
         ORDER BY COALESCE(ba.estimated_h_index, 0) DESC, ba.total_citations DESC",
    )?;

    struct BaseRow {
        id: String,
        display_name: String,
        normalized_name: String,
        article_count: i32,
        first_author_count: i32,
        total_citations: i64,
        estimated_h_index: i32,
        avg_year: Option<f64>,
        i10_index: i32,
        last_author_count: i32,
        solo_paper_count: i32,
        recent_paper_count: i32,
        years_active: Option<i32>,
    }

    let base_rows: Vec<BaseRow> = stmt
        .query_map([], |row| {
            let avg_year_raw: f64 = row.get(7)?;
            Ok(BaseRow {
                id: row.get(0)?,
                display_name: row.get(1)?,
                normalized_name: row.get(2)?,
                article_count: row.get(3)?,
                first_author_count: row.get(4)?,
                total_citations: row.get(5)?,
                estimated_h_index: row.get(6)?,
                avg_year: if avg_year_raw == 0.0 { None } else { Some(avg_year_raw) },
                i10_index: row.get(8)?,
                last_author_count: row.get(9)?,
                solo_paper_count: row.get(10)?,
                recent_paper_count: row.get(11)?,
                years_active: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // ── Per-author citation lists for g-index computation ──────────
    let mut cites_map: HashMap<String, Vec<i64>> = HashMap::new();
    let mut cites_stmt = conn.prepare(
        "SELECT baa.author_id, a.num_cited \
         FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE a.status != 'duplicate' AND a.num_cited IS NOT NULL",
    )?;
    let cite_rows = cites_stmt.query_map([], |row| {
        Ok::<_, rusqlite::Error>((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
    })?;
    for row in cite_rows.flatten() {
        if let Some(c) = row.1 {
            cites_map.entry(row.0).or_default().push(c);
        }
    }
    drop(cites_stmt);

    // ── Per-author primary institution ─────────────────────────────
    let mut institution_map: HashMap<String, String> = HashMap::new();
    let mut inst_stmt = conn.prepare(
        "SELECT baa.author_id, bi.normalized_name \
         FROM biblio_author_affiliations baa \
         JOIN biblio_institutions bi ON bi.id = baa.institution_id \
         JOIN articles a ON a.id = baa.article_id \
         WHERE a.status != 'duplicate' \
         ORDER BY a.publication_year DESC NULLS LAST, bi.normalized_name ASC",
    )?;
    let inst_rows = inst_stmt.query_map([], |row| {
        Ok::<_, rusqlite::Error>((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in inst_rows.flatten() {
        // First occurrence wins (most recent year due to ORDER BY).
        institution_map.entry(row.0).or_insert(row.1);
    }
    drop(inst_stmt);

    // ── Assemble final AuthorRank vec ──────────────────────────────
    let mut rankings: Vec<AuthorRank> = base_rows
        .into_iter()
        .map(|b| {
            let author_id = b.id.clone();
            let citations = cites_map.get(&author_id).cloned().unwrap_or_default();
            let g_index = compute_g_index(&citations);
            let avg_citations_per_paper = if b.article_count > 0 {
                Some(b.total_citations as f64 / b.article_count as f64)
            } else {
                None
            };
            let productivity_rate = b.years_active.map(|y| {
                if y > 0 {
                    b.article_count as f64 / y as f64
                } else {
                    b.article_count as f64
                }
            });
            AuthorRank {
                id: b.id,
                display_name: b.display_name,
                normalized_name: b.normalized_name,
                article_count: b.article_count,
                first_author_count: b.first_author_count,
                last_author_count: b.last_author_count,
                solo_paper_count: b.solo_paper_count,
                total_citations: b.total_citations,
                estimated_h_index: b.estimated_h_index,
                i10_index: b.i10_index,
                g_index,
                avg_citations_per_paper,
                avg_year: b.avg_year,
                years_active: b.years_active,
                productivity_rate,
                recent_paper_count: b.recent_paper_count,
                primary_institution: institution_map.get(&author_id).cloned(),
            }
        })
        .collect();

    // Ensure deterministic order after Rust transformations (stable sort, already sorted above)
    rankings.sort_by(|a, b| {
        b.estimated_h_index
            .cmp(&a.estimated_h_index)
            .then_with(|| b.total_citations.cmp(&a.total_citations))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    Ok(rankings)
}

/// Compute the g-index: largest n such that the top-n papers collectively
/// receive at least n² citations.
fn compute_g_index(citations: &[i64]) -> i32 {
    if citations.is_empty() {
        return 0;
    }
    let mut sorted: Vec<i64> = citations.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));

    let mut cumulative: i64 = 0;
    let mut g: i32 = 0;
    for (i, &c) in sorted.iter().enumerate() {
        cumulative += c.max(0);
        let n = (i + 1) as i64;
        if cumulative >= n * n {
            g = n as i32;
        } else {
            break;
        }
    }
    g
}

/// Get the full profile for a single author - lazy-loaded by the detail panel.
///
/// Reuses `get_author_pubs_by_year` and `get_institutions_by_author`, then adds
/// top collaborators (from co-authorship edges) and recent papers.
pub fn get_author_detail(conn: &Connection, author_id: &str) -> Result<AuthorDetail, AppError> {
    // ── Base rank (reuse get_author_rankings, filter to this author) ──
    let all_rankings = get_author_rankings(conn)?;
    let rank = all_rankings
        .into_iter()
        .find(|r| r.id == author_id)
        .ok_or_else(|| AppError::Database(rusqlite::Error::QueryReturnedNoRows))?;

    // ── Pubs by year (sparkline) ───────────────────────────────────
    let pubs_by_year = get_author_pubs_by_year(conn, author_id)?;

    // ── Institutions ───────────────────────────────────────────────
    let institutions = get_institutions_by_author(conn, author_id)?;

    // ── Top 5 collaborators (co-authorship edge weight) ────────────
    let mut collab_stmt = conn.prepare(
        "SELECT \
            CASE WHEN e.source_id = ?1 THEN e.target_id ELSE e.source_id END AS collaborator_id, \
            e.weight \
         FROM biblio_network_edges e \
         JOIN biblio_network_meta m ON m.id = e.network_id \
         WHERE m.network_type = 'co_authorship' \
           AND (e.source_id = ?1 OR e.target_id = ?1) \
         ORDER BY e.weight DESC \
         LIMIT 5",
    )?;
    let collab_ids_weights: Vec<(String, f64)> = collab_stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(collab_stmt);

    // Resolve collaborator names
    let mut top_collaborators: Vec<AuthorCollaborator> = Vec::new();
    for (collab_id, weight) in collab_ids_weights {
        let name: Option<String> = conn
            .query_row(
                "SELECT display_name FROM biblio_authors WHERE id = ?1",
                rusqlite::params![collab_id],
                |row| row.get(0),
            )
            .ok();
        if let Some(collaborator_name) = name {
            top_collaborators.push(AuthorCollaborator {
                collaborator_id: collab_id,
                collaborator_name,
                shared_papers: weight as i32,
            });
        }
    }

    // ── Top 5 recent papers by citation ────────────────────────────
    let mut papers_stmt = conn.prepare(
        "SELECT a.id, a.title, a.publication_year, a.journal, a.num_cited, \
                baa.author_order, a.doi \
         FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 AND a.status != 'duplicate' \
         ORDER BY COALESCE(a.num_cited, 0) DESC, a.publication_year DESC NULLS LAST \
         LIMIT 5",
    )?;
    let recent_papers: Vec<AuthorPaper> = papers_stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(AuthorPaper {
                article_id: row.get(0)?,
                title: row.get(1)?,
                publication_year: row.get(2)?,
                journal: row.get(3)?,
                num_cited: row.get(4)?,
                author_order: row.get(5)?,
                doi: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AuthorDetail { rank, pubs_by_year, institutions, top_collaborators, recent_papers })
}

/// Aggregate KPI stats for the productivity view header strip.
pub fn get_author_productivity_kpis(conn: &Connection) -> Result<AuthorProductivityKpis, AppError> {
    let total_authors: i32 = conn
        .query_row(
            "SELECT COUNT(DISTINCT baa.author_id) \
             FROM biblio_article_authors baa \
             JOIN articles a ON a.id = baa.article_id \
             WHERE a.status != 'duplicate'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_papers: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(ba.article_count), 0) \
             FROM biblio_authors ba \
             WHERE EXISTS (SELECT 1 FROM biblio_article_authors baa \
                           JOIN articles a ON a.id = baa.article_id \
                           WHERE baa.author_id = ba.id AND a.status != 'duplicate')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let avg_h_index: Option<f64> = conn
        .query_row(
            "SELECT AVG(COALESCE(ba.estimated_h_index, 0)) \
             FROM biblio_authors ba \
             WHERE EXISTS (SELECT 1 FROM biblio_article_authors baa \
                           JOIN articles a ON a.id = baa.article_id \
                           WHERE baa.author_id = ba.id AND a.status != 'duplicate')",
            [],
            |r| r.get(0),
        )
        .ok();

    let max_h_index: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(COALESCE(ba.estimated_h_index, 0)), 0) \
             FROM biblio_authors ba \
             WHERE EXISTS (SELECT 1 FROM biblio_article_authors baa \
                           JOIN articles a ON a.id = baa.article_id \
                           WHERE baa.author_id = ba.id AND a.status != 'duplicate')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let avg_citations: Option<f64> = conn
        .query_row(
            "SELECT AVG(COALESCE(ba.total_citations, 0)) \
             FROM biblio_authors ba \
             WHERE EXISTS (SELECT 1 FROM biblio_article_authors baa \
                           JOIN articles a ON a.id = baa.article_id \
                           WHERE baa.author_id = ba.id AND a.status != 'duplicate')",
            [],
            |r| r.get(0),
        )
        .ok();

    let total_collaborations: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_network_edges e \
             JOIN biblio_network_meta m ON m.id = e.network_id \
             WHERE m.network_type = 'co_authorship'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let (year_from, year_to): (Option<i32>, Option<i32>) = conn
        .query_row(
            "SELECT MIN(a.publication_year), MAX(a.publication_year) \
             FROM articles a \
             WHERE a.status != 'duplicate' AND a.publication_year IS NOT NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));

    Ok(AuthorProductivityKpis {
        total_authors,
        total_papers,
        avg_h_index,
        max_h_index,
        avg_citations,
        total_collaborations,
        year_from,
        year_to,
    })
}
