//! Author pre-seeding (Phase 1, multi-batch + single-batch).
//!
//! Builds a canonical author manifest from `biblio_authors` and pre-seeds rich
//! author hub pages (metrics, publications, research areas, collaborators) so
//! independent parallel ingest batches link to the same author slugs instead of
//! inventing their own.

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::author_slug;

/// A manifest of canonical author slugs, derived deterministically from the
/// `biblio_authors` table (or the raw sources when that table is empty).
///
/// Injected into every batch prompt so independent parallel batches link to the
/// same canonical author slugs instead of inventing their own. This eliminates
/// the worst class of cross-batch duplication: fragmented author pages
/// (`jane-doe` vs `j-doe`).
#[derive(Debug, Clone, Default)]
pub struct AuthorManifest {
    /// One entry per canonical author.
    pub entries: Vec<AuthorManifestEntry>,
}

/// One row in the manifest: a raw name variant and its canonical slug plus
/// the rich bibliometric data that makes the pre-seeded author page useful.
#[derive(Debug, Clone, Default)]
pub struct AuthorManifestEntry {
    /// Canonical author page slug (e.g. `author-smith-j`).
    pub slug: String,
    /// Display name for the page title (e.g. "Smith, J").
    pub display_name: String,
    /// Raw name variants that should link to this slug (lowercased for matching).
    pub raw_variants: Vec<String>,
    /// Number of included articles this author appears on.
    pub article_count: i32,
    /// The articles this author appears on (for the Publications section + source_articles frontmatter).
    pub articles: Vec<AuthorArticle>,
    /// Deduplicated keywords aggregated across all the author's articles, ranked by frequency.
    pub keywords: Vec<String>,
    /// Co-authors who share at least one article with this author.
    pub coauthors: Vec<CoauthorLink>,
    /// Estimated h-index (from `biblio_authors.estimated_h_index`).
    pub h_index: Option<i32>,
    /// Total citations across all the author's articles.
    pub total_citations: i32,
    /// Number of articles where this author is the first author.
    pub first_author_count: i32,
    /// Publications per year (article_count / year span). `None` when years are missing.
    pub productivity_rate: Option<f64>,
}

/// A publication by an author, rendered in the author page's Publications section.
#[derive(Debug, Clone)]
pub struct AuthorArticle {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub journal: Option<String>,
}

/// A co-author link for the author page's Frequent Collaborators section.
#[derive(Debug, Clone)]
pub struct CoauthorLink {
    /// Canonical wiki slug for the co-author (e.g. `author-doe-a`).
    pub slug: String,
    /// Display name of the co-author.
    pub display_name: String,
    /// Number of shared papers.
    pub shared_papers: i32,
}

impl AuthorManifest {
    /// Render the manifest as a prompt section with a two-part directive:
    ///
    /// 1. **Known authors** (in the manifest) → "link to their pre-seeded page using the
    ///    EXACT slug - do NOT create a duplicate page for them."
    /// 2. **Unknown authors** (mentioned in uploaded documents but NOT in the manifest) →
    ///    "you SHOULD create a new author page" with slug `author-{lastname}-{initial}`,
    ///    linked to the uploaded document.
    ///
    /// This split prevents the blanket "DO NOT create author pages" from blocking
    /// legitimate new-author pages derived from user-uploaded documents (Add Documents).
    ///
    /// Returns an empty string when the manifest is empty (so it can be unconditionally
    /// interpolated). When empty, there are no pre-seeded author pages to protect, so the
    /// LLM is free to create author pages normally.
    pub(super) fn to_prompt_section(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("# Author Pages (Pre-Seeded - LINK, DON'T DUPLICATE)\n\n");
        out.push_str(
            "Author pages for the following authors have already been generated from the \
             project's bibliometric data. When you mention one of these authors, link to \
             their pre-seeded page using the EXACT slug. Do NOT create a new page for them:\n\n",
        );
        for entry in &self.entries {
            let variants = if entry.raw_variants.is_empty() {
                String::new()
            } else {
                format!(" (variants: {})", entry.raw_variants.join(", "))
            };
            out.push_str(&format!(
                "- [[{}]] - {}{} - {} articles\n",
                entry.slug, entry.display_name, variants, entry.article_count
            ));
        }
        out.push_str(
            "\nWhen a source lists an author whose name matches one of the variants above \
             (case-insensitive), use the canonical slug for the link.\n\n",
        );
        out.push_str("## New Authors from Uploaded Documents\n\n");
        out.push_str(
            "If an uploaded document mentions an author who is NOT in the list above (i.e., \
             an author not in the article corpus), you SHOULD create a new author page:\n\
             - Slug: `author-{lastname}-{initial}` (e.g. `author-doe-j` for \"Jane Doe\").\n\
             - type: author, status: draft.\n\
             - Link to the uploaded document via [[document-slug]] or [^art-document-slug].\n\
             - Include any biographical details, affiliations, or research areas mentioned.\n\n",
        );
        out
    }
}

