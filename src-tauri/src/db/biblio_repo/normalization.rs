use rusqlite::Connection;

use super::authors::{link_article_author, upsert_author};
use super::institutions::{insert_author_affiliation, upsert_institution};
use super::terms::save_article_terms;
use crate::error::AppError;
use crate::models::biblio::{BiblioStatus, TermSource, TermType};

/// Extract and normalize all authors from the articles table into biblio tables.
/// Returns the number of unique authors created.
pub fn normalize_authors_from_articles(conn: &Connection) -> Result<usize, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, authors, affiliation, author_address, custom_field3 FROM articles \
         WHERE status = 'included' AND authors IS NOT NULL AND authors != ''",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    for (article_id, authors_str, affiliation_opt, author_address_opt, custom_field3_opt) in &rows {
        let parsed = crate::biblio::normalizer::parse_authors(authors_str);
        let author_count = parsed.len();

        let mut resolved_affs = None;

        // 1. Check C3 field (custom_field3)
        if let Some(c3_str) = custom_field3_opt.as_ref() {
            let c3_list: Vec<String> =
                c3_str.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            if c3_list.len() == author_count {
                resolved_affs = Some(c3_list);
            }
        }

        // 2. Fallback to AD field (author_address)
        if resolved_affs.is_none() {
            if let Some(ad_str) = author_address_opt.as_ref() {
                let ad_list: Vec<String> = ad_str
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if ad_list.len() == author_count {
                    resolved_affs = Some(ad_list);
                }
            }
        }

        // 3. Fallback to article affiliation
        let affs = resolved_affs.unwrap_or_else(|| {
            if let Some(aff_str) = affiliation_opt.as_ref() {
                aff_str.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                Vec::new()
            }
        });

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
         WHERE status = 'included'",
    )?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    for (article_id, keywords, title, abstract_text) in &rows {
        let mut terms: Vec<(String, TermType, TermSource)> = Vec::new();

        // Extract keywords from metadata.
        // `articles.keywords` is stored as a JSON array (e.g.
        // `["Allura Red", "tartrazine"]`), so we must parse JSON before
        // falling back to `;`/`,` delimiter splitting. See `split_keywords`.
        if let Some(kw) = keywords {
            for k in crate::biblio::normalizer::split_keywords(kw) {
                if !k.is_empty() {
                    terms.push((k, TermType::Keyword, TermSource::Metadata));
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
    let extractor = crate::biblio::affiliation_extractor::AffiliationExtractor::new().ok();

    let mut stmt = conn.prepare(
        "SELECT id, raw_affiliation FROM biblio_article_authors \
         WHERE raw_affiliation IS NOT NULL AND raw_affiliation != ''",
    )?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;

    let mut institutions_created = 0usize;
    let mut links_created = 0usize;

    for (article_author_id, raw_aff) in &rows {
        let parsed = crate::biblio::normalizer::parse_affiliation_with_extractor(
            raw_aff,
            extractor.as_ref(),
        );
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
