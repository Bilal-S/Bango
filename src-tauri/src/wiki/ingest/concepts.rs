//! Concept hub pre-seed (Phase 3).
//!
//! Pre-seeds `wiki/concepts/{term-slug}.md` for the top-N terms by frequency
//! from `biblio_terms`. Each concept page is a hub linking to the articles
//! that contain the term and to co-occurring concepts.

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::concept_slug;

/// A term row with its frequency + article IDs, for concept hub pre-seeding.
pub struct TermRow {
    pub raw_term: String,
    pub normalized_term: String,
    pub frequency: i64,
    pub article_ids: Vec<String>,
    pub co_terms: Vec<String>,
}

/// Query the top-N terms by total frequency across included articles, with the
/// list of articles each appears in + the top co-occurring terms.
fn fetch_top_terms(conn: &Connection, limit: usize) -> Result<Vec<TermRow>, AppError> {
    // Top terms by total frequency.
    let mut stmt = conn.prepare(
        "SELECT bt.id, bt.raw_term, bt.normalized_term, SUM(bat.frequency) as total_freq \
         FROM biblio_terms bt \
         JOIN biblio_article_terms bat ON bat.term_id = bt.id \
         JOIN articles a ON a.id = bat.article_id \
         WHERE a.status = 'included' \
         GROUP BY bt.normalized_term \
         ORDER BY total_freq DESC \
         LIMIT ?1",
    )?;
    let top: Vec<(String, String, String, i64)> = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .filter_map(Result::ok)
        .collect();
    if top.is_empty() {
        return Ok(Vec::new());
    }

    // Clone the normalized terms so the set owns them (avoids borrowing `top`,
    // which we consume by-value in the for loop below).
    let normalized_set: std::collections::HashSet<String> =
        top.iter().map(|(_, _, n, _)| n.clone()).collect();

    let mut out = Vec::with_capacity(top.len());
    for (term_id, raw_term, normalized_term, frequency) in top {
        // Articles containing this term.
        let article_ids: Vec<String> = conn
            .prepare(
                "SELECT a.id FROM articles a \
                 JOIN biblio_article_terms bat ON bat.article_id = a.id \
                 WHERE bat.term_id = ?1 AND a.status = 'included' \
                 ORDER BY a.publication_year DESC NULLS LAST",
            )?
            .query_map(rusqlite::params![term_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();

        // Co-occurring terms: terms that share the most articles with this one.
        let co_terms: Vec<String> = conn
            .prepare(
                "SELECT bt2.normalized_term as co_term, COUNT(*) as shared \
                 FROM biblio_article_terms bat1 \
                 JOIN biblio_article_terms bat2 \
                   ON bat1.article_id = bat2.article_id AND bat2.term_id != bat1.term_id \
                 JOIN biblio_terms bt2 ON bt2.id = bat2.term_id \
                 WHERE bat1.term_id = ?1 \
                 GROUP BY bt2.normalized_term \
                 ORDER BY shared DESC \
                 LIMIT 5",
            )?
            .query_map(rusqlite::params![term_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .filter(|t| normalized_set.contains(t.as_str()))
            .collect();

        out.push(TermRow { raw_term, normalized_term, frequency, article_ids, co_terms });
    }
    Ok(out)
}

/// Pre-seed `wiki/concepts/{term-slug}.md` for the top-N terms by frequency.
/// Each concept page is a hub linking to the articles that contain the term
/// (`[[{article_id}]]` synthesis links) and to co-occurring concepts.
///
/// Reviewed (user-edited) concept pages are preserved. Terms with no articles
/// are skipped (defensive - the query already filters by included articles).
///
/// Returns the count of pages written.
pub fn preseed_concept_hubs(
    conn: &Connection,
    root: &Path,
    limit: usize,
) -> Result<usize, AppError> {
    let concepts_dir = root.join("wiki").join("concepts");
    std::fs::create_dir_all(&concepts_dir)?;
    let terms = fetch_top_terms(conn, limit)?;
    let mut written = 0;
    for term in terms {
        if term.article_ids.is_empty() {
            continue;
        }
        let slug = concept_slug(&term.normalized_term);
        let path = concepts_dir.join(format!("{}.md", slug));
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let (fm, body) = render_concept_hub(&term, &slug);
        frontmatter::write_file(&path, &fm, &body)?;
        written += 1;
    }
    Ok(written)
}

/// Render the frontmatter + body for a concept hub page. Pure function.
fn render_concept_hub(term: &TermRow, slug: &str) -> (Frontmatter, String) {
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", &term.raw_term);
    fm.set("type", "concept");
    fm.set("slug", slug);
    fm.set("summary", &format!("{} articles mention {}.", term.article_ids.len(), term.raw_term));
    fm.set("status", "draft");
    let source_ids: Vec<String> = term.article_ids.iter().map(|id| format!("\"{}\"", id)).collect();
    fm.set("source_articles", &format!("[{}]", source_ids.join(", ")));
    fm.set("content_source", "metadata");
    // tags: co-occurring term slugs (concept-to-concept links).
    let co_tags: Vec<String> =
        term.co_terms.iter().map(|t| format!("\"{}\"", concept_slug(t))).collect();
    fm.set("tags", &format!("[{}]", co_tags.join(", ")));
    // links: co-occurring concepts as [[slug]].
    let co_links: Vec<String> =
        term.co_terms.iter().map(|t| format!("\"[[{}]]\"", concept_slug(t))).collect();
    fm.set("links", &format!("[{}]", co_links.join(", ")));

    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", term.raw_term));
    body.push_str(&format!(
        "Found in {} included articles (total frequency: {}).\n",
        term.article_ids.len(),
        term.frequency
    ));
    body.push_str("\n## Relevant Studies\n\n");
    for id in &term.article_ids {
        body.push_str(&format!("- [[{}]]\n", id));
    }
    if !term.co_terms.is_empty() {
        body.push_str("\n## Related Concepts\n\n");
        let links: Vec<String> =
            term.co_terms.iter().map(|t| format!("[[{}]]", concept_slug(t))).collect();
        body.push_str(&links.join(", "));
        body.push('\n');
    }
    (fm, body)
}
