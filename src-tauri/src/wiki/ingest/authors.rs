//! Author pre-seeding (Phase 1). Builds canonical author manifest from `biblio_authors`,
//! pre-seeds rich author hub pages (metrics, main themes, publications, most-cited,
//! key references, research areas, collaborators) so independent parallel ingest
//! batches link to the same author slugs.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::concepts::{fetch_top_tags, fetch_top_terms, TAG_CONCEPT_LIMIT};
use super::slugs::{author_slug, concept_slug};

/// Term-hub budget mirroring the `preseed_concept_hubs` call in
/// `commands/wiki_cmd/ingest.rs` (phase 3), so the Main Themes hub map matches
/// the concept pages actually seeded.
const CONCEPT_TERM_LIMIT: usize = 25;

/// Generic academic filler terms filtered from Research Areas. Live-data
/// finding: raw keyword dumps included "upon", "significant", "years".
const KEYWORD_BLOCKLIST: &[&str] = &[
    "upon",
    "years",
    "significant",
    "significantly",
    "studies",
    "study",
    "associated",
    "association",
    "between",
    "using",
    "used",
    "results",
    "methods",
    "based",
    "data",
    "research",
    "article",
    "paper",
    "papers",
    "effect",
    "effects",
    "total",
    "current",
    "present",
    "recent",
    "among",
    "common",
    "general",
    "high",
    "higher",
    "low",
    "lower",
];

/// Cited-works section size and keyword cap.
const MOST_CITED_LIMIT: usize = 5;
const KEYWORD_LIMIT: usize = 10;

/// Canonical author manifest from `biblio_authors`. Injected into every batch prompt so
/// independent parallel batches link to the same author slugs instead of inventing their own.
/// Eliminates the worst cross-batch duplication: fragmented author pages.
#[derive(Debug, Clone, Default)]
pub struct AuthorManifest {
    /// One entry per canonical author.
    pub entries: Vec<AuthorManifestEntry>,
}

/// One manifest row: raw name variant + canonical slug + rich bibliometric data.
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
    /// The external works this author's papers reference most (top 5).
    pub references: Vec<AuthorReference>,
    /// Concept hub pages this author's work connects to (top 5 wikilinks).
    pub main_themes: Vec<MainThemeLink>,
}

/// A publication by an author, rendered in the Publications section of their page.
#[derive(Debug, Clone)]
pub struct AuthorArticle {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub journal: Option<String>,
    /// Times this article is cited (`articles.num_cited`); powers Most Cited.
    pub citation_count: Option<i32>,
}

/// An external reference paper the author's work relies on (Key References).
#[derive(Debug, Clone)]
pub struct AuthorReference {
    pub title: String,
    pub year: Option<i32>,
    /// How many of the author's articles reference it.
    pub times_used: i64,
}

