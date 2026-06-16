use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use crate::error::AppError;

/// Generate the Keyword Co-Occurrence network dynamically from the database.
///
/// Merges and stems terms from active sources, filters by thresholds, and returns
/// Gephi-compatible Node/Edge JSON structures.
#[allow(clippy::type_complexity)]
pub fn get_keyword_network_json(
    conn: &Connection,
    sources: &[String],
    min_occurrences: i32,
    min_cooccurrence: i32,
) -> Result<serde_json::Value, AppError> {
    struct RawTermRow {
        article_id: String,
        year: Option<i32>,
        raw_term: String,
        source: String,
    }
    let mut rows: Vec<RawTermRow> = Vec::new();

    let mut term_sources = Vec::new();
    for src in sources {
        if src == "metadata" || src == "ai_extracted" || src == "user_added" {
            term_sources.push(src.clone());
        }
    }

    if !term_sources.is_empty() {
        let placeholders: String = term_sources.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT bat.article_id, a.publication_year, bt.raw_term, bt.source \
             FROM biblio_article_terms bat \
             JOIN articles a ON a.id = bat.article_id \
             JOIN biblio_terms bt ON bt.id = bat.term_id \
             WHERE a.status = 'included' AND bt.source IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let params = rusqlite::params_from_iter(term_sources.iter());
        let term_rows = stmt
            .query_map(params, |row| {
                Ok(RawTermRow {
                    article_id: row.get(0)?,
                    year: row.get(1)?,
                    raw_term: row.get(2)?,
                    source: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.extend(term_rows);
    }

    if sources.iter().any(|s| s == "tags") {
        let mut stmt = conn.prepare(
            "SELECT at.article_id, a.publication_year, t.name, 'tags' \
             FROM article_tags at \
             JOIN articles a ON a.id = at.article_id \
             JOIN tags t ON t.id = at.tag_id \
             WHERE a.status = 'included'",
        )?;
        let tag_rows = stmt
            .query_map([], |row| {
                Ok(RawTermRow {
                    article_id: row.get(0)?,
                    year: row.get(1)?,
                    raw_term: row.get(2)?,
                    source: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.extend(tag_rows);
    }

    if sources.iter().any(|s| s == "labels") {
        let mut stmt = conn.prepare(
            "SELECT al.article_id, a.publication_year, l.name, 'labels' \
             FROM article_labels al \
             JOIN articles a ON a.id = al.article_id \
             JOIN labels l ON l.id = al.label_id \
             WHERE a.status = 'included'",
        )?;
        let label_rows = stmt
            .query_map([], |row| {
                Ok(RawTermRow {
                    article_id: row.get(0)?,
                    year: row.get(1)?,
                    raw_term: row.get(2)?,
                    source: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.extend(label_rows);
    }

    // Map of normalized_term -> (HashSet<article_id>, Vec<year>, HashMap<raw_term, count>, HashMap<source, count>)
    let mut term_aggregates: HashMap<
        String,
        (HashSet<String>, Vec<i32>, HashMap<String, usize>, HashMap<String, usize>),
    > = HashMap::new();

    for row in rows {
        let norm = crate::biblio::normalizer::normalize_term(&row.raw_term);
        if norm.is_empty() {
            continue;
        }

        let entry = term_aggregates
            .entry(norm)
            .or_insert_with(|| (HashSet::new(), Vec::new(), HashMap::new(), HashMap::new()));

        entry.0.insert(row.article_id);
        if let Some(y) = row.year {
            if y > 0 {
                entry.1.push(y);
            }
        }
        *entry.2.entry(row.raw_term).or_insert(0) += 1;
        *entry.3.entry(row.source).or_insert(0) += 1;
    }

    struct TempNode {
        id: String,
        label: String,
        weight: usize,
        source: String,
        avg_year: Option<f64>,
        raw_terms: Vec<String>,
        articles: HashSet<String>,
        years: Vec<i32>,
    }

    let mut temp_nodes = Vec::new();
    for (norm, (articles, years, raw_terms_map, sources_map)) in term_aggregates {
        let weight = articles.len();
        if weight < min_occurrences as usize {
            continue;
        }

        let mut raw_terms: Vec<(String, usize)> = raw_terms_map.into_iter().collect();
        raw_terms.sort_by_key(|b| std::cmp::Reverse(b.1)); // Sort by count descending
        let label = raw_terms.first().map(|(term, _)| term.clone()).unwrap_or_else(|| norm.clone());
        let raw_terms_list: Vec<String> = raw_terms.into_iter().map(|(term, _)| term).collect();

        let source = sources_map
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(src, _)| src)
            .unwrap_or_else(|| "metadata".to_string());

        let avg_year = if !years.is_empty() {
            let sum: i32 = years.iter().sum();
            Some(sum as f64 / years.len() as f64)
        } else {
            None
        };

        temp_nodes.push(TempNode {
            id: norm,
            label,
            weight,
            source,
            avg_year,
            raw_terms: raw_terms_list,
            articles,
            years,
        });
    }

    let mut edges = Vec::new();
    let n = temp_nodes.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let u_articles = &temp_nodes[i].articles;
            let v_articles = &temp_nodes[j].articles;
            let intersection_count = u_articles.intersection(v_articles).count();
            if intersection_count >= min_cooccurrence as usize {
                edges.push(serde_json::json!({
                    "source": temp_nodes[i].id,
                    "target": temp_nodes[j].id,
                    "weight": intersection_count as f64,
                }));
            }
        }
    }

    let nodes: Vec<serde_json::Value> = temp_nodes
        .into_iter()
        .map(|node| {
            let mut year_map: HashMap<i32, i32> = HashMap::new();
            for y in &node.years {
                *year_map.entry(*y).or_insert(0) += 1;
            }
            let mut year_counts: Vec<serde_json::Value> = year_map
                .into_iter()
                .map(|(year, count)| {
                    serde_json::json!({
                        "year": year,
                        "count": count,
                    })
                })
                .collect();
            year_counts.sort_by_key(|v| v["year"].as_i64().unwrap_or(0));

            serde_json::json!({
                "id": node.id,
                "label": node.label,
                "weight": node.weight as f64,
                "source": node.source,
                "avgYear": node.avg_year,
                "rawTerms": node.raw_terms,
                "yearCounts": year_counts,
            })
        })
        .collect();

    let total_articles = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);

    Ok(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "meta": {
            "nodeCount": nodes.len(),
            "edgeCount": edges.len(),
            "totalArticles": total_articles,
            "sourcesActive": sources,
        }
    }))
}
