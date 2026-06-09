use rusqlite::Connection;

use crate::error::AppError;
use crate::models::biblio::{
    BiblioArticleAuthor, BiblioAuthor, BiblioNetworkEdge, BiblioNetworkMeta, BiblioNetworkNode,
    BiblioStatus, BiblioTerm, NetworkType, TermType,
};

// =============================================================================
// Terms
// =============================================================================

/// Upsert a term: insert if new normalized_term+term_type combo, otherwise increment article_count.
/// Returns the term ID.
pub fn upsert_term(
    conn: &Connection,
    raw_term: &str,
    normalized_term: &str,
    term_type: &TermType,
) -> Result<String, AppError> {
    let type_str = term_type.to_string();

    // Try to find existing
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_terms WHERE normalized_term = ?1 AND term_type = ?2",
            rusqlite::params![normalized_term, type_str],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        // Increment article_count
        conn.execute(
            "UPDATE biblio_terms SET article_count = article_count + 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(id)
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_terms (id, normalized_term, raw_term, term_type, article_count) \
             VALUES (?1, ?2, ?3, ?4, 1)",
            rusqlite::params![id, normalized_term, raw_term, type_str],
        )?;
        Ok(id)
    }
}

/// Link an article to a term. Creates or increments frequency.
pub fn link_article_term(
    conn: &Connection,
    article_id: &str,
    term_id: &str,
) -> Result<(), AppError> {
    // Check if link exists
    let existing: Option<i32> = conn
        .query_row(
            "SELECT frequency FROM biblio_article_terms WHERE article_id = ?1 AND term_id = ?2",
            rusqlite::params![article_id, term_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(freq) = existing {
        conn.execute(
            "UPDATE biblio_article_terms SET frequency = ?1 WHERE article_id = ?2 AND term_id = ?3",
            rusqlite::params![freq + 1, article_id, term_id],
        )?;
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_article_terms (id, article_id, term_id, frequency) VALUES (?1, ?2, ?3, 1)",
            rusqlite::params![id, article_id, term_id],
        )?;
    }
    Ok(())
}

/// Get all terms linked to an article.
pub fn get_terms_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<BiblioTerm>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.normalized_term, t.raw_term, t.term_type, t.article_count, t.created_at \
         FROM biblio_terms t \
         JOIN biblio_article_terms bat ON t.id = bat.term_id \
         WHERE bat.article_id = ?1 \
         ORDER BY t.normalized_term",
    )?;
    let terms = stmt
        .query_map(rusqlite::params![article_id], |row| {
            let type_str: String = row.get(3)?;
            let term_type =
                if type_str == "noun_phrase" { TermType::NounPhrase } else { TermType::Keyword };
            Ok(BiblioTerm {
                id: row.get(0)?,
                normalized_term: row.get(1)?,
                raw_term: row.get(2)?,
                term_type,
                article_count: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(terms)
}

/// Save extracted terms (from LLM noun-phrase extraction or keywords) for an article.
pub fn save_article_terms(
    conn: &Connection,
    article_id: &str,
    terms: &[(String, TermType)],
) -> Result<(), AppError> {
    for (raw_term, term_type) in terms {
        let normalized = crate::biblio::normalizer::normalize_term(raw_term);
        if normalized.is_empty() {
            continue;
        }
        let term_id = upsert_term(conn, raw_term, &normalized, term_type)?;
        let _ = link_article_term(conn, article_id, &term_id);
    }
    Ok(())
}

// =============================================================================
// Authors
// =============================================================================

/// Upsert a normalized author. Returns the author ID.
pub fn upsert_author(
    conn: &Connection,
    normalized_name: &str,
    display_name: &str,
) -> Result<String, AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_authors WHERE normalized_name = ?1",
            rusqlite::params![normalized_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE biblio_authors SET article_count = article_count + 1 WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(id)
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_authors (id, normalized_name, display_name, first_author_count, article_count) \
             VALUES (?1, ?2, ?3, 0, 1)",
            rusqlite::params![id, normalized_name, display_name],
        )?;
        Ok(id)
    }
}

/// Link an article to an author at a specific order position.
pub fn link_article_author(
    conn: &Connection,
    article_id: &str,
    author_id: &str,
    author_order: i32,
    raw_name: Option<&str>,
    raw_affiliation: Option<&str>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO biblio_article_authors (id, article_id, author_id, author_order, raw_name, raw_affiliation) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, article_id, author_id, author_order, raw_name, raw_affiliation],
    )?;

    // If first author, update first_author_count
    if author_order == 0 {
        conn.execute(
            "UPDATE biblio_authors SET first_author_count = first_author_count + 1 WHERE id = ?1",
            rusqlite::params![author_id],
        )?;
    }
    Ok(())
}