/// A Main Themes link: an existing concept hub page (slug + display name).
#[derive(Debug, Clone)]
pub struct MainThemeLink {
    pub slug: String,
    pub display: String,
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
    /// Render manifest as prompt section. Known authors → "link, DON'T duplicate."
    /// Unknown authors (from uploaded docs) → "create new author page" with `author-lastname-initial`.
    /// Empty string when manifest is empty (LLM creates authors normally).
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

/// Build `AuthorManifest` from `biblio_authors` table. Caller runs normalization first.
/// Empty when no authors (corpus lacks author metadata).
pub fn build_author_manifest_from_db(conn: &Connection) -> Result<AuthorManifest, AppError> {
    let authors = crate::db::biblio_repo::get_all_authors(conn)?;
    if authors.is_empty() {
        return Ok(AuthorManifest::default());
    }
    /* Concept hub source set (mirrors preseed_concept_hubs) so Main Themes
    links always resolve to concept pages seeded later in the same run. */
    let concept_hubs = build_concept_hub_map(conn)?;
    let mut entries = Vec::with_capacity(authors.len());
    for author in authors {
        let slug = author_slug(&author.normalized_name);
        let raw_variants = collect_raw_variants(conn, &author.id)?;
        let articles = collect_author_articles(conn, &author.id)?;
        let keywords = collect_author_keywords(conn, &author.id)?;
        let coauthors = collect_coauthors(conn, &author.id)?;
        let references = collect_author_references(conn, &author.id)?;
        let main_themes = collect_author_main_themes(conn, &author.id, &concept_hubs)?;
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
            references,
            main_themes,
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
        "SELECT a.id, a.title, a.publication_year, a.journal, a.num_cited \
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
                citation_count: row.get(4)?,
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
    // Curate: drop blocklisted filler + tiny terms, cap at KEYWORD_LIMIT.
    let mut curated = Vec::with_capacity(KEYWORD_LIMIT);
    for keyword in keywords {
        if curated.len() >= KEYWORD_LIMIT {
            break;
        }
        let lower = keyword.to_lowercase();
        if lower.len() < 3 || KEYWORD_BLOCKLIST.contains(&lower.as_str()) {
            continue;
        }
        curated.push(keyword);
    }
    Ok(curated)
}

/// Collect the external reference papers the author's articles rely on most
/// (top 5 by usage count; metadata only - copyright-safe).
fn collect_author_references(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<AuthorReference>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rp.title, rp.publication_year, COUNT(*) as times_used \
         FROM article_reference_links arl \
         JOIN articles a ON a.id = arl.parent_article_id \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         JOIN reference_papers rp ON rp.id = arl.reference_paper_id \
         WHERE baa.author_id = ?1 AND arl.type = 1 \
         GROUP BY rp.id \
         ORDER BY times_used DESC, rp.title \
         LIMIT 5",
    )?;
    let references: Vec<AuthorReference> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok(AuthorReference { title: row.get(0)?, year: row.get(1)?, times_used: row.get(2)? })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(references)
}

/// Concept hub source set mirroring `preseed_concept_hubs` (tags first - they
/// win collisions - then top terms), keyed by normalized term. Main Themes
/// links resolve only against this set, so they always point at seeded pages.
fn build_concept_hub_map(conn: &Connection) -> Result<HashMap<String, (String, String)>, AppError> {
    let tags = fetch_top_tags(conn, TAG_CONCEPT_LIMIT).unwrap_or_default();
    let terms = fetch_top_terms(conn, CONCEPT_TERM_LIMIT)?;
    let mut map: HashMap<String, (String, String)> = HashMap::new();
    for term in tags.into_iter().chain(terms) {
        if term.article_ids.is_empty() {
            continue;
        }
        map.entry(term.normalized_term.clone())
            .or_insert((concept_slug(&term.normalized_term), term.raw_term));
    }
    Ok(map)
}

/// Collect up to 5 Main Themes for an author: their articles' user-curated
/// tags plus their ranked extracted terms, matched against the concept hub
/// map. Tags carry a curation bonus so they outrank extracted terms.
fn collect_author_main_themes(
    conn: &Connection,
    author_id: &str,
    hub_map: &HashMap<String, (String, String)>,
) -> Result<Vec<MainThemeLink>, AppError> {
    // Author's ranked terms (normalized) with frequencies.
    let mut stmt = conn.prepare(
        "SELECT bt.normalized_term, SUM(bat.frequency) as freq \
         FROM biblio_article_terms bat \
         JOIN biblio_terms bt ON bt.id = bat.term_id \
         WHERE bat.article_id IN ( \
             SELECT article_id FROM biblio_article_authors WHERE author_id = ?1 \
         ) \
         GROUP BY bt.normalized_term \
         ORDER BY freq DESC \
         LIMIT 25",
    )?;
    let term_freqs: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(Result::ok)
        .collect();

    // Author's articles' tags, boosted so user curation outranks raw terms.
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(*) as cnt \
         FROM article_tags at \
         JOIN tags t ON t.id = at.tag_id \
         JOIN articles a ON a.id = at.article_id \
         JOIN biblio_article_authors baa ON baa.article_id = a.id \
         WHERE baa.author_id = ?1 \
         GROUP BY t.name \
         ORDER BY cnt DESC \
         LIMIT 10",
    )?;
    let mut candidates: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? * 1_000))
        })?
        .filter_map(Result::ok)
        .collect();
    candidates.extend(term_freqs);
    candidates.sort_by_key(|(_, freq)| std::cmp::Reverse(*freq));

    let mut out: Vec<MainThemeLink> = Vec::new();
    let mut seen_slugs: HashSet<String> = HashSet::new();
    for (normalized, _freq) in candidates {
        if out.len() >= 5 {
            break;
        }
        if let Some((slug, display)) = hub_map.get(&normalized.to_lowercase()) {
            if seen_slugs.insert(slug.clone()) {
                out.push(MainThemeLink { slug: slug.clone(), display: display.clone() });
            }
        }
    }
    Ok(out)
}

