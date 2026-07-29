//! Concept hub pre-seed (Phase 3).
//!
//! Pre-seeds `wiki/concepts/{term-slug}.md` from two complementary sources:
//!
//! 1. **User-curated tags** (top-40 by included-article count). Tags are the
//!    highest-signal source because the user explicitly chose them; many are
//!    multi-word domain concepts (`supply-chain-management`,
//!    `agri-food-digitalization`) that the unigram-only `biblio_terms`
//!    extraction cannot produce. Queried first so tags win on slug collisions
//!    with terms.
//! 2. **`biblio_terms`** (top-25 by frequency). Extracted from article
//!    keywords + titles + abstracts by the bibliometric normalizer. Backfills
//!    concepts the user hasn't tagged.
//!
//! Slug-based merge: if a tag and a term normalize to the same slug, the
//! term's articles + co-occurring concepts are UNIONED into the tag's page
//! (tags are the preferred canonical display name). Reviewed (user-edited)
//! pages are preserved.
//!
//! Each concept page is a hub linking to the articles that contain the
//! term/tag and to co-occurring concepts.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::concept_slug;

/// Number of user-curated tags to surface as concept hubs. Tags are the
/// highest-signal source (user-chosen, often multi-word domain concepts), so
/// they get a larger budget than the `biblio_terms` fallback.
pub const TAG_CONCEPT_LIMIT: usize = 40;

