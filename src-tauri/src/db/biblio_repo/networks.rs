use std::collections::{HashMap, HashSet};
use rusqlite::{Connection, OptionalExtension};

use crate::db::reference_repo;
use crate::error::AppError;
use crate::models::biblio::{
    BiblioNetworkEdge, BiblioNetworkMeta, BiblioNetworkNode, NetworkType,
};
use super::authors::get_all_authors;

/// Save a network with its nodes and edges. Returns the network ID.
pub fn save_network(
    conn: &Connection,
    network_type: &NetworkType,
    label: &str,
    article_filter: Option<&str>,
    params_json: Option<&str>,
    nodes: &[BiblioNetworkNode],
    edges: &[BiblioNetworkEdge],
) -> Result<String, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let type_str = network_type.to_string();
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, article_filter, params_json, node_count, edge_count) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, type_str, label, article_filter, params_json, nodes.len() as i32, edges.len() as i32],
    )?;

    for node in nodes {
        let node_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_nodes (id, network_id, entity_id, label, weight, cluster, x, y) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                node_id, id, node.entity_id, node.label, node.weight, node.cluster, node.x, node.y
            ],
        )?;
    }

    for edge in edges {
        let edge_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![edge_id, id, edge.source_id, edge.target_id, edge.weight],
        )?;
    }

    Ok(id)
}

