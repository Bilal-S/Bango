use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use super::labels::format_paper_label;
use crate::error::AppError;

/// In-memory match index over the article set, mirroring the predicate shapes
/// of `reference_repo::auto_match_paper_to_article` (DOI first, then
/// title+journal+year with no cross-fallback between weaker combinations).
///
/// Built ONCE per normalization run so matching every reference paper becomes
/// hash lookups instead of one full `articles` scan per paper. The old
/// per-paper `SELECT *` + unindexed `LOWER(doi)`/`LOWER(title)` scans over the
/// widest table in the DB dominated `biblio_normalize` runtime on libraries
/// with harvested references/citations (O(papers x articles) scans).
///
/// Keys use ASCII lowercasing to stay byte-equivalent to SQLite's `LOWER()`
/// (ASCII-only in non-ICU builds). The build scan is ordered by `rowid` and
/// key collisions keep the FIRST id, which reproduces the old full-scan
/// `LIMIT 1` (and the v010 expression-index) row order deterministically.
struct ArticleMatchIndex {
    by_doi: HashMap<String, String>,
    by_title_journal_year: HashMap<(String, String, i32), String>,
    by_title_journal: HashMap<(String, String), String>,
    by_title_year: HashMap<(String, i32), String>,
    by_title: HashMap<String, String>,
}

/// One candidate row: (article/paper id, doi, title, journal, publication_year).
type MatchRow = (String, Option<String>, String, Option<String>, Option<i32>);

impl ArticleMatchIndex {
    /// Load the article match keys once. All article statuses participate -
    /// `auto_match_paper_to_article` never filtered by status.
    fn build(conn: &Connection) -> Result<Self, AppError> {
        let mut stmt = conn.prepare(
            "SELECT id, doi, title, journal, publication_year FROM articles ORDER BY rowid",
        )?;
        let rows: Vec<MatchRow> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut index = ArticleMatchIndex {
            by_doi: HashMap::new(),
            by_title_journal_year: HashMap::new(),
            by_title_journal: HashMap::new(),
            by_title_year: HashMap::new(),
            by_title: HashMap::new(),
        };

        for (id, doi, title, journal, year) in rows {
            // Empty titles never match (the paper-side branch requires a
            // non-empty title), so they are skipped to avoid '' collisions.
            let title_key = title.to_ascii_lowercase();
            if !title_key.is_empty() {
                index.by_title.entry(title_key.clone()).or_insert_with(|| id.clone());
                if let Some(j) = journal.as_deref() {
                    let journal_key = j.to_ascii_lowercase();
                    index
                        .by_title_journal
                        .entry((title_key.clone(), journal_key.clone()))
                        .or_insert_with(|| id.clone());
                    if let Some(y) = year {
                        index
                            .by_title_journal_year
                            .entry((title_key.clone(), journal_key, y))
                            .or_insert_with(|| id.clone());
                    }
                }
                if let Some(y) = year {
                    index.by_title_year.entry((title_key, y)).or_insert_with(|| id.clone());
                }
            }
            if let Some(d) = doi.as_deref() {
                let doi_key = d.to_ascii_lowercase();
                if !doi_key.is_empty() {
                    index.by_doi.entry(doi_key).or_insert_with(|| id.clone());
                }
            }
        }

        Ok(index)
    }

    /// Resolve one paper to an article id with the same precedence as
    /// `reference_repo::auto_match_paper_to_article`: non-empty DOI first,
    /// then the title combo keyed by exactly the fields the paper carries
    /// (a paper with a journal never falls back to a title-only match).
    fn resolve(
        &self,
        doi: Option<&str>,
        title: &str,
        journal: Option<&str>,
        year: Option<i32>,
    ) -> Option<&str> {
        if let Some(d) = doi {
            if !d.is_empty() {
                if let Some(id) = self.by_doi.get(&d.to_ascii_lowercase()) {
                    return Some(id);
                }
            }
        }
        if title.is_empty() {
            return None;
        }
        let title_key = title.to_ascii_lowercase();
        let found = match (journal, year) {
            (Some(j), Some(y)) => {
                self.by_title_journal_year.get(&(title_key, j.to_ascii_lowercase(), y))
            }
            (Some(j), None) => self.by_title_journal.get(&(title_key, j.to_ascii_lowercase())),
            (None, Some(y)) => self.by_title_year.get(&(title_key, y)),
            (None, None) => self.by_title.get(&title_key),
        };
        found.map(String::as_str)
    }
}

