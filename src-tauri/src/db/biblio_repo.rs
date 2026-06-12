use std::collections::HashMap;

use chrono::Datelike;
use rusqlite::Connection;

use crate::error::AppError;
use crate::models::biblio::{
    BiblioArticleAuthor, BiblioAuthor, BiblioInstitution, BiblioKpis, BiblioNetworkEdge,
    BiblioNetworkMeta, BiblioNetworkNode, BiblioStatus, BiblioTerm, NetworkType, TermSource,
    TermType, YearCount,
};

// =============================================================================
// Terms
// =============================================================================

/// Upsert a term: insert if new normalized_term+term_type combo, otherwise increment article_count.
/// When an AI-extracted term already exists, metadata normalisation reuses it instead of creating a duplicate.
/// Returns the term ID.
pub fn upsert_term(
    conn: &Connection,
    raw_term: &str,
    normalized_term: &str,
    term_type: &TermType,
    source: &TermSource,
) -> Result<String, AppError> {
    let type_str = term_type.to_string();
    let source_str = source.to_string();

    // Try to find existing (by normalized_term + term_type, regardless of source)
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
            "INSERT INTO biblio_terms (id, normalized_term, raw_term, term_type, source, article_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            rusqlite::params![id, normalized_term, raw_term, type_str, source_str],
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
        "SELECT t.id, t.normalized_term, t.raw_term, t.term_type, t.article_count, t.created_at, t.source \
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
            let source_str: String = row.get(6)?;
            let source = match source_str.as_str() {
                "ai_extracted" => TermSource::AiExtracted,
                "user_added" => TermSource::UserAdded,
                _ => TermSource::Metadata,
            };
            Ok(BiblioTerm {
                id: row.get(0)?,
                normalized_term: row.get(1)?,
                raw_term: row.get(2)?,
                term_type,
                source,
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
    terms: &[(String, TermType, TermSource)],
) -> Result<(), AppError> {
    for (raw_term, term_type, source) in terms {
        let normalized = crate::biblio::normalizer::normalize_term(raw_term);
        if normalized.is_empty() {
            continue;
        }
        let term_id = upsert_term(conn, raw_term, &normalized, term_type, source)?;
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

/// Upsert an institution. Returns the institution ID and a boolean indicating if it was newly created.
pub fn upsert_institution(
    conn: &Connection,
    normalized_name: &str,
    country: Option<&str>,
    city: Option<&str>,
) -> Result<(String, bool), AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM biblio_institutions WHERE normalized_name = ?1",
            rusqlite::params![normalized_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        Ok((id, false))
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO biblio_institutions (id, normalized_name, country, city) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, normalized_name, country, city],
        )?;
        Ok((id, true))
    }
}

/// Link an author (per-article) to an institution. Uses INSERT OR IGNORE to avoid duplicates.
pub fn insert_author_affiliation(
    conn: &Connection,
    article_author_id: &str,
    institution_id: &str,
) -> Result<(), AppError> {
    let (article_id, author_id): (String, String) = conn.query_row(
        "SELECT article_id, author_id FROM biblio_article_authors WHERE id = ?1",
        rusqlite::params![article_author_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO biblio_author_affiliations (id, article_id, author_id, institution_id) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), article_id, author_id, institution_id],
    )?;
    Ok(())
}