/// Build an `AuthorManifest` from the `biblio_authors` table.
///
/// The caller is expected to run `normalize_authors_from_articles(conn)` first
/// so the table is populated; this function treats the DB as the single source
/// of truth. Returns an empty manifest when there are no authors (e.g. a
/// corpus with no author metadata), which the caller treats as "no manifest".
pub fn build_author_manifest_from_db(conn: &Connection) -> Result<AuthorManifest, AppError> {
    let authors = crate::db::biblio_repo::get_all_authors(conn)?;
    if authors.is_empty() {
        return Ok(AuthorManifest::default());
    }
    let mut entries = Vec::with_capacity(authors.len());
    for author in authors {
        let slug = author_slug(&author.normalized_name);
        let raw_variants = collect_raw_variants(conn, &author.id)?;
        let articles = collect_author_articles(conn, &author.id)?;
        let keywords = collect_author_keywords(conn, &author.id)?;
        let coauthors = collect_coauthors(conn, &author.id);
        let productivity_rate = compute_productivity_rate(&articles);
        entries.push(AuthorManifestEntry {
            slug,
            display_name: author.display_name,
            raw_variants,
            article_count: author.article_count,
            articles,
            keywords,
            coauthors,
            h_index: author.estimated_h_index,
            total_citations: author.total_citations,
            first_author_count: author.first_author_count,
            productivity_rate,
        });
    }
    Ok(AuthorManifest { entries })
}

/// Collect the articles an author appears on, ordered by year (most recent first).
fn collect_author_articles(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<AuthorArticle>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.title, a.publication_year, a.journal \
         FROM articles a \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 \
         ORDER BY a.publication_year DESC NULLS LAST, a.title",
    )?;
    let articles: Vec<AuthorArticle> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(AuthorArticle {
                id: row.get(0)?,
                title: row.get(1)?,
                year: row.get(2)?,
                journal: row.get(3)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(articles)
}

/// Collect deduplicated keywords for an author, aggregated across all their
/// articles, ranked by total frequency. Each term appears once (deduplicated by
/// `normalized_term`), using its `raw_term` for display.
fn collect_author_keywords(conn: &Connection, author_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT bt.raw_term, SUM(bat.frequency) as total_freq \
         FROM biblio_article_terms bat \
         JOIN biblio_terms bt ON bt.id = bat.term_id \
         WHERE bat.article_id IN ( \
             SELECT article_id FROM biblio_article_authors WHERE author_id = ?1 \
         ) \
         GROUP BY bt.normalized_term \
         ORDER BY total_freq DESC \
         LIMIT 15",
    )?;
    let keywords: Vec<String> = stmt
        .query_map(rusqlite::params![author_id], |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter(|s| !s.is_empty())
        .collect();
    Ok(keywords)
}

/// Collect co-authors who share at least one article with the given author,
/// ranked by shared-paper count. Each co-author's canonical slug is derived
/// from their normalized name in `biblio_authors`.
fn collect_coauthors(conn: &Connection, author_id: &str) -> Vec<CoauthorLink> {
    let sql = "SELECT ba2.author_id, ba2.normalized_name, ba2.display_name, COUNT(*) as shared \
         FROM biblio_article_authors ba1 \
         JOIN biblio_article_authors ba2 ON ba1.article_id = ba2.article_id \
         JOIN biblio_authors ba2_meta ON ba2.author_id = ba2_meta.id \
         WHERE ba1.author_id = ?1 AND ba2.author_id != ?1 \
         GROUP BY ba2.author_id \
         ORDER BY shared DESC \
         LIMIT 10";
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![author_id, author_id], |row| {
        let normalized_name: String = row.get(1)?;
        let display_name: String = row.get(2)?;
        let shared: i32 = row.get(3)?;
        Ok(CoauthorLink {
            slug: author_slug(&normalized_name),
            display_name,
            shared_papers: shared,
        })
    })
    .map(|rows| rows.filter_map(Result::ok).collect())
    .unwrap_or_default()
}