/// Collect co-authors who share at least one article with the given author,
/// ranked by shared-paper count. Each co-author's canonical slug is derived
/// from their normalized name in `biblio_authors`.
///
/// Historical bug (fixed): this query was doubly broken behind a swallowed
/// error - (1) `params![author_id, author_id]` vs a single distinct `?1`
/// (rusqlite parameter-count error), and (2) name columns selected from
/// `biblio_article_authors` (`ba2`) instead of `biblio_authors` (`ba2_meta`).
/// Both silently emptied the Collaborators section on every author page.
fn collect_coauthors(conn: &Connection, author_id: &str) -> Result<Vec<CoauthorLink>, AppError> {
    let sql = "SELECT ba2.author_id, ba2_meta.normalized_name, ba2_meta.display_name, \
         COUNT(*) as shared \
         FROM biblio_article_authors ba1 \
         JOIN biblio_article_authors ba2 ON ba1.article_id = ba2.article_id \
         JOIN biblio_authors ba2_meta ON ba2.author_id = ba2_meta.id \
         WHERE ba1.author_id = ?1 AND ba2.author_id != ?1 \
         GROUP BY ba2.author_id \
         ORDER BY shared DESC \
         LIMIT 10";
    let mut stmt = conn.prepare(sql)?;
    let coauthors: Vec<CoauthorLink> = stmt
        .query_map(rusqlite::params![author_id], |row| {
            let normalized_name: String = row.get(1)?;
            let display_name: String = row.get(2)?;
            let shared: i32 = row.get(3)?;
            Ok(CoauthorLink {
                slug: author_slug(&normalized_name),
                display_name,
                shared_papers: shared,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(coauthors)
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

/// Pre-seed `wiki/authors/` with rich author pages (metrics, publications, keywords,
/// co-author wikilinks). Skips `status: reviewed` pages. Returns count written.
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

/// Render frontmatter + body for an author page. Pure function (no I/O).
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
    /* Page title lives in frontmatter, rendered by the viewer header.
    Do NOT emit `# {title}` as first body line - would show the title twice. */
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

    // Main Themes: links to seeded concept hub pages.
    if !entry.main_themes.is_empty() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str("## Main Themes\n\n");
        for theme in &entry.main_themes {
            body.push_str(&format!("- [[{}|{}]]\n", theme.slug, theme.display));
        }
    }

    /* Publications: each entry links to source article via `[^art-{uuid}]`.
    The `art-` prefix required by wiki Markdown renderer for green `.art-ref` chips.
    No footnote definition block emitted: renderer resolves from in-memory sources map. */
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

    // Most Cited: the author's own top-cited publications (top 5).
    let mut cited: Vec<&AuthorArticle> =
        entry.articles.iter().filter(|a| a.citation_count.unwrap_or(0) > 0).collect();
    cited.sort_by_key(|a| std::cmp::Reverse(a.citation_count.unwrap_or(0)));
    cited.truncate(MOST_CITED_LIMIT);
    if !cited.is_empty() {
        body.push_str("\n## Most Cited\n\n");
        for article in cited {
            let year_str =
                article.year.map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
            body.push_str(&format!(
                "- \"{}\" ({}) - {} citations [^art-{}]\n",
                article.title,
                year_str,
                article.citation_count.unwrap_or(0),
                article.id
            ));
        }
    }

    // Research Areas section (deduplicated keywords).
    if !entry.keywords.is_empty() {
        body.push_str("\n## Research Areas\n\n");
        body.push_str(&entry.keywords.join(", "));
        body.push('\n');
    }

    // Key References: external works this author's papers rely on most.
    if !entry.references.is_empty() {
        body.push_str("\n## Key References\n\n");
        for reference in &entry.references {
            let year_str = reference.year.map(|y| y.to_string()).unwrap_or_default();
            let meta = if year_str.is_empty() { String::new() } else { format!(" ({year_str})") };
            body.push_str(&format!(
                "- \"{}\"{meta} - used by {} of this author's papers\n",
                reference.title, reference.times_used
            ));
        }
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