/// Auto-match reference papers to library articles (batch, set-based).
///
/// Collects all `reference_papers` whose `matched_article_id IS NULL` (and
/// that are linked to at least one article), resolves them against an
/// in-memory [`ArticleMatchIndex`] built from the current article set using
/// the exact `reference_repo::auto_match_paper_to_article` semantics (DOI
/// first, then title+journal+year with no cross-fallback), and applies the
/// `matched` updates through one prepared statement. Papers that match are
/// updated with their `matched_article_id` and `match_status = 'matched'`.
///
/// Returns the number of papers newly matched.
pub fn auto_match_references_to_articles(conn: &Connection) -> Result<usize, AppError> {
    let index = ArticleMatchIndex::build(conn)?;

    // Candidate papers: not yet matched and linked to at least one article so
    // we don't waste cycles on orphan reference rows. Only the five match
    // columns are fetched - no per-paper `SELECT *` (abstracts stay unread).
    let mut stmt = conn.prepare(
        "SELECT rp.id, rp.doi, rp.title, rp.journal, rp.publication_year \
         FROM reference_papers rp \
         WHERE rp.matched_article_id IS NULL \
           AND rp.match_status IN ('unmatched', 'matched') \
           AND EXISTS (SELECT 1 FROM article_reference_links l WHERE l.reference_paper_id = rp.id)",
    )?;
    let candidates: Vec<MatchRow> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let matches: Vec<(String, String)> = candidates
        .iter()
        .filter_map(|(paper_id, doi, title, journal, year)| {
            index
                .resolve(doi.as_deref(), title, journal.as_deref(), *year)
                .map(|article_id| (paper_id.clone(), article_id.to_string()))
        })
        .collect();

    let mut matched = 0usize;
    if !matches.is_empty() {
        let mut update_stmt = conn.prepare(
            "UPDATE reference_papers \
             SET matched_article_id = ?1, match_status = 'matched', updated_at = datetime('now') \
             WHERE id = ?2",
        )?;
        for (paper_id, article_id) in &matches {
            update_stmt.execute(rusqlite::params![article_id, paper_id])?;
            matched += 1;
        }
    }

    Ok(matched)
}