/// Compute publications per year (article_count / year span) from the article
/// list. Returns `None` when there are no years to compute a span from.
fn compute_productivity_rate(articles: &[AuthorArticle]) -> Option<f64> {
    let years: Vec<i32> = articles.iter().filter_map(|a| a.year).collect();
    if years.is_empty() {
        return None;
    }
    let min_year = *years.iter().min()?;
    let max_year = *years.iter().max()?;
    let span = (max_year - min_year + 1).max(1) as f64;
    Some((articles.len() as f64 / span * 10.0).round() / 10.0)
}

/// Collect the distinct raw name variants linked to a normalized author ID.
fn collect_raw_variants(conn: &Connection, author_id: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT raw_name FROM biblio_article_authors \
         WHERE author_id = ?1 AND raw_name IS NOT NULL",
    )?;
    let variants: Vec<String> = stmt
        .query_map(rusqlite::params![author_id], |row| row.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter(|s| !s.is_empty())
        .collect();
    Ok(variants)
}

/// Resolve the effective manifest from the `biblio_authors` table.
///
/// The caller (`build_batches_with_manifest` in `commands/wiki_cmd.rs`) runs
/// `normalize_authors_from_articles` first to ensure the table is populated,
/// so this function can rely on the DB as the single source of truth. Returns
/// an empty manifest when there are no authors (e.g. a corpus with no author
/// metadata at all), which the caller treats as "no manifest".
pub fn build_author_manifest(conn: &Connection) -> Result<AuthorManifest, AppError> {
    build_author_manifest_from_db(conn)
}

/// Pre-seed the `wiki/authors/` directory with rich author pages built from
/// the manifest. Each page includes a metrics line, a publications list with
/// `[^art-id]` source references, deduplicated research-area keywords, and
/// co-author `[[wikilinks]]`. Skips authors whose pages already exist with
/// `status: reviewed` (user-edited). Returns the count of pages written.
pub fn preseed_authors(root: &Path, manifest: &AuthorManifest) -> Result<usize, AppError> {
    let authors_dir = root.join("wiki").join("authors");
    std::fs::create_dir_all(&authors_dir)?;
    let mut written = 0;
    for entry in &manifest.entries {
        let path = authors_dir.join(format!("{}.md", entry.slug));
        // Respect reviewed pages (user has edited them).
        if let Ok((existing_fm, _)) = frontmatter::read_file(&path) {
            if existing_fm.get("status") == Some("reviewed") {
                continue;
            }
        }
        let (fm, body) = render_author_page(entry);
        frontmatter::write_file(&path, &fm, &body)?;
        written += 1;
    }
    Ok(written)
}