/// Get all authors linked to an article.
pub fn get_authors_for_article(
    conn: &Connection,
    article_id: &str,
) -> Result<Vec<BiblioArticleAuthor>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, article_id, author_id, author_order, raw_name, raw_affiliation \
         FROM biblio_article_authors \
         WHERE article_id = ?1 \
         ORDER BY author_order",
    )?;
    let links = stmt
        .query_map(rusqlite::params![article_id], |row| {
            Ok(BiblioArticleAuthor {
                id: row.get(0)?,
                article_id: row.get(1)?,
                author_id: row.get(2)?,
                author_order: row.get(3)?,
                raw_name: row.get(4)?,
                raw_affiliation: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(links)
}

// =============================================================================
// Institutions
// =============================================================================

/// Upsert an institution. Returns the institution ID.
pub fn upsert_institution(
    conn: &Connection,
    normalized_name: &str,
    country: Option<&str>,
    city: Option<&str>,
) -> Result<String, AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_institutions WHERE normalized_name = ?1",
            rusqlite::params![normalized_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        Ok(id)
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_institutions (id, normalized_name, country, city) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, normalized_name, country, city],
        )?;
        Ok(id)
    }
}

// =============================================================================
// Networks
// =============================================================================

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
        .ok();
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

// =============================================================================
// Refresh / Clear
// =============================================================================

/// Clear all biblio tables (for refresh).
pub fn clear_all_biblio(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM biblio_article_terms", [])?;
    conn.execute("DELETE FROM biblio_article_authors", [])?;
    conn.execute("DELETE FROM biblio_author_affiliations", [])?;
    conn.execute("DELETE FROM biblio_network_edges", [])?;
    conn.execute("DELETE FROM biblio_network_nodes", [])?;
    conn.execute("DELETE FROM biblio_network_meta", [])?;
    conn.execute("DELETE FROM biblio_terms", [])?;
    conn.execute("DELETE FROM biblio_authors", [])?;
    conn.execute("DELETE FROM biblio_institutions", [])?;
    Ok(())
}

/// Get bibliometrics status (row counts).
pub fn get_biblio_status(conn: &Connection) -> Result<BiblioStatus, AppError> {
    let author_count: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_authors", [], |r| r.get(0))?;
    let institution_count: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_institutions", [], |r| r.get(0))?;
    let term_count: i32 = conn.query_row("SELECT COUNT(*) FROM biblio_terms", [], |r| r.get(0))?;
    let article_author_links: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_article_authors", [], |r| r.get(0))?;
    let article_term_links: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_article_terms", [], |r| r.get(0))?;
    let network_count: i32 =
        conn.query_row("SELECT COUNT(*) FROM biblio_network_meta", [], |r| r.get(0))?;
    Ok(BiblioStatus {
        author_count,
        institution_count,
        term_count,
        article_author_links,
        article_term_links,
        network_count,
    })
}

// =============================================================================
// Batch normalization (called from biblio_normalize command)
// =============================================================================