/// Convert a tag name (kebab-case or free text) into a human-readable display
/// name suitable for a wiki page title.
///
/// `"supply-chain-management"` -> `"Supply Chain Management"`,
/// `"agri-food-digitalization"` -> `"Agri Food Digitalization"`,
/// `"RCT"` -> `"Rct"` (intentional - tags are conventionally lowercase
/// kebab-case; mixed-case acronyms are rare and the wiki editor lets the user
/// refine the title).
///
/// Pure function. Used by `fetch_top_tags` to populate `TermRow.raw_term`.
#[must_use]
pub fn tag_to_display_name(tag: &str) -> String {
    tag.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => {
                    let rest: String = chars.collect();
                    let mut out = String::with_capacity(rest.len() + 1);
                    out.push(first.to_ascii_uppercase());
                    out.push_str(&rest);
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

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
///
/// Shared by `preseed_concept_hubs` and `methods::fetch_methods_from_terms`
/// (the abstracts-only fallback path).
pub(super) fn fetch_top_terms(conn: &Connection, limit: usize) -> Result<Vec<TermRow>, AppError> {
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

/// Query the top-N user-curated tags by included-article count, shaped as
/// `TermRow`s so the existing renderer applies unchanged.
///
/// Tags are the highest-signal concept source because the user explicitly
/// chose them. Many are multi-word domain concepts
/// (`supply-chain-management`, `agri-food-digitalization`) that the
/// unigram-only `biblio_terms` extraction cannot produce.
///
/// Each row carries:
/// - `raw_term` = human-readable display name via `tag_to_display_name`.
/// - `normalized_term` = the tag name lowercased (so it dedups against terms
///   that resolve to the same slug).
/// - `frequency` = the included-article count (matches how `fetch_top_terms`
///   weights terms by total frequency).
/// - `article_ids` = the included articles carrying the tag.
/// - `co_terms` = the top co-occurring tag names (tags that share the most
///   articles with this one). Used for the "Related Concepts" links.
///
/// Errors are non-fatal at the call site (the caller proceeds with terms
/// alone when the tag query fails or the tables are absent).
pub(super) fn fetch_top_tags(conn: &Connection, limit: usize) -> Result<Vec<TermRow>, AppError> {
    // Top tags by included-article count.
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(at.article_id) as article_count \
         FROM tags t \
         JOIN article_tags at ON at.tag_id = t.id \
         JOIN articles a ON a.id = at.article_id \
         WHERE a.status = 'included' \
         GROUP BY t.name \
         ORDER BY article_count DESC \
         LIMIT ?1",
    )?;
    let top: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(Result::ok)
        .collect();
    if top.is_empty() {
        return Ok(Vec::new());
    }

    // Build a normalized-name set so co-occurring tags outside the top-N are
    // filtered out (mirrors the `fetch_top_terms` co-term filter).
    let normalized_set: HashSet<String> = top.iter().map(|(name, _)| name.to_lowercase()).collect();

    let mut out = Vec::with_capacity(top.len());
    for (tag_name, frequency) in top {
        let normalized_term = tag_name.to_lowercase();
        let raw_term = tag_to_display_name(&tag_name);

        // Articles carrying this tag.
        let article_ids: Vec<String> = conn
            .prepare(
                "SELECT a.id FROM articles a \
                 JOIN article_tags at ON at.article_id = a.id \
                 JOIN tags t ON t.id = at.tag_id \
                 WHERE t.name = ?1 AND a.status = 'included' \
                 ORDER BY a.publication_year DESC NULLS LAST",
            )?
            .query_map(rusqlite::params![tag_name], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();

        // Co-occurring tags: tags that share the most articles with this one.
        let co_terms: Vec<String> = conn
            .prepare(
                "SELECT t2.name as co_tag, COUNT(*) as shared \
                 FROM article_tags at1 \
                 JOIN article_tags at2 \
                   ON at1.article_id = at2.article_id AND at2.tag_id != at1.tag_id \
                 JOIN tags t2 ON t2.id = at2.tag_id \
                 JOIN tags t1 ON t1.id = at1.tag_id \
                 WHERE t1.name = ?1 \
                 GROUP BY t2.name \
                 ORDER BY shared DESC \
                 LIMIT 5",
            )?
            .query_map(rusqlite::params![tag_name], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .map(|name| name.to_lowercase())
            .filter(|t| normalized_set.contains(t.as_str()))
            .collect();

        out.push(TermRow { raw_term, normalized_term, frequency, article_ids, co_terms });
    }
    Ok(out)
}

/// Pre-seed `wiki/concepts/{slug}.md` from user-curated tags (top-40 by
/// included-article count) PLUS `biblio_terms` (top-N by frequency). Tags are
/// queried first so they win on slug collisions (a tag and a term that
/// normalize to the same slug produce a single page using the tag's display
/// name, with the articles unioned). Reviewed (user-edited) pages are
/// preserved.
///
/// Each concept page is a hub linking to the articles that carry the tag/term
/// (`[[{article_id}]]` synthesis links) and to co-occurring concepts.
///
/// Returns the count of pages written.
pub fn preseed_concept_hubs(
    conn: &Connection,
    root: &Path,
    limit: usize,
) -> Result<usize, AppError> {
    let concepts_dir = root.join("wiki").join("concepts");
    std::fs::create_dir_all(&concepts_dir)?;
    // Tags first: highest-signal, user-curated, often multi-word.
    let tags = fetch_top_tags(conn, TAG_CONCEPT_LIMIT).unwrap_or_default();
    let terms = fetch_top_terms(conn, limit)?;
    // Merge by slug: tags first (canonical), then terms. When a term collides
    // with an existing slug, its articles + co_terms are UNIONED into the
    // canonical row (lossless) instead of being dropped. This keeps the tag's
    // display name while still capturing every article that references the
    // concept, regardless of whether it was tagged or extracted from text.
    let mut by_slug: std::collections::HashMap<String, TermRow> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for term in tags.into_iter().chain(terms) {
        if term.article_ids.is_empty() {
            continue;
        }
        let slug = concept_slug(&term.normalized_term);
        match by_slug.get_mut(&slug) {
            Some(existing) => {
                // Union articles (deduped, preserving first-seen order).
                for id in &term.article_ids {
                    if !existing.article_ids.contains(id) {
                        existing.article_ids.push(id.clone());
                    }
                }
                // Union co_terms (deduped).
                for co in &term.co_terms {
                    if !existing.co_terms.contains(co) {
                        existing.co_terms.push(co.clone());
                    }
                }
                // Frequency is informational; sum it.
                existing.frequency += term.frequency;
            }
            None => {
                order.push(slug.clone());
                by_slug.insert(slug, term);
            }
        }
    }
    let mut written = 0;
    for slug in &order {
        let path = concepts_dir.join(format!("{}.md", slug));
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let Some(term) = by_slug.get(slug) else {
            // Defensive: `slug` was inserted into `by_slug` in the loop above,
            // so this branch is unreachable. Skip rather than panic.
            continue;
        };
        let (fm, body) = render_concept_hub(term, slug);
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