/// Render the frontmatter + body for a single author page from the manifest
/// entry's rich data. Pure function (no I/O) so it is trivially testable.
pub fn render_author_page(entry: &AuthorManifestEntry) -> (Frontmatter, String) {
    // Frontmatter.
    let mut fm = Frontmatter::default();
    fm.set("id", &entry.slug);
    fm.set("title", &entry.display_name);
    fm.set("type", "author");
    fm.set("slug", &entry.slug);
    fm.set(
        "summary",
        &format!(
            "{}, {} articles, h-index {}.",
            entry.display_name,
            entry.article_count,
            entry.h_index.unwrap_or(0)
        ),
    );
    fm.set("status", "draft");
    // source_articles: real article IDs from the DB.
    let source_ids: Vec<String> = entry.articles.iter().map(|a| format!("\"{}\"", a.id)).collect();
    fm.set("source_articles", &format!("[{}]", source_ids.join(", ")));
    // tags: deduplicated keywords (FTS5 + graph benefit).
    let keyword_tags: Vec<String> = entry.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
    fm.set("tags", &format!("[{}]", keyword_tags.join(", ")));
    // links: co-author slugs.
    let coauthor_links: Vec<String> =
        entry.coauthors.iter().map(|c| format!("\"[[{}]]\"", c.slug)).collect();
    fm.set("links", &format!("[{}]", coauthor_links.join(", ")));
    fm.set("content_source", "metadata");

    // Body.
    // NOTE: do NOT emit `# {title}` as the first body line. The page title
    // lives in frontmatter and is rendered separately by the wiki viewer's
    // header (`<h1>{{ page.title }}</h1>`); repeating it in the body would
    // show the title twice on the rendered page.
    let mut body = String::new();

    // Metrics line (only include metrics that have meaningful values).
    let mut stats: Vec<String> = Vec::new();
    if let Some(h) = entry.h_index {
        if h > 0 {
            stats.push(format!("h-index: {}", h));
        }
    }
    if entry.total_citations > 0 {
        stats.push(format!("Total citations: {}", entry.total_citations));
    }
    if entry.first_author_count > 0 {
        stats.push(format!("First author on {} papers", entry.first_author_count));
    }
    if let Some(rate) = entry.productivity_rate {
        stats.push(format!("~{} papers/year", rate));
    }
    if !stats.is_empty() {
        body.push_str(&format!("{}\n\n", stats.join(" | ")));
    }

    // Publications section.
    // Each entry links to its source article via a `[^art-{uuid}]` reference.
    // The `art-` prefix is required by the wiki Markdown renderer
    // (`src/utils/wiki-markdown.ts` step 1) to convert the ref into a
    // clickable green `.art-ref` chip that opens the article detail panel.
    // No footnote definition block is emitted: the renderer resolves the ref
    // from the in-memory `sources` map, and emitting `/raw/...` definition
    // lines caused duplicate/triplicate clutter in the rendered output.
    body.push_str("## Publications\n\n");
    for article in &entry.articles {
        let year_str = article.year.map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
        let journal_str = article.journal.as_deref().unwrap_or("");
        let meta = if journal_str.is_empty() {
            format!("({})", year_str)
        } else {
            format!("({}, {})", year_str, journal_str)
        };
        body.push_str(&format!("- \"{}\" {} [^art-{}]\n", article.title, meta, article.id));
    }

    // Research Areas section (deduplicated keywords).
    if !entry.keywords.is_empty() {
        body.push_str("\n## Research Areas\n\n");
        body.push_str(&entry.keywords.join(", "));
        body.push('\n');
    }

    // Frequent Collaborators section.
    if !entry.coauthors.is_empty() {
        body.push_str("\n## Frequent Collaborators\n\n");
        for co in &entry.coauthors {
            body.push_str(&format!(
                "- [[{}]] - {} ({} shared)\n",
                co.slug, co.display_name, co.shared_papers
            ));
        }
    }

    (fm, body)
}