/// Build citation edges between included articles.
///
/// A citation edge `source → target` means *source cites target*. We discover
/// these by joining `article_reference_links` with `reference_papers`:
///
/// - `type = 1` (reference): `parent_article_id` cites the paper whose
///   `matched_article_id` resolves to another included article.
/// - `type = 0` (citation): another article cites `parent_article_id` via a
///   reference paper whose `matched_article_id` is the parent. This is the
///   reverse direction, so we swap source/target.
///
/// Only links where the resolved `matched_article_id` refers to an included
/// article are kept, so the network only contains edges between included
/// articles.
pub fn build_citation_edges(conn: &Connection) -> Result<usize, AppError> {
    // Collect raw citation pairs from the database.
    // Direction convention: source cites target.
    let mut stmt = conn.prepare(
        "SELECT \
                 CASE WHEN l.type = 1 THEN l.parent_article_id ELSE rp.matched_article_id END AS source_id, \
                 CASE WHEN l.type = 1 THEN rp.matched_article_id ELSE l.parent_article_id END AS target_id \
          FROM article_reference_links l \
          JOIN reference_papers rp ON rp.id = l.reference_paper_id \
          JOIN articles src ON src.id = \
                 CASE WHEN l.type = 1 THEN l.parent_article_id ELSE rp.matched_article_id END \
          JOIN articles tgt ON tgt.id = \
                 CASE WHEN l.type = 1 THEN rp.matched_article_id ELSE l.parent_article_id END \
          WHERE rp.matched_article_id IS NOT NULL \
            AND rp.matched_article_id != l.parent_article_id \
            AND src.status = 'included' \
            AND tgt.status = 'included'",
    )?;

    let mut edges: Vec<(String, String)> = stmt
        .query_map([], |row| {
            let source: String = row.get(0)?;
            let target: String = row.get(1)?;
            Ok((source, target))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // Deduplicate parallel edges (the same source/target pair via multiple
    // reference papers). Citation networks are simple directed graphs.
    edges.sort_unstable();
    edges.dedup();

    let edge_count = edges.len() as i32;
    if edge_count == 0 {
        return Ok(0);
    }

    let params = serde_json::json!({
        "direction": "directed",
        "description": "source cites target (A → B means A cites B)",
    })
    .to_string();

    let network_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, node_count, edge_count, params_json) \
         VALUES (?1, 'citation', 'Citation Network', 0, ?2, ?3)",
        rusqlite::params![network_id, edge_count, params],
    )?;

    let mut nodes: HashSet<&str> = HashSet::new();
    for (source, target) in &edges {
        let edge_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) \
             VALUES (?1, ?2, ?3, ?4, 1.0)",
            rusqlite::params![edge_id, network_id, source, target],
        )?;
        nodes.insert(source);
        nodes.insert(target);
    }

    conn.execute(
        "UPDATE biblio_network_meta SET node_count = ?1 WHERE id = ?2",
        rusqlite::params![nodes.len() as i32, network_id],
    )?;

    Ok(edges.len())
}