/// Load a network by ID.
pub fn load_network(
    conn: &Connection,
    network_id: &str,
) -> Result<Option<BiblioNetworkMeta>, AppError> {
    let result = conn
        .query_row(
            "SELECT id, network_type, label, article_filter, params_json, node_count, edge_count, created_at \
             FROM biblio_network_meta WHERE id = ?1",
            rusqlite::params![network_id],
            |row| {
                let type_str: String = row.get(1)?;
                let network_type = parse_network_type(&type_str);
                Ok(BiblioNetworkMeta {
                    id: row.get(0)?,
                    network_type,
                    label: row.get(2)?,
                    article_filter: row.get(3)?,
                    params_json: row.get(4)?,
                    node_count: row.get(5)?,
                    edge_count: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()?;
    Ok(result)
}

/// Load nodes for a network.
pub fn load_network_nodes(
    conn: &Connection,
    network_id: &str,
) -> Result<Vec<BiblioNetworkNode>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, network_id, entity_id, label, weight, cluster, x, y \
         FROM biblio_network_nodes WHERE network_id = ?1",
    )?;
    let nodes = stmt
        .query_map(rusqlite::params![network_id], |row| {
            Ok(BiblioNetworkNode {
                id: row.get(0)?,
                network_id: row.get(1)?,
                entity_id: row.get(2)?,
                label: row.get(3)?,
                weight: row.get(4)?,
                cluster: row.get(5)?,
                x: row.get(6)?,
                y: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(nodes)
}

/// Load edges for a network.
pub fn load_network_edges(
    conn: &Connection,
    network_id: &str,
) -> Result<Vec<BiblioNetworkEdge>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, network_id, source_id, target_id, weight \
         FROM biblio_network_edges WHERE network_id = ?1",
    )?;
    let edges = stmt
        .query_map(rusqlite::params![network_id], |row| {
            Ok(BiblioNetworkEdge {
                id: row.get(0)?,
                network_id: row.get(1)?,
                source_id: row.get(2)?,
                target_id: row.get(3)?,
                weight: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(edges)
}

/// Delete a network and all its nodes/edges (CASCADE).
pub fn delete_network(conn: &Connection, network_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM biblio_network_meta WHERE id = ?1", rusqlite::params![network_id])?;
    Ok(())
}

/// Helper parsing network type from string.
fn parse_network_type(s: &str) -> NetworkType {
    match s {
        "co_authorship" => NetworkType::CoAuthorship,
        "co_occurrence" => NetworkType::CoOccurrence,
        _ => NetworkType::Citation,
    }
}

/// Build coauthor edges from the biblio_article_authors table.
/// Two authors are connected if they appear on the same article.
/// Computes both full counting (weight = co-paper count) and fractional counting.
/// Returns the number of edges created.
pub fn build_coauthor_edges(conn: &Connection) -> Result<usize, AppError> {
    // For each article, connect all pairs of authors
    let mut stmt = conn.prepare(
        "SELECT article_id, author_id FROM biblio_article_authors ORDER BY article_id, author_order"
    )?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;

    // Group by article
    let mut article_authors: HashMap<String, Vec<String>> = HashMap::new();
    for (article_id, author_id) in rows {
        article_authors.entry(article_id).or_default().push(author_id);
    }

    // Create edges for each pair — both full and fractional counting
    let mut full_counts: HashMap<(String, String), i32> = HashMap::new();
    let mut fractional_sums: HashMap<(String, String), f64> = HashMap::new();
    // Track max author count per edge (largest author list of any article contributing to this pair)
    let mut max_author_counts: HashMap<(String, String), i32> = HashMap::new();

    for authors in article_authors.values() {
        let n = authors.len();
        if n < 2 {
            continue;
        }
        let pair_count = n * (n - 1) / 2;
        let fractional_weight = 1.0 / pair_count as f64;
        let author_count = n as i32;

        for i in 0..n {
            for j in (i + 1)..n {
                let mut key = (authors[i].clone(), authors[j].clone());
                if key.0 > key.1 {
                    std::mem::swap(&mut key.0, &mut key.1);
                }
                *full_counts.entry(key.clone()).or_insert(0) += 1;
                *fractional_sums.entry(key.clone()).or_insert(0.0) += fractional_weight;
                let current = max_author_counts.get(&key).copied().unwrap_or(0);
                if author_count > current {
                    max_author_counts.insert(key, author_count);
                }
            }
        }
    }

    // Save as a co-authorship network with params documenting counting mode
    let params = serde_json::json!({
        "counting_modes": ["full", "fractional"],
        "description": "weight = full counting (co-paper count), fractional_weight = fractional counting"
    }).to_string();

    let network_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, node_count, edge_count, params_json) VALUES (?1, 'co_authorship', 'Co-authorship', 0, ?2, ?3)",
        rusqlite::params![network_id, full_counts.len() as i32, params],
    )?;

    // Store both counting modes: weight = full count, fractional stored in a separate column
    // Since schema only has `weight`, we store full counting as weight
    // and fractional as a JSON attribute (via params_json on edges if available, or computed client-side)
    for ((source, target), full_count) in &full_counts {
        let edge_id = uuid::Uuid::new_v4().to_string();
        // Store full counting as weight
        conn.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![edge_id, network_id, source, target, *full_count as f64],
        )?;
    }

    // Store fractional counts and max author counts in network metadata
    let fractional_map: Vec<serde_json::Value> = fractional_sums
        .iter()
        .map(|((s, t), w)| {
            let mac = max_author_counts.get(&(s.clone(), t.clone())).copied().unwrap_or(0);
            serde_json::json!({
                "source": s,
                "target": t,
                "fractional_weight": (w * 1000.0).round() / 1000.0,
                "max_author_count": mac,
            })
        })
        .collect();

    // Update network metadata with fractional data and max author counts
    let enriched_params = serde_json::json!({
        "counting_modes": ["full", "fractional"],
        "fractional_edges": fractional_map,
    })
    .to_string();
    conn.execute(
        "UPDATE biblio_network_meta SET params_json = ?1 WHERE id = ?2",
        rusqlite::params![enriched_params, network_id],
    )?;

    // Update node count
    let unique_authors: HashSet<&str> =
        full_counts.keys().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect();
    conn.execute(
        "UPDATE biblio_network_meta SET node_count = ?1 WHERE id = ?2",
        rusqlite::params![unique_authors.len() as i32, network_id],
    )?;

    Ok(full_counts.len())
}

/// Get coauthor network as JSON for graph rendering.
/// Includes author metrics (citations, h-index, avg_year) and fractional edge weights.
pub fn get_coauthor_network_json(conn: &Connection) -> Result<serde_json::Value, AppError> {
    let authors = get_all_authors(conn)?;
    let nodes: Vec<serde_json::Value> = authors
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "label": a.display_name,
                "weight": a.article_count,
                "totalCitations": a.total_citations,
                "estimatedHIndex": a.estimated_h_index,
                "avgYear": a.avg_year,
            })
        })
        .collect();

    // Build a lookup for fractional weights from network params_json
    let mut fractional_lookup: HashMap<(String, String), f64> = HashMap::new();
    let params_json: Option<String> = conn
        .query_row(
            "SELECT params_json FROM biblio_network_meta WHERE network_type = 'co_authorship' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    // Also build a lookup for max_author_count per edge
    let mut max_author_lookup: HashMap<(String, String), i32> = HashMap::new();

    if let Some(ref pj) = params_json {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(pj) {
            if let Some(fe) = parsed.get("fractional_edges").and_then(|v| v.as_array()) {
                for edge in fe {
                    if let (Some(s), Some(t), Some(w)) = (
                        edge.get("source").and_then(|v| v.as_str()),
                        edge.get("target").and_then(|v| v.as_str()),
                        edge.get("fractional_weight").and_then(|v| v.as_f64()),
                    ) {
                        fractional_lookup.insert((s.to_string(), t.to_string()), w);
                    }
                    // Extract max_author_count
                    if let (Some(s), Some(t), Some(mac)) = (
                        edge.get("source").and_then(|v| v.as_str()),
                        edge.get("target").and_then(|v| v.as_str()),
                        edge.get("max_author_count").and_then(|v| v.as_i64()),
                    ) {
                        max_author_lookup.insert((s.to_string(), t.to_string()), mac as i32);
                    }
                }
            }
        }
    }

    let mut edges_stmt = conn.prepare(
        "SELECT source_id, target_id, weight FROM biblio_network_edges WHERE network_id IN \
         (SELECT id FROM biblio_network_meta WHERE network_type = 'co_authorship') ORDER BY weight DESC"
    )?;
    let edges: Vec<serde_json::Value> = edges_stmt
        .query_map([], |row| {
            let source: String = row.get(0)?;
            let target: String = row.get(1)?;
            let weight: f64 = row.get(2)?;
            Ok((source, target, weight))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(source, target, weight)| {
            let frac =
                fractional_lookup.get(&(source.clone(), target.clone())).copied().unwrap_or(0.0);
            let mac =
                max_author_lookup.get(&(source.clone(), target.clone())).copied().unwrap_or(0);
            serde_json::json!({
                "source": source,
                "target": target,
                "weight": weight,
                "fractionalWeight": frac,
                "maxAuthorCount": mac,
            })
        })
        .collect();

    Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
}

/// Format a short paper label from an authors JSON string and optional year.
///
/// Produces `FirstAuthor (Year)` when there is a single author and
/// `FirstAuthor et al. (Year)` when there are multiple authors. When the year
/// is missing the parentheses are omitted entirely.
#[must_use]
pub fn format_paper_label(authors_str: &str, year: Option<i32>) -> String {
    let parsed = crate::biblio::normalizer::parse_authors(authors_str);
    let year_suffix = match year {
        Some(y) => format!(" ({})", y),
        None => String::new(),
    };
    if parsed.is_empty() {
        return format!("Unknown{}", year_suffix);
    }
    // Use the surname portion of the display name ("Last, First" → "Last").
    let first_author = parsed.first().map(|a| a.display_name.split(',').next().unwrap_or(&a.display_name).trim()).unwrap_or("Unknown");
    if parsed.len() == 1 {
        format!("{}{}", first_author, year_suffix)
    } else {
        format!("{} et al.{}", first_author, year_suffix)
    }
}

/// Auto-match reference papers to included articles.
///
/// Walks all `reference_papers` whose `matched_article_id IS NULL` and runs the
/// existing [`reference_repo::auto_match_paper_to_article`] logic (DOI or
/// title+journal+year) against the current article set. Papers that match are
/// updated with their `matched_article_id` and `match_status = 'matched'`.
///
/// Returns the number of papers newly matched.
pub fn auto_match_references_to_articles(conn: &Connection) -> Result<usize, AppError> {
    // Collect candidate papers that have not been matched yet. We only
    // consider papers that are linked to at least one article so we don't
    // waste cycles on orphan reference rows.
    let mut stmt = conn.prepare(
        "SELECT rp.id \
         FROM reference_papers rp \
         WHERE rp.matched_article_id IS NULL \
           AND rp.match_status IN ('unmatched', 'matched') \
           AND EXISTS (SELECT 1 FROM article_reference_links l WHERE l.reference_paper_id = rp.id)",
    )?;
    let paper_ids: Vec<String> =
        stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut matched = 0usize;
    for paper_id in &paper_ids {
        let paper = match reference_repo::get_paper_by_id(conn, paper_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if let Some(article_id) = reference_repo::auto_match_paper_to_article(conn, &paper)? {
            conn.execute(
                "UPDATE reference_papers \
                 SET matched_article_id = ?1, match_status = 'matched', updated_at = datetime('now') \
                 WHERE id = ?2",
                rusqlite::params![article_id, paper_id],
            )?;
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
    // Fetch included articles that appear in the citation network.
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.id, a.title, a.authors, a.publication_year, a.journal, \
                 a.num_cited, a.num_references, a.abstract_text \
          FROM articles a \
          WHERE a.status = 'included' \
            AND a.id IN ( \
                 SELECT source_id FROM biblio_network_edges \
                 WHERE network_id IN (\
                     SELECT id FROM biblio_network_meta WHERE network_type = 'citation'\
                 ) \
                 UNION \
                 SELECT target_id FROM biblio_network_edges \
                 WHERE network_id IN (\
                     SELECT id FROM biblio_network_meta WHERE network_type = 'citation'\
                 ) \
            )",
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
            Ok((id, title, authors, year, journal, num_cited, num_references, abstract_text))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(id, title, authors, year, journal, num_cited, num_references, abstract_text)| {
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
            })
        })
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
                     rp.abstract_text, l.parent_article_id \
              FROM reference_papers rp \
              JOIN article_reference_links l ON l.reference_paper_id = rp.id \
              WHERE rp.matched_article_id IS NULL \
                AND EXISTS (SELECT 1 FROM articles a WHERE a.id = l.parent_article_id AND a.status = 'included')",
        )?;
        let unmatched: Vec<(String, Option<String>, Option<String>, Option<i32>, Option<String>, Option<String>, String)> = um_stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (pid, title, authors, year, journal, abstract_text, parent_id) in unmatched {
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
                "numCited": 0,
                "numReferences": 0,
                "abstract": abstract_text.unwrap_or_default(),
                "unmatched": true,
            }));
            // Faint dashed edge from parent article → unmatched reference leaf.
            edges.push(serde_json::json!({
                "source": parent_id,
                "target": pid,
                "weight": 0.1,
                "unmatched": true,
            }));
        }
    }

    // Diagnostic meta block — drives the frontend empty-state messaging.
    let included_article_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |r| {
            r.get(0)
        })
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