/// Get all institutions linked to an author (across all their articles).
pub fn get_institutions_by_author(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<BiblioInstitution>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT i.id, i.normalized_name, i.country, i.city, i.created_at \
         FROM biblio_institutions i \
         JOIN biblio_author_affiliations baa ON baa.institution_id = i.id \
         WHERE baa.author_id = ?1 \
         ORDER BY i.normalized_name",
    )?;
    let institutions = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(BiblioInstitution {
                id: row.get(0)?,
                normalized_name: row.get(1)?,
                country: row.get(2)?,
                city: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(institutions)
}

/// Count article-author rows that have a raw_affiliation but no linked institution.
/// These are candidates for LLM-based affiliation normalization.
pub fn count_unmatched_affiliations(conn: &Connection) -> Result<i32, AppError> {
    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_article_authors baa \
             LEFT JOIN biblio_author_affiliations baf ON baf.article_id = baa.article_id AND baf.author_id = baa.author_id \
             WHERE baa.raw_affiliation IS NOT NULL AND baa.raw_affiliation != '' \
             AND baf.id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(count)
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

/// Clear only regeneratable biblio data, preserving AI-extracted and user-added terms.
pub fn clear_regeneratable_biblio(conn: &Connection) -> Result<(), AppError> {
    // Delete links for metadata-sourced terms only
    conn.execute(
        "DELETE FROM biblio_article_terms WHERE term_id IN \
         (SELECT id FROM biblio_terms WHERE source = 'metadata')",
        [],
    )?;
    // Delete only metadata-sourced terms
    conn.execute("DELETE FROM biblio_terms WHERE source = 'metadata'", [])?;

    // Everything else is fully regeneratable
    conn.execute("DELETE FROM biblio_article_authors", [])?;
    conn.execute("DELETE FROM biblio_author_affiliations", [])?;
    conn.execute("DELETE FROM biblio_network_edges", [])?;
    conn.execute("DELETE FROM biblio_network_nodes", [])?;
    conn.execute("DELETE FROM biblio_network_meta", [])?;
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
    let citations_by_year = compute_citations_by_year(conn);

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

// =============================================================================
// Batch normalization (called from biblio_normalize command)
// =============================================================================

/// Extract and normalize all authors from the articles table into biblio tables.
/// Returns the number of unique authors created.
pub fn normalize_authors_from_articles(conn: &Connection) -> Result<usize, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, authors, affiliation FROM articles WHERE status = 'included' AND authors IS NOT NULL AND authors != ''",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    for (article_id, authors_str, affiliation_opt) in &rows {
        let parsed = crate::biblio::normalizer::parse_authors(authors_str);
        let affs: Vec<String> = if let Some(aff_str) = affiliation_opt.as_ref() {
            aff_str.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else {
            Vec::new()
        };

        for (order, author) in parsed.iter().enumerate() {
            let norm = crate::biblio::normalizer::normalize_author_name(&author.raw);
            let display = crate::biblio::normalizer::build_display_name(&author.raw);
            if norm.is_empty() {
                continue;
            }
            let author_id = upsert_author(conn, &norm, &display)?;

            let raw_aff = if order < affs.len() {
                Some(affs[order].as_str())
            } else if affs.len() == 1 {
                Some(affs[0].as_str())
            } else {
                None
            };

            link_article_author(
                conn,
                article_id,
                &author_id,
                order as i32,
                Some(&author.raw),
                raw_aff,
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
         WHERE status = 'included' \
         AND id NOT IN (SELECT DISTINCT article_id FROM biblio_article_terms)",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    for (article_id, keywords, title, abstract_text) in &rows {
        let mut terms: Vec<(String, TermType, TermSource)> = Vec::new();

        // Extract keywords from metadata
        if let Some(kw) = keywords {
            for k in kw.split(';').chain(kw.split(',')) {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    terms.push((trimmed.to_string(), TermType::Keyword, TermSource::Metadata));
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
            terms.push((word, TermType::NounPhrase, TermSource::Metadata));
        }

        save_article_terms(conn, article_id, &terms)?;
    }

    let unique: i32 = conn.query_row("SELECT COUNT(*) FROM biblio_terms", [], |r| r.get(0))?;
    Ok(unique as usize)
}

/// Normalize affiliations from raw_affiliation strings in biblio_article_authors.
/// Parses each raw_affiliation, upserts institutions, and creates links.
/// Returns (institutions_created, links_created).
pub fn normalize_affiliations(conn: &Connection) -> Result<(usize, usize), AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, raw_affiliation FROM biblio_article_authors \
         WHERE raw_affiliation IS NOT NULL AND raw_affiliation != ''",
    )?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;

    let mut institutions_created = 0usize;
    let mut links_created = 0usize;

    for (article_author_id, raw_aff) in &rows {
        let parsed = crate::biblio::normalizer::parse_affiliation(raw_aff);
        if let Some(inst_name) = parsed.institution.as_ref() {
            let normalized =
                inst_name.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.is_empty() {
                continue;
            }
            let (inst_id, created) = upsert_institution(
                conn,
                &normalized,
                parsed.country.as_deref(),
                parsed.city.as_deref(),
            )?;
            if created {
                institutions_created += 1;
            }
            insert_author_affiliation(conn, article_author_id, &inst_id)?;
            links_created += 1;
        }
    }

    Ok((institutions_created, links_created))
}

/// Compute author metrics: total_citations, avg_year, estimated_h_index.
/// Updates all rows in biblio_authors.
pub fn compute_author_metrics(conn: &Connection) -> Result<(), AppError> {
    // Get all author IDs
    let mut stmt = conn.prepare("SELECT id FROM biblio_authors")?;
    let author_ids: Vec<String> =
        stmt.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;

    for author_id in &author_ids {
        // Total citations: sum of num_cited for all articles by this author
        let total_citations: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(a.num_cited), 0) \
                 FROM articles a \
                 JOIN biblio_article_authors baa ON baa.article_id = a.id \
                 WHERE baa.author_id = ?1",
                rusqlite::params![author_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Average publication year
        let avg_year: Option<f64> = conn
            .query_row(
                "SELECT AVG(a.publication_year) \
                 FROM articles a \
                 JOIN biblio_article_authors baa ON baa.article_id = a.id \
                 WHERE baa.author_id = ?1 AND a.publication_year IS NOT NULL",
                rusqlite::params![author_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        // Programmatic h-index (largest h where h papers have >= h citations each)
        let h_index = compute_h_index(conn, author_id);

        conn.execute(
            "UPDATE biblio_authors SET total_citations = ?1, avg_year = ?2, estimated_h_index = ?3 WHERE id = ?4",
            rusqlite::params![total_citations, avg_year, h_index, author_id],
        )?;
    }

    Ok(())
}

/// Compute h-index for a single author programmatically.
fn compute_h_index(conn: &Connection, author_id: &str) -> i32 {
    let mut stmt = match conn.prepare(
        "SELECT a.num_cited FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 AND a.num_cited IS NOT NULL \
         ORDER BY a.num_cited DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = crate::db::audit_repo::log_error(
                conn,
                &format!("biblio: h-index prepare failed for author {author_id}: {e}"),
            );
            return 0;
        }
    };
    let citations: Vec<i32> = match stmt.query_map(rusqlite::params![author_id], |row| row.get(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            let _ = crate::db::audit_repo::log_error(
                conn,
                &format!("biblio: h-index query failed for author {author_id}: {e}"),
            );
            Vec::new()
        }
    };

    let mut h = 0;
    for (i, &cites) in citations.iter().enumerate() {
        if cites >= (i + 1) as i32 {
            h = (i + 1) as i32;
        } else {
            break;
        }
    }
    h
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
    let mut article_authors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (article_id, author_id) in rows {
        article_authors.entry(article_id).or_default().push(author_id);
    }

    // Create edges for each pair — both full and fractional counting
    let mut full_counts: std::collections::HashMap<(String, String), i32> =
        std::collections::HashMap::new();
    let mut fractional_sums: std::collections::HashMap<(String, String), f64> =
        std::collections::HashMap::new();
    // Track max author count per edge (largest author list of any article contributing to this pair)
    let mut max_author_counts: std::collections::HashMap<(String, String), i32> =
        std::collections::HashMap::new();

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
    let unique_authors: std::collections::HashSet<&str> =
        full_counts.keys().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect();
    conn.execute(
        "UPDATE biblio_network_meta SET node_count = ?1 WHERE id = ?2",
        rusqlite::params![unique_authors.len() as i32, network_id],
    )?;

    Ok(full_counts.len())
}

/// Get all normalized authors.
pub fn get_all_authors(conn: &Connection) -> Result<Vec<BiblioAuthor>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_name, display_name, first_author_count, article_count, \
                total_citations, avg_year, estimated_h_index, created_at \
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
                total_citations: row.get(5)?,
                avg_year: row.get(6)?,
                estimated_h_index: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(authors)
}

/// Get all terms.
pub fn get_all_terms(conn: &Connection) -> Result<Vec<BiblioTerm>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, normalized_term, raw_term, term_type, article_count, created_at, source \
         FROM biblio_terms ORDER BY article_count DESC",
    )?;
    let terms = stmt
        .query_map([], |row| {
            let type_str: String = row.get(3)?;
            let term_type =
                if type_str == "noun_phrase" { TermType::NounPhrase } else { TermType::Keyword };
            let source_str: String = row.get(6)?;
            let source = match source_str.as_str() {
                "ai_extracted" => TermSource::AiExtracted,
                "user_added" => TermSource::UserAdded,
                _ => TermSource::Metadata,
            };
            Ok(BiblioTerm {
                id: row.get(0)?,
                normalized_term: row.get(1)?,
                raw_term: row.get(2)?,
                term_type,
                source,
                article_count: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(terms)
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
                "citations": a.total_citations,
                "hIndex": a.estimated_h_index,
                "avgYear": a.avg_year,
            })
        })
        .collect();

    // Build a lookup for fractional weights from network params_json
    let mut fractional_lookup: std::collections::HashMap<(String, String), f64> =
        std::collections::HashMap::new();
    let params_json: Option<String> = conn
        .query_row(
            "SELECT params_json FROM biblio_network_meta WHERE network_type = 'co_authorship' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    // Also build a lookup for max_author_count per edge
    let mut max_author_lookup: std::collections::HashMap<(String, String), i32> =
        std::collections::HashMap::new();

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
// Normalized Citations by Year
// =============================================================================

/// Decay distribution weights (Year 0..5 after publication).
const CITATION_DECAY: [f64; 6] = [0.02, 0.08, 0.13, 0.17, 0.15, 0.11];

/// Compute normalized citations by year.
///
/// For articles **with** citation detail records (`has_citation_details = 1`),
/// group actual linked reference papers by their `publication_year`.
///
/// For articles **without** detail records, spread `num_cited` across years
/// using the decay distribution, with undistributed remainder going to the
/// current year.
fn compute_citations_by_year(conn: &Connection) -> Vec<YearCount> {
    let current_year = chrono::Utc::now().year();
    let mut map: HashMap<i32, i32> = HashMap::new();

    // ── 1. Articles WITH citation detail records ─────────────────
    // Each linked reference paper (type = 0 = citation) counts as 1 citation
    // in the reference paper's publication_year.
    let mut detail_stmt = match conn.prepare(
        "SELECT rp.publication_year \
         FROM article_reference_links arl \
         JOIN reference_papers rp ON rp.id = arl.reference_paper_id \
         JOIN articles a ON a.id = arl.parent_article_id \
         WHERE a.status = 'included' \
           AND arl.type = 0 \
           AND rp.publication_year IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    if let Ok(rows) = detail_stmt.query_map([], |row| {
        let year: i32 = row.get(0)?;
        Ok(year)
    }) {
        for year in rows.flatten() {
            *map.entry(year).or_insert(0) += 1;
        }
    }

    // ── 2. Articles WITHOUT citation detail records ──────────────
    let mut no_detail_stmt = match conn.prepare(
        "SELECT publication_year, num_cited \
         FROM articles \
         WHERE status = 'included' \
           AND has_citation_details = 0 \
           AND publication_year IS NOT NULL \
           AND num_cited IS NOT NULL AND num_cited > 0",
    ) {
        Ok(s) => s,
        Err(_) => return sort_year_map(map),
    };

    if let Ok(rows) = no_detail_stmt.query_map([], |row| {
        let year: i32 = row.get(0)?;
        let cited: i32 = row.get(1)?;
        Ok((year, cited))
    }) {
        for row in rows.flatten() {
            distribute_citations(&mut map, row.0, row.1, current_year);
        }
    }

    sort_year_map(map)
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
        let id = upsert_term(
            &conn,
            "Machine Learning",
            "machine learning",
            &TermType::Keyword,
            &TermSource::Metadata,
        )
        .unwrap();
        assert!(!id.is_empty());

        let count: i32 = conn
            .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_term_increments_count() {
        let conn = test_db();
        let id1 = upsert_term(
            &conn,
            "Machine Learning",
            "machine learning",
            &TermType::Keyword,
            &TermSource::Metadata,
        )
        .unwrap();
        let id2 = upsert_term(
            &conn,
            "machine learning",
            "machine learning",
            &TermType::Keyword,
            &TermSource::Metadata,
        )
        .unwrap();
        assert_eq!(id1, id2);

        let count: i32 = conn
            .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id1], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_upsert_term_different_types() {
        let conn = test_db();
        let id_kw =
            upsert_term(&conn, "ML", "ml", &TermType::Keyword, &TermSource::Metadata).unwrap();
        let id_np = upsert_term(&conn, "ML", "ml", &TermType::NounPhrase, &TermSource::AiExtracted)
            .unwrap();
        assert_ne!(id_kw, id_np);
    }

    #[test]
    fn test_link_article_term_creates_link() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        let term_id =
            upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
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
        let term_id =
            upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
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
        let t1 = upsert_term(
            &conn,
            "Machine Learning",
            "machine learning",
            &TermType::Keyword,
            &TermSource::Metadata,
        )
        .unwrap();
        let t2 = upsert_term(
            &conn,
            "neural network",
            "neural network",
            &TermType::NounPhrase,
            &TermSource::AiExtracted,
        )
        .unwrap();
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
                ("Machine Learning".to_string(), TermType::Keyword, TermSource::Metadata),
                ("deep learning".to_string(), TermType::NounPhrase, TermSource::AiExtracted),
                ("machine learning".to_string(), TermType::Keyword, TermSource::Metadata), // duplicate normalized
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
        let (id, was_created) =
            upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
        assert!(!id.is_empty());
        assert!(was_created);
    }

    #[test]
    fn test_upsert_institution_returns_same() {
        let conn = test_db();
        let (id1, was_created1) =
            upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
        let (id2, was_created2) = upsert_institution(&conn, "mit", None, None).unwrap();
        assert_eq!(id1, id2);
        assert!(was_created1);
        assert!(!was_created2);
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
        upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
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
        upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
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
        save_article_terms(
            &conn,
            "art1",
            &[("ML".to_string(), TermType::Keyword, TermSource::Metadata)],
        )
        .unwrap();
        save_article_terms(
            &conn,
            "art2",
            &[("AI".to_string(), TermType::Keyword, TermSource::Metadata)],
        )
        .unwrap();

        let status_before = get_biblio_status(&conn).unwrap();
        assert_eq!(status_before.term_count, 2);
        assert_eq!(status_before.article_term_links, 2);

        // Clear and repopulate
        clear_all_biblio(&conn).unwrap();
        let status_cleared = get_biblio_status(&conn).unwrap();
        assert_eq!(status_cleared.term_count, 0);

        // Repopulate
        save_article_terms(
            &conn,
            "art1",
            &[("ML".to_string(), TermType::Keyword, TermSource::Metadata)],
        )
        .unwrap();
        save_article_terms(
            &conn,
            "art2",
            &[("AI".to_string(), TermType::Keyword, TermSource::Metadata)],
        )
        .unwrap();

        let status_after = get_biblio_status(&conn).unwrap();
        assert_eq!(status_after.term_count, 2);
        assert_eq!(status_after.article_term_links, 2);
    }

    // ── KPI tests ──────────────────────────────────────────────────

    /// Helper: insert an article with full control over key KPI fields.
    /// `year` is an Option<i32> matching the INTEGER publication_year column.
    fn insert_kpi_article(
        conn: &Connection,
        id: &str,
        status: &str,
        pub_year: Option<i32>,
        num_cited: Option<i32>,
        authors: &str,
    ) {
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited)
             VALUES (?1, 'Test', 'Abstract', ?2, ?3, ?4, ?5)",
            rusqlite::params![id, authors, status, pub_year, num_cited],
        )
        .unwrap();
    }

    #[test]
    fn kpi_empty_db_returns_zeros() {
        let conn = test_db();
        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.included_count, 0);
        assert_eq!(kpis.total_citations, 0);
        assert_eq!(kpis.unique_authors, 0);
        assert_eq!(kpis.year_from, None);
        assert_eq!(kpis.year_to, None);
        assert_eq!(kpis.pubs_per_year, None);
        assert!(kpis.pubs_by_year.is_empty());
        assert_eq!(kpis.avg_growth_rate, None);
        assert!(kpis.refs_by_year.is_empty());
    }

    #[test]
    fn kpi_rejected_only_returns_zeros() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "rejected", Some(2020), Some(5), "Smith J");
        insert_kpi_article(&conn, "a2", "duplicate", Some(2021), Some(10), "Doe A");
        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.included_count, 0);
        assert_eq!(kpis.total_citations, 0);
        assert_eq!(kpis.unique_authors, 0);
        assert_eq!(kpis.year_from, None);
        assert_eq!(kpis.year_to, None);
        assert_eq!(kpis.pubs_per_year, None);
        assert!(kpis.pubs_by_year.is_empty());
    }

    #[test]
    fn kpi_basic_happy_path() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2020), Some(5), "Smith J; Doe A");
        insert_kpi_article(&conn, "a2", "included", Some(2021), Some(10), "Smith J");
        insert_kpi_article(&conn, "a3", "included", Some(2022), Some(15), "Lee K");

        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.included_count, 3);
        assert_eq!(kpis.total_citations, 30);
        assert_eq!(kpis.year_from, Some(2020));
        assert_eq!(kpis.year_to, Some(2022));

        // pubs_by_year: [{2020,1}, {2021,1}, {2022,1}]
        assert_eq!(kpis.pubs_by_year.len(), 3);
        assert_eq!(kpis.pubs_by_year[0], YearCount { year: 2020, count: 1 });
        assert_eq!(kpis.pubs_by_year[2], YearCount { year: 2022, count: 1 });

        // pubs_per_year = 3 articles / 3 years = 1.0
        assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);

        // avg_growth_rate: 0% (2020→2021) and 0% (2021→2022) = avg 0%
        assert!((kpis.avg_growth_rate.unwrap() - 0.0).abs() < 0.01);
    }

    #[test]
    fn kpi_year_null_value() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", None, Some(3), "Smith J");
        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.year_from, None);
        assert_eq!(kpis.year_to, None);
        assert_eq!(kpis.pubs_per_year, None);
        assert!(kpis.pubs_by_year.is_empty());
        assert_eq!(kpis.included_count, 1);
    }

    #[test]
    fn kpi_year_null_filtered() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "A");
        insert_kpi_article(&conn, "a2", "included", None, Some(1), "B"); // NULL year
        insert_kpi_article(&conn, "a4", "included", Some(2022), Some(1), "D");

        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.included_count, 3);
        assert_eq!(kpis.year_from, Some(2020));
        assert_eq!(kpis.year_to, Some(2022));

        // pubs_by_year only has 2020 and 2022 (NULL excluded)
        assert_eq!(kpis.pubs_by_year.len(), 2);
        assert_eq!(kpis.pubs_by_year[0], YearCount { year: 2020, count: 1 });
        assert_eq!(kpis.pubs_by_year[1], YearCount { year: 2022, count: 1 });

        // pubs_per_year = 2 articles with year / 2 distinct years = 1.0
        assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn kpi_pubs_per_year_precision() {
        let conn = test_db();
        // 3 articles across 3 years → pubs_per_year = 1.0
        insert_kpi_article(&conn, "a1", "included", Some(2018), Some(1), "A");
        insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "B");
        insert_kpi_article(&conn, "a3", "included", Some(2022), Some(1), "C");
        let kpis = get_biblio_kpis(&conn).unwrap();
        assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);

        // With 5 articles across 2 years → pubs_per_year = 2.5
        insert_kpi_article(&conn, "a4", "included", Some(2018), Some(1), "D");
        insert_kpi_article(&conn, "a5", "included", Some(2020), Some(1), "E");
        let kpis2 = get_biblio_kpis(&conn).unwrap();
        assert!((kpis2.pubs_per_year.unwrap() - (5.0 / 3.0)).abs() < 0.01);
    }

    #[test]
    fn kpi_citations_with_nulls() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2020), Some(10), "A");
        insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B"); // NULL citations
        insert_kpi_article(&conn, "a3", "included", Some(2020), Some(5), "C");

        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.total_citations, 15); // 10 + 5, NULL excluded
    }

    #[test]
    fn kpi_avg_growth_rate_positive() {
        let conn = test_db();
        // 5 articles in 2021, 10 in 2022 → one pair: +100%
        for i in 0..5 {
            insert_kpi_article(&conn, &format!("old{i}"), "included", Some(2021), Some(1), "A");
        }
        for i in 0..10 {
            insert_kpi_article(&conn, &format!("new{i}"), "included", Some(2022), Some(1), "B");
        }

        let kpis = get_biblio_kpis(&conn).unwrap();
        let rate = kpis.avg_growth_rate.unwrap();
        assert!((rate - 100.0).abs() < 0.1, "expected +100%, got {rate}");
    }

    #[test]
    fn kpi_avg_growth_rate_negative() {
        let conn = test_db();
        // 10 articles in 2021, 5 in 2022 → one pair: -50%
        for i in 0..10 {
            insert_kpi_article(&conn, &format!("old{i}"), "included", Some(2021), Some(1), "A");
        }
        for i in 0..5 {
            insert_kpi_article(&conn, &format!("new{i}"), "included", Some(2022), Some(1), "B");
        }

        let kpis = get_biblio_kpis(&conn).unwrap();
        let rate = kpis.avg_growth_rate.unwrap();
        assert!((rate - (-50.0)).abs() < 0.1, "expected -50%, got {rate}");
    }

    #[test]
    fn kpi_avg_growth_rate_multi_year() {
        let conn = test_db();
        // 4 in 2019, 8 in 2020, 4 in 2021, 12 in 2022
        for i in 0..4 {
            insert_kpi_article(&conn, &format!("a19_{i}"), "included", Some(2019), Some(1), "A");
        }
        for i in 0..8 {
            insert_kpi_article(&conn, &format!("a20_{i}"), "included", Some(2020), Some(1), "B");
        }
        for i in 0..4 {
            insert_kpi_article(&conn, &format!("a21_{i}"), "included", Some(2021), Some(1), "C");
        }
        for i in 0..12 {
            insert_kpi_article(&conn, &format!("a22_{i}"), "included", Some(2022), Some(1), "D");
        }

        let kpis = get_biblio_kpis(&conn).unwrap();
        // Growth rates: 2019→2020 = +100%, 2020→2021 = -50%, 2021→2022 = +200%
        // Avg = (100 + (-50) + 200) / 3 = 250 / 3 ≈ 83.33
        let rate = kpis.avg_growth_rate.unwrap();
        let expected = (100.0 + (-50.0) + 200.0) / 3.0;
        assert!((rate - expected).abs() < 0.1, "expected {expected}%, got {rate}");

        // pubs_per_year = 28 / 4 = 7.0
        assert!((kpis.pubs_per_year.unwrap() - 7.0).abs() < 0.01);
    }

    #[test]
    fn kpi_avg_growth_rate_single_year_is_none() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2022), Some(1), "A");
        insert_kpi_article(&conn, "a2", "included", Some(2022), Some(1), "B");

        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.avg_growth_rate, None);
    }

    #[test]
    fn kpi_unique_authors_zero_without_normalization() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J; Doe A");
        insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "Smith J");

        // Without normalization, biblio_authors is empty → unique_authors = 0
        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.unique_authors, 0);
    }

    #[test]
    fn kpi_unique_authors_from_biblio_table() {
        let conn = test_db();
        insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J");
        upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        upsert_author(&conn, "doe a", "Doe, A.").unwrap();

        let kpis = get_biblio_kpis(&conn).unwrap();
        assert_eq!(kpis.unique_authors, 2);
    }

    // ── Selective Clear Tests ───────────────────────────────────

    #[test]
    fn test_clear_regeneratable_preserves_ai_terms() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        // Insert metadata term and AI term
        save_article_terms(
            &conn,
            "art1",
            &[
                ("keyword".to_string(), TermType::Keyword, TermSource::Metadata),
                ("ai concept".to_string(), TermType::NounPhrase, TermSource::AiExtracted),
            ],
        )
        .unwrap();

        let before = get_biblio_status(&conn).unwrap();
        assert_eq!(before.term_count, 2);

        clear_regeneratable_biblio(&conn).unwrap();

        let after = get_biblio_status(&conn).unwrap();
        assert_eq!(after.term_count, 1, "AI term should be preserved");
        assert_eq!(after.author_count, 0, "Authors should be cleared");
    }

    #[test]
    fn test_clear_regeneratable_preserves_user_terms() {
        let conn = test_db();
        insert_test_article(&conn, "art1");
        save_article_terms(
            &conn,
            "art1",
            &[
                ("user tag".to_string(), TermType::Keyword, TermSource::UserAdded),
                ("metadata kw".to_string(), TermType::Keyword, TermSource::Metadata),
            ],
        )
        .unwrap();

        clear_regeneratable_biblio(&conn).unwrap();

        let terms = get_all_terms(&conn).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].source, TermSource::UserAdded);
    }

    // ── Author Metrics Tests ────────────────────────────────────

    #[test]
    fn test_compute_author_metrics() {
        let conn = test_db();
        // Insert articles with citation counts
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a1', 'T1', 'Abs', 'Smith J', 'included', 2020, 10)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a2', 'T2', 'Abs', 'Smith J', 'included', 2022, 5)",
            [],
        ).unwrap();

        // Create author and links
        let aid = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        link_article_author(&conn, "a1", &aid, 0, Some("Smith J"), None).unwrap();
        link_article_author(&conn, "a2", &aid, 0, Some("Smith J"), None).unwrap();

        compute_author_metrics(&conn).unwrap();

        let author = get_all_authors(&conn).unwrap().into_iter().next().unwrap();
        assert_eq!(author.total_citations, 15, "Total citations should be 10+5=15");
        assert!(author.avg_year.unwrap() > 2020.0, "Avg year should be ~2021");
        assert_eq!(author.estimated_h_index, Some(2), "h-index: 2 papers with >=2 citations");
    }

    #[test]
    fn test_compute_h_index() {
        let conn = test_db();
        // 5 papers with citations [10, 8, 5, 4, 3] → h-index = 4 (4 papers with >=4 citations)
        for i in 0..5 {
            let cites = [10, 8, 5, 4, 3][i];
            conn.execute(
                "INSERT INTO articles (id, title, abstract_text, authors, status, num_cited) VALUES (?1, 'T', 'Abs', 'A', 'included', ?2)",
                rusqlite::params![format!("a{i}"), cites],
            ).unwrap();
        }
        let aid = upsert_author(&conn, "a", "A.").unwrap();
        for i in 0..5 {
            link_article_author(&conn, &format!("a{i}"), &aid, i as i32, None, None).unwrap();
        }

        let h = compute_h_index(&conn, &aid);
        assert_eq!(h, 4, "h-index should be 4 for [10,8,5,4,3]");
    }

    // ── Edge Counting Tests ─────────────────────────────────────

    #[test]
    fn test_build_coauthor_edges_full_and_fractional() {
        let conn = test_db();
        // 1 article with 3 authors → 3 pairs
        conn.execute("INSERT INTO articles (id, title, abstract_text, authors, status) VALUES ('a1', 'T', 'Abs', 'A; B; C', 'included')", []).unwrap();
        let a1 = upsert_author(&conn, "a", "A.").unwrap();
        let a2 = upsert_author(&conn, "b", "B.").unwrap();
        let a3 = upsert_author(&conn, "c", "C.").unwrap();
        link_article_author(&conn, "a1", &a1, 0, None, None).unwrap();
        link_article_author(&conn, "a1", &a2, 1, None, None).unwrap();
        link_article_author(&conn, "a1", &a3, 2, None, None).unwrap();

        let edge_count = build_coauthor_edges(&conn).unwrap();
        assert_eq!(edge_count, 3, "3 authors → 3 edges");

        // Verify fractional data in network metadata
        let meta: String = conn
            .query_row(
                "SELECT params_json FROM biblio_network_meta WHERE network_type = 'co_authorship'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&meta).unwrap();
        let frac_edges = parsed["fractional_edges"].as_array().unwrap();
        // 3 pairs from 3 authors: each pair gets 1/3 ≈ 0.333
        for fe in frac_edges {
            let fw = fe["fractional_weight"].as_f64().unwrap();
            assert!((fw - 0.333).abs() < 0.01, "Fractional weight should be ~0.333, got {fw}");
        }
    }

    // ── Network JSON Output Tests ───────────────────────────────

    #[test]
    fn test_get_coauthor_network_json_includes_metrics() {
        let conn = test_db();
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a1', 'T', 'Abs', 'Smith J', 'included', 2020, 10)",
            [],
        ).unwrap();
        let aid = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
        link_article_author(&conn, "a1", &aid, 0, None, None).unwrap();
        compute_author_metrics(&conn).unwrap();

        let json = get_coauthor_network_json(&conn).unwrap();
        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["citations"], 10);
        assert_eq!(nodes[0]["hIndex"], 1);
        assert_eq!(nodes[0]["avgYear"], 2020.0);
    }
}