/// Extract and normalize all authors from the articles table into biblio tables.
/// Returns the number of unique authors created.
pub fn normalize_authors_from_articles(conn: &Connection) -> Result<usize, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, authors FROM articles WHERE authors IS NOT NULL AND authors != ''")?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;

    for (article_id, authors_str) in &rows {
        let parsed = crate::biblio::normalizer::parse_authors(authors_str);
        for (order, author) in parsed.iter().enumerate() {
            let norm = crate::biblio::normalizer::normalize_author_name(&author.raw);
            let display = crate::biblio::normalizer::build_display_name(&author.raw);
            if norm.is_empty() {
                continue;
            }
            let author_id = upsert_author(conn, &norm, &display)?;
            link_article_author(
                conn,
                article_id,
                &author_id,
                order as i32,
                Some(&author.raw),
                None,
            )?;
        }
    }

    // Count unique authors
    let unique: i32 = conn.query_row("SELECT COUNT(*) FROM biblio_authors", [], |r| r.get(0))?;
    Ok(unique as usize)
}

/// Extract terms from article keywords, titles, and abstracts.
/// Returns the number of unique terms created.
pub fn normalize_terms_from_articles(conn: &Connection) -> Result<usize, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, keywords, title, abstract_text FROM articles \
         WHERE status != 'excluded' \
         AND id NOT IN (SELECT DISTINCT article_id FROM biblio_article_terms)",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    for (article_id, keywords, title, abstract_text) in &rows {
        let mut terms: Vec<(String, TermType)> = Vec::new();

        // Extract keywords from metadata
        if let Some(kw) = keywords {
            for k in kw.split(';').chain(kw.split(',')) {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    terms.push((trimmed.to_string(), TermType::Keyword));
                }
            }
        }

        // Extract noun phrases from title + abstract (simple approach: significant words)
        let text = format!(
            "{} {}",
            title.as_deref().unwrap_or(""),
            abstract_text.as_deref().unwrap_or("")
        );
        for word in extract_significant_words(&text) {
            terms.push((word, TermType::NounPhrase));
        }

        save_article_terms(conn, article_id, &terms)?;
    }

    let unique: i32 = conn.query_row("SELECT COUNT(*) FROM biblio_terms", [], |r| r.get(0))?;
    Ok(unique as usize)
}

/// Build coauthor edges from the biblio_article_authors table.
/// Two authors are connected if they appear on the same article.
/// Returns the number of edges created.
pub fn build_coauthor_edges(conn: &Connection) -> Result<usize, AppError> {
    // For each article, connect all pairs of authors
    let mut stmt = conn.prepare(
        "SELECT article_id, author_id FROM biblio_article_authors ORDER BY article_id, author_order"
    )?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;

    // Group by article
    let mut article_authors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (article_id, author_id) in rows {
        article_authors.entry(article_id).or_default().push(author_id);
    }

    // Create edges for each pair
    let mut edge_counts: std::collections::HashMap<(String, String), i32> =
        std::collections::HashMap::new();
    for authors in article_authors.values() {
        for i in 0..authors.len() {
            for j in (i + 1)..authors.len() {
                let mut key = (authors[i].clone(), authors[j].clone());
                if key.0 > key.1 {
                    std::mem::swap(&mut key.0, &mut key.1);
                }
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    // Save as a co-authorship network
    let network_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, node_count, edge_count) VALUES (?1, 'co_authorship', 'Co-authorship', 0, ?2)",
        rusqlite::params![network_id, edge_counts.len() as i32],
    )?;

    for ((source, target), count) in &edge_counts {
        let edge_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![edge_id, network_id, source, target, *count as f64],
        )?;
    }

    // Update node count
    let unique_authors: std::collections::HashSet<&str> =
        edge_counts.keys().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect();
    conn.execute(
        "UPDATE biblio_network_meta SET node_count = ?1 WHERE id = ?2",
        rusqlite::params![unique_authors.len() as i32, network_id],
    )?;

    Ok(edge_counts.len())
}

