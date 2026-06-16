use rusqlite::{Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

use super::super::authors::get_all_authors;
use crate::error::AppError;

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