/// Get the citation network as JSON for graph rendering.
///
/// Nodes are derived from all included articles that participate in at least
/// one citation edge. Each node carries article metadata (title, authors,
/// year, journal, citation count, reference count, abstract) so the frontend
/// can render rich tooltips and detail panels.
///
/// When `include_unmatched` is true, additional leaf nodes are emitted for
/// `reference_papers` that have not been matched to an article, connected to
/// their parent article by a dashed "unmatched" edge. This lets users see the
/// full reference topology even when few papers are matched.
///
/// A `meta` block is always returned with diagnostic counts so the frontend
/// can render an informative empty-state.
#[allow(clippy::type_complexity)]
pub fn get_citation_network_json(
    conn: &Connection,
    include_unmatched: bool,
) -> Result<serde_json::Value, AppError> {
    // Fetch all included articles.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.id, a.title, a.authors, a.publication_year, a.journal, \
                 a.num_cited, a.num_references, a.abstract_text, a.reference_type \
          FROM articles a \
          WHERE a.status = 'included'",
    )?;

    let mut nodes: Vec<serde_json::Value> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let authors: String = row.get(2)?;
            let year: Option<i32> = row.get(3)?;
            let journal: Option<String> = row.get(4)?;
            let num_cited: Option<i64> = row.get(5)?;
            let num_references: Option<i64> = row.get(6)?;
            let abstract_text: String = row.get(7)?;
            let reference_type: Option<String> = row.get(8)?;
            Ok((
                id,
                title,
                authors,
                year,
                journal,
                num_cited,
                num_references,
                abstract_text,
                reference_type,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(
            |(
                id,
                title,
                authors,
                year,
                journal,
                num_cited,
                num_references,
                abstract_text,
                reference_type,
            )| {
                let label = format_paper_label(&authors, year);
                serde_json::json!({
                    "id": id,
                    "label": label,
                    "title": title,
                    "authors": authors,
                    "year": year,
                    "journal": journal,
                    "numCited": num_cited.unwrap_or(0),
                    "numReferences": num_references.unwrap_or(0),
                    "abstract": abstract_text,
                    "unmatched": false,
                    "referenceType": reference_type,
                })
            },
        )
        .collect();
    drop(stmt);

    // Fetch directed edges from the citation network.
    let mut edges_stmt = conn.prepare(
        "SELECT source_id, target_id, weight FROM biblio_network_edges \
          WHERE network_id IN (\
             SELECT id FROM biblio_network_meta WHERE network_type = 'citation'\
          )",
    )?;
    let mut edges: Vec<serde_json::Value> = edges_stmt
        .query_map([], |row| {
            let source: String = row.get(0)?;
            let target: String = row.get(1)?;
            let weight: f64 = row.get(2)?;
            Ok((source, target, weight))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(source, target, weight)| {
            serde_json::json!({
                "source": source,
                "target": target,
                "weight": weight,
                "unmatched": false,
            })
        })
        .collect();
    drop(edges_stmt);

    // Optionally append unmatched reference papers as leaf nodes.
    if include_unmatched {
        let mut um_stmt = conn.prepare(
            "SELECT rp.id, rp.title, rp.authors, rp.publication_year, rp.journal, \
                     rp.abstract_text, l.parent_article_id, l.type, \
                     rp.citation_count, rp.reference_count, rp.reference_type \
              FROM reference_papers rp \
              JOIN article_reference_links l ON l.reference_paper_id = rp.id \
              WHERE rp.matched_article_id IS NULL \
                AND EXISTS (SELECT 1 FROM articles a WHERE a.id = l.parent_article_id AND a.status = 'included')",
        )?;
        let unmatched: Vec<(
            String,
            Option<String>,
            Option<String>,
            Option<i32>,
            Option<String>,
            Option<String>,
            String,
            i32,
            i64,
            i64,
            Option<String>,
        )> = um_stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (
            pid,
            title,
            authors,
            year,
            journal,
            abstract_text,
            parent_id,
            l_type,
            citation_count,
            reference_count,
            reference_type,
        ) in unmatched
        {
            let (source, target) = if l_type == 0 {
                (pid.clone(), parent_id.clone())
            } else {
                (parent_id.clone(), pid.clone())
            };

            // Faint dashed edge matching the citation relationship (source → target: source cites target).
            edges.push(serde_json::json!({
                "source": source,
                "target": target,
                "weight": 0.1,
                "unmatched": true,
            }));

            // Dedup: skip if we've already added this paper.
            if nodes.iter().any(|n| n["id"] == pid) {
                continue;
            }
            let authors_str = authors.as_deref().unwrap_or("");
            let label = format_paper_label(authors_str, year);
            nodes.push(serde_json::json!({
                "id": pid,
                "label": label,
                "title": title.unwrap_or_default(),
                "authors": authors.unwrap_or_default(),
                "year": year,
                "journal": journal,
                "numCited": reference_count,
                "numReferences": citation_count,
                "abstract": abstract_text.unwrap_or_default(),
                "unmatched": true,
                "referenceType": reference_type,
            }));
        }
    }

    // Diagnostic meta block - drives the frontend empty-state messaging.
    let included_article_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |r| r.get(0))
        .unwrap_or(0);
    let reference_paper_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM reference_papers", [], |r| r.get(0)).unwrap_or(0);
    let matched_paper_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reference_papers WHERE matched_article_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let matched_edge_count =
        edges.iter().filter(|e| !e["unmatched"].as_bool().unwrap_or(false)).count();
    let unmatched_node_count =
        nodes.iter().filter(|n| n["unmatched"].as_bool().unwrap_or(false)).count();

    Ok(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "meta": {
            "nodeCount": nodes.len(),
            "edgeCount": matched_edge_count,
            "unmatchedCount": unmatched_node_count,
            "includedArticleCount": included_article_count,
            "referencePaperCount": reference_paper_count,
            "matchedPaperCount": matched_paper_count,
        }
    }))
}