/// Get all normalized authors.
pub fn get_all_authors(conn: &Connection) -> Result<Vec<BiblioAuthor>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_name, display_name, first_author_count, article_count, created_at \
         FROM biblio_authors ORDER BY article_count DESC",
    )?;
    let authors = stmt
        .query_map([], |row| {
            Ok(BiblioAuthor {
                id: row.get(0)?,
                normalized_name: row.get(1)?,
                display_name: row.get(2)?,
                first_author_count: row.get(3)?,
                article_count: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(authors)
}

/// Get all terms.
pub fn get_all_terms(conn: &Connection) -> Result<Vec<BiblioTerm>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_term, raw_term, term_type, article_count, created_at \
         FROM biblio_terms ORDER BY article_count DESC",
    )?;
    let terms = stmt
        .query_map([], |row| {
            let type_str: String = row.get(3)?;
            let term_type =
                if type_str == "noun_phrase" { TermType::NounPhrase } else { TermType::Keyword };
            Ok(BiblioTerm {
                id: row.get(0)?,
                normalized_term: row.get(1)?,
                raw_term: row.get(2)?,
                term_type,
                article_count: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(terms)
}

/// Get coauthor network as JSON for graph rendering.
pub fn get_coauthor_network_json(conn: &Connection) -> Result<serde_json::Value, AppError> {
    let authors = get_all_authors(conn)?;
    let nodes: Vec<serde_json::Value> = authors
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "label": a.display_name,
                "weight": a.article_count,
            })
        })
        .collect();

    let mut edges_stmt = conn.prepare(
        "SELECT source_id, target_id, weight FROM biblio_network_edges WHERE network_id IN \
         (SELECT id FROM biblio_network_meta WHERE network_type = 'co_authorship') ORDER BY weight DESC"
    )?;
    let edges: Vec<serde_json::Value> = edges_stmt
        .query_map([], |row| {
            let source: String = row.get(0)?;
            let target: String = row.get(1)?;
            let weight: f64 = row.get(2)?;
            Ok(serde_json::json!({ "source": source, "target": target, "weight": weight }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
}

/// Extract significant words (potential terms) from text.
/// Filters common stop words and short words.
fn extract_significant_words(text: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "the",
        "a",
        "an",
        "and",
        "or",
        "of",
        "in",
        "to",
        "for",
        "with",
        "on",
        "at",
        "by",
        "from",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "may",
        "might",
        "this",
        "that",
        "these",
        "those",
        "it",
        "its",
        "we",
        "our",
        "they",
        "their",
        "which",
        "who",
        "whom",
        "what",
        "where",
        "when",
        "how",
        "not",
        "no",
        "nor",
        "as",
        "if",
        "then",
        "than",
        "too",
        "very",
        "can",
        "but",
        "however",
        "also",
        "such",
        "more",
        "most",
        "other",
        "some",
        "any",
        "all",
        "each",
        "every",
        "between",
        "through",
        "during",
        "before",
        "after",
        "above",
        "below",
        "about",
        "up",
        "out",
        "into",
        "over",
        "under",
        "again",
        "further",
        "using",
        "based",
        "used",
        "use",
        "using",
        "study",
        "studies",
        "result",
        "results",
        "method",
        "methods",
        "approach",
        "approaches",
        "also",
        "well",
    ];

    text.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 4)
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

// =============================================================================
// Helpers
// =============================================================================

fn parse_network_type(s: &str) -> NetworkType {
    match s {
        "co_authorship" => NetworkType::CoAuthorship,
        "co_occurrence" => NetworkType::CoOccurrence,
        "citation" => NetworkType::Citation,
        "biblio_coupling" => NetworkType::BiblioCoupling,
        "co_citation" => NetworkType::CoCitation,
        _ => NetworkType::CoOccurrence, // fallback
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migration::run_migrations;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn insert_test_article(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status) VALUES (?1, 'Test', 'Abstract', 'Smith J', 'included')",
            rusqlite::params![id],
        ).unwrap();
    }

    // ── Term operations ─────────────────────────────────────────

    #[test]
    fn test_upsert_term_creates_new() {
        let conn = test_db();
        let id =
            upsert_term(&conn, "Machine Learning", "machine learning", &TermType::Keyword).unwrap();
        assert!(!id.is_empty());

        let count: i32 = conn
            .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_term_increments_count() {
        let conn = test_db();
        let id1 =
            upsert_term(&conn, "Machine Learning", "machine learning", &TermType::Keyword).unwrap();
        let id2 =
            upsert_term(&conn, "machine learning", "machine learning", &TermType::Keyword).unwrap();
        assert_eq!(id1, id2);

        let count: i32 = conn
            .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id1], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_upsert_term_different_types() {
        let conn = test_db();
        let id_kw = upsert_term(&conn, "ML", "ml", &TermType::Keyword).unwrap();
        let id_np = upsert_term(&conn, "ML", "ml", &TermType::NounPhrase).unwrap();
        assert_ne!(id_kw, id_np);
    }

    #[test]
    fn test_link_article_term_creates_link() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let term_id = upsert_term(&conn, "AI", "ai", &TermType::Keyword).unwrap();
        link_article_term(&conn, "art1", &term_id).unwrap();

        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM biblio_article_terms WHERE article_id = ?1",
                ["art1"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_link_article_term_increments_frequency() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let term_id = upsert_term(&conn, "AI", "ai", &TermType::Keyword).unwrap();
        link_article_term(&conn, "art1", &term_id).unwrap();
        link_article_term(&conn, "art1", &term_id).unwrap();

        let freq: i32 = conn
            .query_row(
                "SELECT frequency FROM biblio_article_terms WHERE article_id = ?1 AND term_id = ?2",
                rusqlite::params!["art1", term_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(freq, 2);
    }

    #[test]
    fn test_get_terms_for_article() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let t1 =
            upsert_term(&conn, "Machine Learning", "machine learning", &TermType::Keyword).unwrap();
        let t2 =
            upsert_term(&conn, "neural network", "neural network", &TermType::NounPhrase).unwrap();
        link_article_term(&conn, "art1", &t1).unwrap();
        link_article_term(&conn, "art1", &t2).unwrap();

        let terms = get_terms_for_article(&conn, "art1").unwrap();
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_save_article_terms() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        save_article_terms(
            &conn,
            "art1",
            &[
                ("Machine Learning".to_string(), TermType::Keyword),
                ("deep learning".to_string(), TermType::NounPhrase),
                ("machine learning".to_string(), TermType::Keyword), // duplicate normalized
            ],
        )
        .unwrap();

        let terms = get_terms_for_article(&conn, "art1").unwrap();
        assert_eq!(terms.len(), 2); // "machine learning" deduplicated
    }

    // ── Author operations ───────────────────────────────────────

    #[test]
    fn test_upsert_author_creates_new() {
        let conn = test_db();
        let id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_upsert_author_increments_count() {
        let conn = test_db();
        let id1 = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        let id2 = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        assert_eq!(id1, id2);

        let count: i32 = conn
            .query_row("SELECT article_count FROM biblio_authors WHERE id = ?1", [&id1], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_link_article_author() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let author_id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        link_article_author(&conn, "art1", &author_id, 0, Some("Smith J"), None).unwrap();

        let links = get_authors_for_article(&conn, "art1").unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].author_order, 0);
    }

    #[test]
    fn test_first_author_count_updated() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let author_id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        link_article_author(&conn, "art1", &author_id, 0, None, None).unwrap();

        let count: i32 = conn
            .query_row(
                "SELECT first_author_count FROM biblio_authors WHERE id = ?1",
                [&author_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── Institution operations ──────────────────────────────────

    #[test]
    fn test_upsert_institution_creates_new() {
        let conn = test_db();
        let id = upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_upsert_institution_returns_same() {
        let conn = test_db();
        let id1 = upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
        let id2 = upsert_institution(&conn, "mit", None, None).unwrap();
        assert_eq!(id1, id2);
    }

    // ── Network operations ──────────────────────────────────────

    #[test]
    fn test_save_and_load_network() {
        let conn = test_db();
        let nodes = vec![BiblioNetworkNode {
            id: String::new(),
            network_id: String::new(),
            entity_id: "author1".to_string(),
            label: "Smith".to_string(),
            weight: 5.0,
            cluster: Some(1),
            x: Some(0.5),
            y: Some(0.3),
        }];
        let edges = vec![BiblioNetworkEdge {
            id: String::new(),
            network_id: String::new(),
            source_id: "author1".to_string(),
            target_id: "author2".to_string(),
            weight: 3.0,
        }];

        let net_id = save_network(
            &conn,
            &NetworkType::CoAuthorship,
            "Test Network",
            None,
            None,
            &nodes,
            &edges,
        )
        .unwrap();

        let meta = load_network(&conn, &net_id).unwrap().unwrap();
        assert_eq!(meta.label, "Test Network");
        assert_eq!(meta.node_count, 1);
        assert_eq!(meta.edge_count, 1);

        let loaded_nodes = load_network_nodes(&conn, &net_id).unwrap();
        assert_eq!(loaded_nodes.len(), 1);
        assert_eq!(loaded_nodes[0].entity_id, "author1");

        let loaded_edges = load_network_edges(&conn, &net_id).unwrap();
        assert_eq!(loaded_edges.len(), 1);
    }

    #[test]
    fn test_delete_network_cascades() {
        let conn = test_db();
        let net_id = save_network(
            &conn,
            &NetworkType::CoOccurrence,
            "To Delete",
            None,
            None,
            &[BiblioNetworkNode {
                id: String::new(),
                network_id: String::new(),
                entity_id: "t1".to_string(),
                label: "term".to_string(),
                weight: 1.0,
                cluster: None,
                x: None,
                y: None,
            }],
            &[],
        )
        .unwrap();

        delete_network(&conn, &net_id).unwrap();
        assert!(load_network(&conn, &net_id).unwrap().is_none());
    }

    // ── Clear and Status ────────────────────────────────────────

    #[test]
    fn test_clear_all_biblio() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        upsert_term(&conn, "AI", "ai", &TermType::Keyword).unwrap();
        upsert_author(&conn, "smith j", "Smith, J.").unwrap();

        clear_all_biblio(&conn).unwrap();

        let status = get_biblio_status(&conn).unwrap();
        assert_eq!(status.author_count, 0);
        assert_eq!(status.term_count, 0);
    }

    #[test]
    fn test_get_biblio_status() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        upsert_term(&conn, "AI", "ai", &TermType::Keyword).unwrap();
        upsert_author(&conn, "smith j", "Smith, J.").unwrap();

        let status = get_biblio_status(&conn).unwrap();
        assert_eq!(status.author_count, 1);
        assert_eq!(status.term_count, 1);
    }

    #[test]
    fn test_refresh_clears_and_repopulates() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        insert_test_article(&conn, "art2");

        // Populate
        save_article_terms(&conn, "art1", &[("ML".to_string(), TermType::Keyword)]).unwrap();
        save_article_terms(&conn, "art2", &[("AI".to_string(), TermType::Keyword)]).unwrap();

        let status_before = get_biblio_status(&conn).unwrap();
        assert_eq!(status_before.term_count, 2);
        assert_eq!(status_before.article_term_links, 2);

        // Clear and repopulate
        clear_all_biblio(&conn).unwrap();
        let status_cleared = get_biblio_status(&conn).unwrap();
        assert_eq!(status_cleared.term_count, 0);

        // Repopulate
        save_article_terms(&conn, "art1", &[("ML".to_string(), TermType::Keyword)]).unwrap();
        save_article_terms(&conn, "art2", &[("AI".to_string(), TermType::Keyword)]).unwrap();

        let status_after = get_biblio_status(&conn).unwrap();
        assert_eq!(status_after.term_count, 2);
        assert_eq!(status_after.article_term_links, 2);
    }
}
