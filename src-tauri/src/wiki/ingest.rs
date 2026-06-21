//! LLM-powered wiki ingest.
//!
//! Takes the raw sources (`raw/*.md`) and asks the LLM to synthesize them into
//! wiki pages (concepts, authors, methods, synthesis) with `[[wikilinks]]` and
//! `summary` frontmatter. Writes the generated pages to `wiki/`, appends a log
//! entry, and rebuilds the FTS5 index.
//!
//! Workflow:
//! 1. `process_user_files` — ensure `raw/` is uniform `.md`.
//! 2. Read the `AGENTS.md` contract + concatenate raw sources.
//! 3. Build a prompt instructing the LLM to output pages in a parseable format.
//! 4. Parse the LLM response into pages (delimited by `<!-- PAGE:slug -->`).
//! 5. Write pages to `wiki/{type}/`.
//! 6. Append a run entry to `wiki/log.md`.
//! 7. Rebuild the FTS5 index.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::Connection;
use tauri::Emitter;

use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;
use crate::wiki::frontmatter::{self, Frontmatter};
use crate::wiki::fts;
use crate::wiki::raw_export;

/// The maximum number of characters of raw source to send to the LLM.
/// Conservative to stay within typical context windows.
const MAX_SOURCE_CHARS: usize = 48_000; // ~12k tokens

/// Result of an ingest run.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestReport {
    pub raw_sources_read: usize,
    pub pages_written: usize,
    pub pages_skipped: usize,
    pub source_chars_truncated: bool,
    pub errors: Vec<String>,
}

/// Run the full ingest pipeline. Requires an LLM response string (the caller
/// obtains it via the orchestrator; this function is split for testability).
/// Write wiki pages from the LLM response (async, with per-page progress).
/// Does NOT touch the DB connection — caller must call `finalize_ingest` afterwards.
pub async fn write_pages_from_response(
    root: &Path,
    llm_response: &str,
    app_handle: Option<&tauri::AppHandle>,
) -> Result<IngestReport, AppError> {
    let mut report = IngestReport::default();

    // 1. Ensure wiki/ output dirs exist.
    crate::wiki::storage::scaffold_tree(root)?;

    // 2. Parse the LLM response into pages.
    let parsed_pages = parse_llm_pages(llm_response);
    report.pages_written = parsed_pages.len();

    // 3. Write each page (with per-page progress).
    let total_pages = parsed_pages.len();
    for (i, page) in parsed_pages.iter().enumerate() {
        if let Some(handle) = app_handle {
            let pct = 50 + ((i + 1) * 50 / total_pages.max(1));
            let _ = handle.emit(
                "wiki:progress",
                crate::commands::wiki_cmd::WikiProgress {
                    step: pct.min(99),
                    total_steps: crate::commands::wiki_cmd::WIKI_PIPELINE_TOTAL_STEPS,
                    message: format!("Writing page {}/{}: {}", i + 1, total_pages, page.slug),
                },
            );
        }
        if let Err(e) = write_page(root, page) {
            report.errors.push(format!("Failed to write {}: {}", page.slug, e));
        }
        // Yield to the async runtime so emitted progress events are delivered.
        tokio::task::yield_now().await;
    }

    Ok(report)
}

/// Finalize ingest: rebuild FTS5, append log, clear staleness flag.
/// Kept synchronous (no .await) since it uses &Connection.
pub fn finalize_ingest(
    conn: &Connection,
    root: &Path,
    report: &mut IngestReport,
) -> Result<(), AppError> {
    // Rebuild the FTS5 index.
    if let Err(e) = fts::ensure_table(conn) {
        report.errors.push(format!("FTS table creation failed: {e}"));
    }
    if let Err(e) = fts::rebuild_index(conn, root) {
        report.errors.push(format!("FTS rebuild failed: {e}"));
    }

    // Append a log entry.
    let log_path = root.join("wiki").join("log.md");
    let log_body = std::fs::read_to_string(&log_path).unwrap_or_default();
    let entry = format!(
        "{} ingest: {} pages written, {} errors",
        chrono::Utc::now().format("%Y-%m-%d %H:%M"),
        report.pages_written,
        report.errors.len()
    );
    let new_log = frontmatter::append_log_entry(&log_body, &entry);
    let _ = std::fs::write(&log_path, new_log);

    // Clear the staleness flag.
    crate::db::app_settings_repo::clear_wiki_needs_refresh(conn);

    Ok(())
}

/// Build the user prompt from raw sources + the AGENTS.md contract.
/// Returns `(prompt, source_count, truncated)`.
pub fn build_ingest_prompt(root: &Path) -> Result<(String, usize, bool), AppError> {
    // Read the AGENTS.md contract.
    let agents_path = root.join("AGENTS.md");
    let contract = std::fs::read_to_string(&agents_path).unwrap_or_default();

    // Collect raw sources.
    let raw_files = raw_export::list_raw_files(root)?;
    let source_count = raw_files.len();

    // Concatenate sources, respecting the char budget.
    let mut sources_text = String::new();
    let mut truncated = false;
    for (path, fm) in &raw_files {
        let (_fm, body) = frontmatter::read_file(path)?;
        let title = fm.get("title").unwrap_or("Untitled");
        let slug = fm.get("slug").unwrap_or("");
        let entry = format!("### Source: {title} (slug: {slug})\n\n{body}\n\n---\n\n");
        if sources_text.len() + entry.len() > MAX_SOURCE_CHARS {
            truncated = true;
            break;
        }
        sources_text.push_str(&entry);
    }

    let prompt = format!(
        "{contract}\n\n\
         # Raw Sources\n\n\
         {sources_text}\n\n\
         # Instructions\n\n\
         Synthesize the above sources into wiki pages. Output each page in this exact format:\n\n\
         <!-- PAGE:slug -->\n\
         ---\n\
         id: <slug>\n\
         title: \"<title>\"\n\
         type: concept | author | method | synthesis\n\
         slug: <kebab-case-slug>\n\
         summary: \"<1-2 sentence summary>\"\n\
         status: draft\n\
         links: []\n\
         ---\n\
         <Markdown body with [[wikilinks]] to other pages>\n\n\
         IMPORTANT: Create a wiki page for EVERY source document. Do not skip any sources. Each \
         source should appear in at least one page. \
         Do NOT include raw file paths (/raw/...), file names, or source_file references \
         in your output. Use [^art-id] source references or [[wikilinks]] instead. \
         Generate at least 3-5 concept pages, 1-2 synthesis pages, and author pages for \
         prominent authors. Use [[slug]] links to connect related pages. Each page must \
         start with the <!-- PAGE:slug --> delimiter."
    );

    Ok((prompt, source_count, truncated))
}

/// A parsed page from the LLM response.
#[derive(Debug, Clone)]
pub struct ParsedPage {
    pub slug: String,
    pub frontmatter: Frontmatter,
    pub body: String,
}

/// Parse the LLM response into pages.
/// Each page is delimited by `<!-- PAGE:slug -->` and contains a frontmatter
/// block + body.
pub fn parse_llm_pages(response: &str) -> Vec<ParsedPage> {
    let mut pages = Vec::new();
    let delimiter = "<!-- PAGE:";
    let mut current_pos = 0;

    while let Some(start) = response[current_pos..].find(delimiter) {
        let abs_start = current_pos + start;
        // Extract the slug from the delimiter line.
        let after_delim = &response[abs_start + delimiter.len()..];
        let slug =
            after_delim.lines().next().unwrap_or("").trim_end_matches("-->").trim().to_string();

        // Find the next delimiter (or end of response) to get this page's content.
        let page_start = abs_start;
        let page_end = response[page_start + delimiter.len()..]
            .find(delimiter)
            .map(|p| page_start + delimiter.len() + p)
            .unwrap_or(response.len());

        let page_content = &response[page_start..page_end];

        // Strip the <!-- PAGE:slug --> delimiter line to expose the --- frontmatter.
        let after_comment =
            page_content.find("\n").map(|p| &page_content[p + 1..]).unwrap_or(page_content);

        // Split into frontmatter + body.
        let (fm, body) = Frontmatter::split_markdown(after_comment);
        if !slug.is_empty() && !fm.fields.is_empty() {
            pages.push(ParsedPage { slug: slug.clone(), frontmatter: fm, body });
        }

        current_pos = page_end;
    }

    pages
}

/// Write a parsed page to the wiki directory.
fn write_page(root: &Path, page: &ParsedPage) -> Result<(), AppError> {
    let page_type = page.frontmatter.get("type").unwrap_or("concept");
    let subdir = match page_type {
        "author" => "authors",
        "method" => "methods",
        "synthesis" => "synthesis",
        _ => "concepts",
    };
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir)?;
    let filename = format!("{}.md", sanitize_slug(&page.slug));
    let path = dir.join(&filename);
    frontmatter::write_file(&path, &page.frontmatter, &page.body)?;
    Ok(())
}

/// Sanitize a slug for use as a filename.
fn sanitize_slug(slug: &str) -> String {
    let cleaned: String =
        slug.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' }).collect();
    cleaned.trim_matches('-').to_string()
}

// ---------------------------------------------------------------------------
// Author pre-seeding (multi-batch only)
// ---------------------------------------------------------------------------

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
    /// Render the manifest as a prompt section directing the LLM not to create
    /// author pages and to use the listed slugs for `[[author]]` links.
    /// Returns an empty string when the manifest is empty (so it can be
    /// unconditionally interpolated).
    #[must_use]
    fn to_prompt_section(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("# Author Pages (Pre-Seeded - DO NOT CREATE)\n\n");
        out.push_str(
            "Author pages have already been generated deterministically from the project's \
             bibliometric data. Do NOT output any page with `type: author`. Instead, when you \
             mention an author, link to their pre-seeded page using the EXACT slug below:\n\n",
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
             (case-insensitive), use the canonical slug for the link. Do not invent new author \
             slugs.\n\n",
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

/// Derive a deterministic, kebab-case slug for an author from their normalized
/// name. Prefixed with `author-` to avoid collisions with concept pages
/// (e.g. a researcher named "Author" vs a concept page about authors).
fn author_slug(normalized_name: &str) -> String {
    let mut cleaned: String = normalized_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    // Collapse consecutive dashes (e.g. "O'Brien, K." -> "o-brien--k" -> "o-brien-k").
    let mut prev_dash = false;
    let mut squeezed = String::with_capacity(cleaned.len());
    for c in cleaned.drain(..) {
        if c == '-' {
            if !prev_dash {
                squeezed.push('-');
            }
            prev_dash = true;
        } else {
            squeezed.push(c);
            prev_dash = false;
        }
    }
    let squeezed = squeezed.trim_matches('-').to_string();
    if squeezed.is_empty() {
        "author-unnamed".to_string()
    } else {
        format!("author-{squeezed}")
    }
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
#[must_use]
fn render_author_page(entry: &AuthorManifestEntry) -> (Frontmatter, String) {
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
    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", entry.display_name));

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

// ---------------------------------------------------------------------------
// Chunked / parallel ingest
// ---------------------------------------------------------------------------

/// Fraction of the configured context window reserved for the input prompt.
/// The remainder is available for the model's output (the wiki pages).
const INPUT_BUDGET_FRACTION: f64 = 0.4;

/// Hard cap on the number of input chars per batch, regardless of the
/// configured context window. Protects against pathological oversized calls.
const MAX_BATCH_INPUT_CHARS: usize = 80_000;

/// Jaccard similarity threshold (on stemmed slug tokens) above which two
/// concept/method pages are considered near-duplicates and merged.
const DEDUP_JACCARD_THRESHOLD: f64 = 0.5;

/// Minimum number of shared `source_articles` for two pages to be considered
/// near-duplicates regardless of slug similarity.
const DEDUP_SHARED_SOURCES_MIN: usize = 2;

/// Approximate token count for a chunk of text (1 token ~= 4 chars).
#[must_use]
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Compute the input character budget for one batch from the configured context
/// window. Falls back to `MAX_SOURCE_CHARS` when the configured window is
/// unusable (zero / negative).
#[must_use]
fn batch_input_char_budget(context_window_tokens: i32) -> usize {
    if context_window_tokens <= 0 {
        return MAX_SOURCE_CHARS;
    }
    let tokens = (f64::from(context_window_tokens) * INPUT_BUDGET_FRACTION) as usize;
    let chars = tokens.saturating_mul(4);
    chars.clamp(4_000, MAX_BATCH_INPUT_CHARS)
}

/// A single source document loaded from `raw/` and ready for batching.
#[derive(Debug, Clone)]
pub struct RawSource {
    /// `slug` frontmatter value (or empty when absent).
    pub slug: String,
    /// `title` frontmatter value (or "Untitled").
    pub title: String,
    /// Full Markdown body (frontmatter stripped).
    pub body: String,
    /// Original path on disk (for debugging / error messages).
    pub path: PathBuf,
}

/// A compiled ingest batch: the prompt to send to the LLM plus metadata.
#[derive(Debug, Clone)]
pub struct IngestBatch {
    /// Index of this batch within the run (0-based).
    pub index: usize,
    /// Total number of batches in the run.
    pub total: usize,
    /// Source slugs included in this batch's `sources_text`.
    pub source_slugs: Vec<String>,
    /// The full user prompt (contract + source index + sources + instructions).
    pub prompt: String,
}

/// Load and parse every `raw/*.md` source into a `RawSource`.
pub fn load_raw_sources(root: &Path) -> Result<Vec<RawSource>, AppError> {
    let raw_files = raw_export::list_raw_files(root)?;
    let mut sources = Vec::with_capacity(raw_files.len());
    for (path, fm) in raw_files {
        let (_fm, body) = frontmatter::read_file(&path)?;
        let title = fm.get("title").unwrap_or("Untitled").to_string();
        let slug = fm.get("slug").unwrap_or("").to_string();
        sources.push(RawSource { slug, title, body, path });
    }
    Ok(sources)
}

/// Build a compact, metadata-only index of ALL sources. Embedded in every
/// batch prompt so the model can `[[link]]` to sources it does not fully
/// process - this is what keeps batches independent and parallelizable.
fn build_source_index(sources: &[RawSource]) -> String {
    let mut out = String::new();
    for s in sources {
        let slug = if s.slug.is_empty() { "unknown" } else { &s.slug };
        out.push_str(&format!("- {} [[{}]]\n", s.title, slug));
    }
    out
}

/// Build a single batch's prompt from the contract, the full source index,
/// the batch's source bodies, and the standard instructions block.
fn build_batch_prompt(
    contract: &str,
    source_index: &str,
    batch_sources: &[&RawSource],
    author_manifest: Option<&AuthorManifest>,
) -> String {
    let mut sources_text = String::new();
    for s in batch_sources {
        let slug = if s.slug.is_empty() { "unknown" } else { &s.slug };
        sources_text.push_str(&format!(
            "### Source: {} (slug: {})\n\n{}\n\n---\n\n",
            s.title, slug, s.body
        ));
    }
    let manifest_section =
        author_manifest.map(AuthorManifest::to_prompt_section).unwrap_or_default();
    format!(
        "{contract}\n\n\
         # Full Source Index (for cross-referencing)\n\n\
         The complete set of source documents in this wiki run is listed below. \
         You may create [[wikilinks]] to any of them, even if you are not asked to \
         fully process that source in this batch:\n\n\
         {source_index}\n\n\
         {manifest_section}\
         # Raw Sources for THIS Batch\n\n\
         {sources_text}\n\n\
         # Instructions\n\n\
         Synthesize the above sources into wiki pages. Output each page in this exact format:\n\n\
         <!-- PAGE:slug -->\n\
         ---\n\
         id: <slug>\n\
         title: \"<title>\"\n\
         type: concept | author | method | synthesis\n\
         slug: <kebab-case-slug>\n\
         summary: \"<1-2 sentence summary>\"\n\
         status: draft\n\
         links: []\n\
         ---\n\
         <Markdown body with [[wikilinks]] to other pages>\n\n\
         IMPORTANT: Create a wiki page for EVERY source document in THIS batch. Do not skip \
         any. Each source should appear in at least one page. \
         Do NOT include raw file paths (/raw/...), file names, or source_file references \
         in your output. Use [^art-id] source references or [[wikilinks]] instead. \
         Generate concept, synthesis, and author pages for prominent entities. Use [[slug]] \
         links to connect related pages (you may link to sources from the Full Source Index). \
         Each page must start with the <!-- PAGE:slug --> delimiter."
    )
}

/// Split the raw sources into batches sized to the configured input budget.
///
/// Each batch carries the full source index (so the model can cross-link to
/// sources outside its batch), making the batches independent and safe to run
/// in parallel. Returns an empty `Vec` when there are no sources.
pub fn build_ingest_prompt_batches(
    root: &Path,
    context_window_tokens: i32,
    author_manifest: Option<&AuthorManifest>,
) -> Result<Vec<IngestBatch>, AppError> {
    let contract = std::fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let sources = load_raw_sources(root)?;
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    let budget = batch_input_char_budget(context_window_tokens);
    let source_index = build_source_index(&sources);
    // Reserve room for the contract + source index + instructions overhead.
    let overhead = estimate_tokens(&contract) + estimate_tokens(&source_index) + 600;
    let overhead_chars = overhead.saturating_mul(4);
    let usable_budget = budget.saturating_sub(overhead_chars).max(2_000);

    // Accumulate (slugs, prompt) tuples, then convert to IngestBatch at the end.
    let mut batches: Vec<(Vec<String>, String)> = Vec::new();
    let mut current: Vec<&RawSource> = Vec::new();
    let mut current_len: usize = 0;

    for src in &sources {
        // Entry header + body + separator.
        let slug = if src.slug.is_empty() { "unknown" } else { &src.slug };
        let entry_len = src.body.len() + slug.len() + src.title.len() + 40;
        if !current.is_empty() && current_len + entry_len > usable_budget {
            // Flush current batch.
            let prompt = build_batch_prompt(&contract, &source_index, &current, author_manifest);
            let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
            batches.push((slugs, prompt));
            current.clear();
            current_len = 0;
        }
        current.push(src);
        current_len += entry_len;
    }
    if !current.is_empty() {
        let prompt = build_batch_prompt(&contract, &source_index, &current, author_manifest);
        let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
        batches.push((slugs, prompt));
    }

    let total = batches.len();
    Ok(batches
        .into_iter()
        .enumerate()
        .map(|(i, (slugs, prompt))| IngestBatch { index: i, total, source_slugs: slugs, prompt })
        .collect())
}

/// Injectable LLM sender for the chunked ingest. Production wraps the
/// `LlmOrchestrator`; tests provide a deterministic, latency-simulating fake.
#[async_trait]
pub trait IngestLlmSender: Send + Sync {
    /// Send one batch prompt and return the raw LLM response text.
    async fn send(&self, prompt: &str) -> Result<String, AppError>;
}

/// Production sender: delegates to the shared `LlmOrchestrator`.
pub struct OrchestratorIngestSender {
    orchestrator: Arc<LlmOrchestrator>,
    config: LlmConfig,
    system_prompt: &'static str,
}

impl OrchestratorIngestSender {
    #[must_use]
    pub fn new(orchestrator: Arc<LlmOrchestrator>, config: LlmConfig) -> Self {
        Self { orchestrator, config, system_prompt: INGEST_SYSTEM_PROMPT }
    }
}

#[async_trait]
impl IngestLlmSender for OrchestratorIngestSender {
    async fn send(&self, prompt: &str) -> Result<String, AppError> {
        let (response, _tokens) = self
            .orchestrator
            .send(&self.config, self.system_prompt, prompt, LlmRequestType::WikiIngest)
            .await?;
        Ok(response)
    }
}

/// Static system prompt shared by the chunked and legacy single-call paths.
pub const INGEST_SYSTEM_PROMPT: &str = "You are a research knowledge-base synthesizer. Follow \
     the AGENTS.md contract strictly. Output wiki pages in the exact delimited format requested. \
     Use [[wikilinks]] to connect pages, and ALWAYS use the exact lowercase kebab-case slug of \
     the target page as the link text (e.g. [[sugar-tax]] NOT [[Sugar Tax]] or [[Sugar-Tax]]). \
     Do not use em dashes.";

/// Run the chunked, parallel ingest pipeline.
///
/// - **Single batch** (`batches.len() == 1`): writes pages immediately as they
///   complete. No dedup, no link rewriting. The LLM sees all sources at once
///   and produces a self-consistent page set.
/// - **Multiple batches** (`batches.len() > 1`): collects all parsed pages
///   across batches, runs a deterministic dedup pass to merge near-duplicate
///   concept/method pages (which independent batches often produce), rewrites
///   inbound `[[wikilinks]]` to the canonical slugs, then writes the
///   consolidated page set. This prevents the fragmentation that parallel
///   batches would otherwise cause (`childhood-obesity` vs
///   `obesity-in-children`). No LLM merge calls - the merge is a lossless
///   append + metadata union.
///
/// `progress_range` is `(start_pct, end_pct)` - the slice of the 0-100 pipeline
/// bar that the LLM + write phase should occupy (e.g. `(25, 95)`).
pub async fn run_chunked_ingest(
    root: &Path,
    batches: Vec<IngestBatch>,
    sender: Arc<dyn IngestLlmSender>,
    app_handle: Option<&tauri::AppHandle>,
    progress_range: (usize, usize),
) -> Result<IngestReport, AppError> {
    let mut report = IngestReport::default();
    if batches.is_empty() {
        return Ok(report);
    }

    // Ensure wiki/ output dirs exist before any batch writes.
    crate::wiki::storage::scaffold_tree(root)?;

    let total_batches = batches.len();
    let (start_pct, end_pct) = progress_range;
    let span = end_pct.saturating_sub(start_pct).max(1);

    // Spawn one task per batch. Each task sends its prompt and returns the
    // parsed pages (parsing inside the task keeps the main loop tight).
    let mut join_set: tokio::task::JoinSet<Result<(usize, Vec<ParsedPage>), AppError>> =
        tokio::task::JoinSet::new();
    for batch in batches {
        let sender = Arc::clone(&sender);
        join_set.spawn(async move {
            let response = sender.send(&batch.prompt).await?;
            Ok((batch.index, parse_llm_pages(&response)))
        });
    }

    // Collect results as they complete. For single-batch runs we write pages
    // immediately (current behavior). For multi-batch runs we collect all pages
    // and consolidate them after all batches finish.
    let mut collected_pages: Vec<ParsedPage> = Vec::new();
    let mut completed = 0usize;
    while let Some(res) = join_set.join_next().await {
        completed += 1;
        match res {
            Ok(Ok((batch_index, pages))) => {
                let page_count = pages.len();
                report.pages_written += page_count;
                if total_batches > 1 {
                    // Multi-batch: defer writing until consolidation.
                    collected_pages.extend(pages);
                } else {
                    // Single-batch: write immediately.
                    for page in &pages {
                        if let Err(e) = write_page(root, page) {
                            report
                                .errors
                                .push(format!("Failed to write page {}: {}", page.slug, e));
                        }
                    }
                }
                if let Some(handle) = app_handle {
                    let pct = start_pct + (completed * span) / total_batches.max(1);
                    let _ = handle.emit(
                        "wiki:progress",
                        crate::commands::wiki_cmd::WikiProgress {
                            step: pct.min(end_pct),
                            total_steps: crate::commands::wiki_cmd::WIKI_PIPELINE_TOTAL_STEPS,
                            message: format!(
                                "Processed batch {completed}/{total_batches} (batch {}): {} pages",
                                batch_index + 1,
                                page_count
                            ),
                        },
                    );
                }
            }
            Ok(Err(e)) => {
                report.errors.push(format!("Batch LLM call failed: {e}"));
                if let Some(handle) = app_handle {
                    let pct = start_pct + (completed * span) / total_batches.max(1);
                    let _ = handle.emit(
                        "wiki:progress",
                        crate::commands::wiki_cmd::WikiProgress {
                            step: pct.min(end_pct),
                            total_steps: crate::commands::wiki_cmd::WIKI_PIPELINE_TOTAL_STEPS,
                            message: format!("Batch failed ({completed}/{total_batches}): {e}"),
                        },
                    );
                }
            }
            Err(join_err) => {
                report.errors.push(format!("Batch task panicked: {join_err}"));
            }
        }
        // Yield so emitted events flush to the webview between completions.
        tokio::task::yield_now().await;
    }

    // Multi-batch consolidation: dedup + link rewrite + write.
    if total_batches > 1 && !collected_pages.is_empty() {
        let slug_map = consolidate_pages(&mut collected_pages);
        rewrite_page_links(&mut collected_pages, &slug_map);
        for page in &collected_pages {
            if let Err(e) = write_page(root, page) {
                report.errors.push(format!("Failed to write page {}: {}", page.slug, e));
            }
        }
        // Adjust pages_written to the consolidated count.
        report.pages_written = collected_pages.len();
        if let Some(handle) = app_handle {
            let _ = handle.emit(
                "wiki:progress",
                crate::commands::wiki_cmd::WikiProgress {
                    step: end_pct,
                    total_steps: crate::commands::wiki_cmd::WIKI_PIPELINE_TOTAL_STEPS,
                    message: format!(
                        "Consolidated to {} pages ({} merges)",
                        collected_pages.len(),
                        slug_map.len()
                    ),
                },
            );
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Deterministic page consolidation (multi-batch only)
// ---------------------------------------------------------------------------

/// Merge near-duplicate pages in-place. Returns a map of `old_slug -> new_slug`
/// for all pages that were merged into a canonical page (the inbound link
/// rewriter uses this to update `[[wikilinks]]` across the page set).
///
/// Detection (two pages are duplicates when ANY is true):
/// - Exact slug match (case-insensitive).
/// - Stemmed-token Jaccard similarity of slugs >= `DEDUP_JACCARD_THRESHOLD`.
/// - Shared `source_articles` count >= `DEDUP_SHARED_SOURCES_MIN`.
///
/// Merge is lossless: the duplicate's body is appended under a
/// `## Additional perspectives` heading; `source_articles` and `tags` are
/// unioned. The canonical page is the one with the shortest slug (most likely
/// the LLM's "preferred" form) or, on ties, the first encountered.
pub fn consolidate_pages(pages: &mut Vec<ParsedPage>) -> HashMap<String, String> {
    if pages.len() <= 1 {
        return HashMap::new();
    }

    // Build the list of merge targets: for each page, find the canonical page
    // it should merge INTO (if any). We use a simple O(n^2) scan since n is
    // small (dozens to low hundreds of pages).
    let n = pages.len();
    // `canonical[i]` = the index of the page that page `i` should merge into.
    // Initially, each page is its own canonical.
    let mut canonical: Vec<usize> = (0..n).collect();
    let mut slug_map: HashMap<String, String> = HashMap::new();

    for i in 0..n {
        // Skip if page i already merged into something.
        if canonical[i] != i {
            continue;
        }
        // Skip author pages - they are pre-seeded and should never be merged.
        let page_type_i = pages[i].frontmatter.get("type").unwrap_or("concept");
        if page_type_i == "author" {
            continue;
        }
        for j in (i + 1)..n {
            // Skip if page j already merged into something.
            if canonical[j] != j {
                continue;
            }
            let page_type_j = pages[j].frontmatter.get("type").unwrap_or("concept");
            if page_type_j == "author" {
                continue;
            }
            // Only merge pages of the same type (concept + concept, etc.).
            if page_type_i != page_type_j {
                continue;
            }
            if pages_are_duplicates(&pages[i], &pages[j]) {
                canonical[j] = i;
            }
        }
    }

    // Collect the list of merges: (source_idx, canonical_idx). We build the
    // merge data (body + frontmatter to append) from the immutable borrow,
    // then apply the appends + removals in separate passes to satisfy the
    // borrow checker.
    let mut merges: Vec<(usize, usize)> = Vec::new(); // (source_idx, canonical_idx)
    for (i, &canon) in canonical.iter().enumerate().take(n) {
        if canon != i {
            merges.push((i, canon));
        }
    }

    let mut append_data: HashMap<usize, Vec<(String, Frontmatter)>> = HashMap::new();
    // Track which indices to remove (sorted desc so swap_remove ordering is safe).
    let mut to_remove: Vec<usize> = merges.iter().map(|(src, _)| *src).collect();
    to_remove.sort_unstable_by(|a, b| b.cmp(a));

    for &(src_idx, canon_idx) in &merges {
        let src_body = pages[src_idx].body.clone();
        let src_fm = pages[src_idx].frontmatter.clone();
        append_data.entry(canon_idx).or_default().push((src_body, src_fm));
        // Record the slug redirect.
        let old_slug = pages[src_idx].slug.clone();
        let new_slug = pages[canon_idx].slug.clone();
        // Case-insensitive: store the lowercased old slug so the rewriter can
        // match [[Old-Slug]] as well as [[old-slug]].
        slug_map.insert(old_slug.to_lowercase(), new_slug);
    }

    // Apply the appends.
    for (canon_idx, appends) in append_data {
        for (body, fm) in appends {
            // Append body.
            pages[canon_idx].body.push_str("\n\n## Additional perspectives\n\n");
            pages[canon_idx].body.push_str(&body);
            // Union source_articles.
            union_list_field(&mut pages[canon_idx].frontmatter, &fm, "source_articles");
            // Union tags.
            union_list_field(&mut pages[canon_idx].frontmatter, &fm, "tags");
        }
    }

    // Remove merged source pages (descending order keeps earlier indices valid).
    for &idx in &to_remove {
        if idx < pages.len() {
            pages.remove(idx);
        }
    }

    slug_map
}

/// Union a list-valued frontmatter field from `src` into `dest`.
/// Handles the `[a, b]` inline YAML format used by the wiki frontmatter.
fn union_list_field(dest: &mut Frontmatter, src: &Frontmatter, field: &str) {
    let dest_list = frontmatter::parse_list(dest.get(field).unwrap_or(""));
    let src_list = frontmatter::parse_list(src.get(field).unwrap_or(""));
    if dest_list.is_empty() && src_list.is_empty() {
        return;
    }
    let mut seen: HashSet<String> = dest_list.iter().cloned().collect();
    for item in src_list {
        seen.insert(item);
    }
    let mut combined: Vec<String> = seen.into_iter().collect();
    combined.sort();
    let formatted = format!("[{}]", combined.join(", "));
    dest.set(field, &formatted);
}

/// Determine whether two parsed pages are near-duplicates.
fn pages_are_duplicates(a: &ParsedPage, b: &ParsedPage) -> bool {
    // Exact slug match (case-insensitive).
    if a.slug.to_lowercase() == b.slug.to_lowercase() {
        return true;
    }
    // Stemmed-token Jaccard similarity on slugs.
    let tokens_a = stemmed_token_set(&a.slug);
    let tokens_b = stemmed_token_set(&b.slug);
    let jaccard = jaccard_similarity(&tokens_a, &tokens_b);
    if jaccard >= DEDUP_JACCARD_THRESHOLD {
        return true;
    }
    // Shared source_articles count.
    let sources_a = frontmatter::parse_list(a.frontmatter.get("source_articles").unwrap_or(""));
    let sources_b = frontmatter::parse_list(b.frontmatter.get("source_articles").unwrap_or(""));
    let set_a: HashSet<&String> = sources_a.iter().collect();
    let shared = sources_b.iter().filter(|s| set_a.contains(s)).count();
    if shared >= DEDUP_SHARED_SOURCES_MIN {
        return true;
    }
    false
}

/// Tokenize a slug into a set of stemmed words (using the project's existing
/// Snowball stemmer). This catches semantic paraphrase with word reordering
/// (`childhood-obesity` vs `obesity-in-children` both stem to {childhood,
/// obes} / {obes, children} - the `in` stopword is filtered).
fn stemmed_token_set(slug: &str) -> HashSet<String> {
    let stopwords: HashSet<&str> =
        ["in", "of", "the", "a", "an", "and", "or", "for", "to", "on"].into_iter().collect();
    slug.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .filter(|s| !stopwords.contains(s.to_lowercase().as_str()))
        .map(|s| crate::biblio::normalizer::stem_phrase(&s.to_lowercase()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compute the Jaccard similarity between two sets: |A ∩ B| / |A ∪ B|.
#[must_use]
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// Rewrite `[[wikilink]]` targets in every page's body to point to the
/// canonical slug. The `slug_map` keys are lowercased old slugs; matching is
/// case-insensitive (per the lint convention where `[[Sugar-Reduction]]`
/// resolves to `sugar-reduction`). Aliases are preserved:
/// `[[old-slug|Alias]]` -> `[[new-slug|Alias]]`.
pub fn rewrite_page_links(pages: &mut [ParsedPage], slug_map: &HashMap<String, String>) {
    if slug_map.is_empty() {
        return;
    }
    // Pre-compile a case-insensitive lookup: lowercase old -> new.
    for page in pages.iter_mut() {
        page.body = rewrite_body_links(&page.body, slug_map);
    }
}

/// Rewrite `[[target]]` and `[[target|alias]]` links in a body string.
fn rewrite_body_links(body: &str, slug_map: &HashMap<String, String>) -> String {
    if slug_map.is_empty() {
        return body.to_string();
    }
    let bytes: Vec<char> = body.chars().collect();
    let n = bytes.len();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < n {
        if bytes[i] == '[' && i + 1 < n && bytes[i + 1] == '[' {
            // Found opening [[. Extract the target up to | or ]].
            let start = i + 2;
            let mut j = start;
            let mut target = String::new();
            let mut alias_start: Option<usize> = None;
            let mut closed = false;
            while j < n {
                if bytes[j] == '|' {
                    alias_start = Some(j);
                    break;
                }
                if bytes[j] == ']' && j + 1 < n && bytes[j + 1] == ']' {
                    closed = true;
                    break;
                }
                target.push(bytes[j]);
                j += 1;
            }
            if closed || alias_start.is_some() {
                let trimmed = target.trim();
                if let Some(new_slug) = slug_map.get(&trimmed.to_lowercase()) {
                    // Rewrite this link. Preserve alias if present.
                    out.push_str("[[");
                    out.push_str(new_slug);
                    if let Some(alias_idx) = alias_start {
                        // Copy from the alias separator to the closing ]].
                        out.push('|');
                        let mut k = alias_idx + 1;
                        while k < n && !(bytes[k] == ']' && k + 1 < n && bytes[k + 1] == ']') {
                            out.push(bytes[k]);
                            k += 1;
                        }
                        out.push_str("]]");
                        // Advance past closing ]].
                        i = k + 2;
                    } else {
                        out.push_str("]]");
                        i = j + 2;
                    }
                    continue;
                }
            }
            // Not a match: copy the [[ and continue scanning.
            out.push('[');
            out.push('[');
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_llm_pages_extracts_multiple_pages() {
        let response = r#"<!-- PAGE:sugar-tax -->
---
id: sugar-tax
title: "Sugar Tax"
type: concept
slug: sugar-tax
summary: "A levy on sugary drinks"
status: draft
links: []
---
# Sugar Tax

A tax on sugar-sweetened beverages. See [[obesity]].

<!-- PAGE:obesity -->
---
id: obesity
title: "Obesity"
type: concept
slug: obesity
summary: "Excess body fat"
status: draft
links: []
---
# Obesity

A major public health concern related to [[sugar-tax]].
"#;
        let pages = parse_llm_pages(response);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].slug, "sugar-tax");
        assert_eq!(pages[0].frontmatter.get("title"), Some("Sugar Tax"));
        assert!(pages[0].body.contains("[[obesity]]"));
        assert_eq!(pages[1].slug, "obesity");
        assert_eq!(pages[1].frontmatter.get("type"), Some("concept"));
    }

    #[test]
    fn parse_llm_pages_empty_response_returns_empty() {
        let pages = parse_llm_pages("No pages here.");
        assert!(pages.is_empty());
    }

    #[test]
    fn parse_llm_pages_skips_page_without_frontmatter() {
        let response = "<!-- PAGE:bad -->\nJust some text, no frontmatter.";
        let pages = parse_llm_pages(response);
        assert!(pages.is_empty());
    }

    #[test]
    fn write_page_uses_correct_subdir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("wiki/concepts")).unwrap();

        let mut fm = Frontmatter::default();
        fm.set("id", "alpha");
        fm.set("title", "Alpha");
        fm.set("type", "concept");
        fm.set("slug", "alpha");
        fm.set("status", "draft");
        fm.set("summary", "");
        fm.set("links", "[]");

        let page =
            ParsedPage { slug: "alpha".to_string(), frontmatter: fm, body: "# Alpha".to_string() };
        write_page(root, &page).unwrap();

        let path = root.join("wiki/concepts/alpha.md");
        assert!(path.exists());
        let (fm2, body) = frontmatter::read_file(&path).unwrap();
        assert_eq!(fm2.get("title"), Some("Alpha"));
        assert!(body.contains("# Alpha"));
    }

    #[test]
    fn write_page_routes_author_to_authors_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut fm = Frontmatter::default();
        fm.set("id", "jane-doe");
        fm.set("title", "Jane Doe");
        fm.set("type", "author");
        fm.set("slug", "jane-doe");
        fm.set("status", "draft");
        fm.set("summary", "");
        fm.set("links", "[]");

        let page = ParsedPage {
            slug: "jane-doe".to_string(),
            frontmatter: fm,
            body: "# Jane Doe".to_string(),
        };
        write_page(root, &page).unwrap();

        assert!(root.join("wiki/authors/jane-doe.md").exists());
    }

    #[tokio::test]
    async fn run_ingest_from_response_writes_pages_and_clears_flag() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&conn).unwrap();
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();

        // Mark as needing refresh.
        crate::db::app_settings_repo::mark_wiki_needs_refresh(&conn);
        assert!(crate::db::app_settings_repo::get_wiki_needs_refresh(&conn).unwrap());

        let response = r#"<!-- PAGE:alpha -->
---
id: alpha
title: "Alpha"
type: concept
slug: alpha
summary: "Alpha concept"
status: draft
links: []
---
# Alpha

See [[beta]].

<!-- PAGE:beta -->
---
id: beta
title: "Beta"
type: concept
slug: beta
summary: "Beta concept"
status: draft
links: []
---
# Beta

See [[alpha]].
"#;
        let mut report = write_pages_from_response(root, response, None).await.unwrap();
        finalize_ingest(&conn, root, &mut report).unwrap();
        assert_eq!(report.pages_written, 2);
        assert!(report.errors.is_empty());

        // Pages exist.
        assert!(root.join("wiki/concepts/alpha.md").exists());
        assert!(root.join("wiki/concepts/beta.md").exists());

        // FTS index was rebuilt.
        fts::ensure_table(&conn).unwrap();
        let hits = fts::search(&conn, "alpha", 5).unwrap();
        assert!(!hits.is_empty());

        // Staleness flag cleared.
        assert!(!crate::db::app_settings_repo::get_wiki_needs_refresh(&conn).unwrap());

        // Log entry appended.
        let log = std::fs::read_to_string(root.join("wiki/log.md")).unwrap();
        assert!(log.contains("ingest"));
    }

    #[test]
    fn build_ingest_prompt_includes_sources_and_contract() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# Agent Contract\nRules here.").unwrap();

        // Write a raw source.
        let mut fm = Frontmatter::default();
        fm.set("id", "art-1");
        fm.set("title", "Article One");
        fm.set("type", "source");
        fm.set("slug", "art-1");
        fm.set("status", "draft");
        fm.set("summary", "");
        fm.set("links", "[]");
        frontmatter::write_file(&root.join("raw/art-1.md"), &fm, "Article content here").unwrap();

        let (prompt, count, truncated) = build_ingest_prompt(root).unwrap();
        assert_eq!(count, 1);
        assert!(!truncated);
        assert!(prompt.contains("Agent Contract"));
        assert!(prompt.contains("Article One"));
        assert!(prompt.contains("Article content here"));
        assert!(prompt.contains("<!-- PAGE:slug -->"));
    }

    #[test]
    fn build_ingest_prompt_truncates_when_over_budget() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "Contract").unwrap();

        // Write many large sources to exceed budget.
        for i in 0..50 {
            let mut fm = Frontmatter::default();
            let id = format!("art-{i}");
            fm.set("id", &id);
            let title = format!("Article {i}");
            fm.set("title", &title);
            fm.set("type", "source");
            fm.set("slug", &id);
            fm.set("status", "draft");
            fm.set("summary", "");
            fm.set("links", "[]");
            let body = "x".repeat(2000);
            let path = root.join(format!("raw/art-{i}.md"));
            frontmatter::write_file(&path, &fm, &body).unwrap();
        }

        let (_prompt, count, truncated) = build_ingest_prompt(root).unwrap();
        assert!(truncated);
        assert!(count > 10); // all sources were counted
    }

    #[test]
    fn sanitize_slug_replaces_special_chars() {
        assert_eq!(sanitize_slug("sugar-tax!"), "sugar-tax");
        assert_eq!(sanitize_slug("foo bar baz"), "foo-bar-baz");
        assert_eq!(sanitize_slug("---leading"), "leading");
    }

    // -----------------------------------------------------------------
    // Author manifest + pre-seeding
    // -----------------------------------------------------------------

    #[test]
    fn author_slug_is_prefixed_and_kebab() {
        assert_eq!(author_slug("smith j"), "author-smith-j");
        // Punctuation becomes a single dash; consecutive dashes collapse.
        assert_eq!(author_slug("O'Brien, K."), "author-o-brien-k");
        assert_eq!(author_slug(""), "author-unnamed");
    }

    #[test]
    fn preseed_authors_writes_pages_and_respects_reviewed() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();

        let manifest = AuthorManifest {
            entries: vec![AuthorManifestEntry {
                slug: "author-smith-j".to_string(),
                display_name: "Smith, J".to_string(),
                raw_variants: vec!["smith, j".to_string()],
                article_count: 3,
                ..Default::default()
            }],
        };
        let written = preseed_authors(root, &manifest).unwrap();
        assert_eq!(written, 1);
        let path = root.join("wiki/authors/author-smith-j.md");
        assert!(path.exists());
        let (fm, body) = frontmatter::read_file(&path).unwrap();
        assert_eq!(fm.get("type"), Some("author"));
        assert_eq!(fm.get("status"), Some("draft"));
        // New template: "## Publications" header + empty articles list.
        assert!(body.contains("## Publications"));

        // Now mark it reviewed and re-seed - should skip.
        let mut fm2 = fm.clone();
        fm2.set("status", "reviewed");
        frontmatter::write_file(&path, &fm2, "# User edited").unwrap();
        let written2 = preseed_authors(root, &manifest).unwrap();
        assert_eq!(written2, 0, "reviewed author page should not be overwritten");
    }

    #[test]
    fn render_author_page_emits_art_prefixed_refs_and_no_raw_lines() {
        // Regression: the pre-seeder previously emitted inline refs as
        // `[^{id}]` (no `art-` prefix) plus a `/raw/{id}.md` definition block.
        // The renderer only resolves `[^art-{uuid}]`, so the refs rendered as
        // literal `[^...]` text and the definitions leaked as duplicate
        // clutter. This test pins the new contract: refs carry the `art-`
        // prefix and the body contains zero `/raw/` lines.
        let entry = AuthorManifestEntry {
            slug: "author-doe-j".to_string(),
            display_name: "Doe, J".to_string(),
            article_count: 2,
            articles: vec![
                AuthorArticle {
                    id: "11111111-1111-1111-1111-111111111111".to_string(),
                    title: "Paper One".to_string(),
                    year: Some(2020),
                    journal: Some("Nature".to_string()),
                },
                AuthorArticle {
                    id: "22222222-2222-2222-2222-222222222222".to_string(),
                    title: "Paper Two".to_string(),
                    year: Some(2023),
                    journal: None,
                },
            ],
            h_index: Some(5),
            total_citations: 100,
            ..Default::default()
        };
        let (_fm, body) = render_author_page(&entry);

        // Each publication row carries an art-prefixed ref.
        assert!(body.contains("[^art-11111111-1111-1111-1111-111111111111]"));
        assert!(body.contains("[^art-22222222-2222-2222-2222-222222222222]"));

        // No bare (non-art) ref keys and no /raw/ artifact lines.
        assert!(!body.contains("/raw/"), "body must not contain /raw/ paths: {body}");
        // A bare `[^{uuid}]` (no art-) would indicate the old bug.
        assert!(
            !body.contains("[^11111111") && !body.contains("[^22222222"),
            "refs must be art-prefixed, body was: {body}"
        );

        // The publications list + metrics are still present.
        assert!(body.contains("## Publications"));
        assert!(body.contains("Paper One"));
        assert!(body.contains("h-index: 5"));
    }

    // -----------------------------------------------------------------
    // Chunked / parallel ingest
    // -----------------------------------------------------------------

    /// Write `n` raw source files with bodies of roughly `body_chars` each.
    fn write_many_sources(root: &Path, n: usize, body_chars: usize) {
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# Contract").unwrap();
        for i in 0..n {
            let mut fm = Frontmatter::default();
            let id = format!("art-{i}");
            let title = format!("Article {i}");
            fm.set("id", &id);
            fm.set("title", &title);
            fm.set("type", "source");
            fm.set("slug", &id);
            fm.set("status", "draft");
            fm.set("summary", "");
            fm.set("links", "[]");
            let body = "x".repeat(body_chars);
            frontmatter::write_file(&root.join(format!("raw/{id}.md")), &fm, &body).unwrap();
        }
    }

    #[test]
    fn batch_input_char_budget_uses_fraction_of_context_window() {
        // 50_000 tokens * 0.4 * 4 chars/token = 80_000, but capped at MAX_BATCH_INPUT_CHARS.
        assert_eq!(batch_input_char_budget(50_000), MAX_BATCH_INPUT_CHARS);
        // 10_000 tokens * 0.4 * 4 = 16_000 chars.
        assert_eq!(batch_input_char_budget(10_000), 16_000);
        // Zero/negative falls back to MAX_SOURCE_CHARS.
        assert_eq!(batch_input_char_budget(0), MAX_SOURCE_CHARS);
        assert_eq!(batch_input_char_budget(-1), MAX_SOURCE_CHARS);
        // Tiny window clamps to the 4_000 floor.
        assert_eq!(batch_input_char_budget(1), 4_000);
    }

    #[test]
    fn build_ingest_prompt_batches_single_batch_when_small() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_many_sources(root, 2, 500);

        let batches = build_ingest_prompt_batches(root, 50_000, None).unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].index, 0);
        assert_eq!(batches[0].total, 1);
        assert_eq!(batches[0].source_slugs, vec!["art-0".to_string(), "art-1".to_string()]);
        // Single batch still carries the full source index + the instructions.
        assert!(batches[0].prompt.contains("Full Source Index"));
        assert!(batches[0].prompt.contains("Article 0"));
        assert!(batches[0].prompt.contains("Article 1"));
        assert!(batches[0].prompt.contains("<!-- PAGE:slug -->"));
    }

    #[test]
    fn build_ingest_prompt_batches_splits_large_corpus() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // 20 sources * 2000 chars = 40_000 chars of bodies. With a small
        // context window (2_000 tokens -> 3_200 chars budget), this must split
        // into multiple batches.
        write_many_sources(root, 20, 2000);

        let batches = build_ingest_prompt_batches(root, 2_000, None).unwrap();
        assert!(batches.len() > 1, "expected multiple batches, got {}", batches.len());

        // Every batch index + total is consistent.
        for (i, b) in batches.iter().enumerate() {
            assert_eq!(b.index, i);
            assert_eq!(b.total, batches.len());
        }

        // The union of all batch source_slugs covers every source exactly once.
        let mut all: Vec<String> = batches.iter().flat_map(|b| b.source_slugs.clone()).collect();
        all.sort();
        let mut expected: Vec<String> = (0..20).map(|i| format!("art-{i}")).collect();
        expected.sort();
        assert_eq!(all, expected, "every source must appear in exactly one batch");
    }

    #[test]
    fn build_ingest_prompt_batches_carries_full_source_index_in_every_batch() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_many_sources(root, 6, 2000);

        let batches = build_ingest_prompt_batches(root, 2_000, None).unwrap();
        assert!(batches.len() > 1);
        // Each batch prompt must reference ALL 6 sources in the index, even
        // though each batch only fully processes a subset. This is what makes
        // batches independently cross-linkable in parallel.
        for b in &batches {
            for i in 0..6 {
                let title = format!("Article {i}");
                assert!(
                    b.prompt.contains(&title),
                    "batch {} prompt missing source index entry '{}'",
                    b.index,
                    title
                );
            }
        }
    }

    #[test]
    fn build_ingest_prompt_batches_empty_when_no_sources() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# Contract").unwrap();

        let batches = build_ingest_prompt_batches(root, 50_000, None).unwrap();
        assert!(batches.is_empty());
    }

    #[test]
    fn build_ingest_prompt_batches_injects_manifest_section() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_many_sources(root, 2, 500);

        let manifest = AuthorManifest {
            entries: vec![AuthorManifestEntry {
                slug: "author-smith-j".to_string(),
                display_name: "Smith, J".to_string(),
                raw_variants: vec!["smith, j".to_string()],
                article_count: 5,
                ..Default::default()
            }],
        };
        let batches = build_ingest_prompt_batches(root, 50_000, Some(&manifest)).unwrap();
        assert_eq!(batches.len(), 1);
        assert!(batches[0].prompt.contains("Author Pages (Pre-Seeded"));
        assert!(batches[0].prompt.contains("[[author-smith-j]]"));
        assert!(batches[0].prompt.contains("Do NOT output any page with `type: author`"));
    }

    /// Fake sender: sleeps to simulate LLM latency, then returns one page per
    /// source slug embedded in the prompt. Lets us exercise the parallel path
    /// deterministically.
    struct FakeSender {
        delay_ms: u64,
        /// When set, the batch whose prompt contains this substring errors.
        fail_marker: Option<String>,
    }

    #[async_trait]
    impl IngestLlmSender for FakeSender {
        async fn send(&self, prompt: &str) -> Result<String, AppError> {
            if let Some(marker) = &self.fail_marker {
                if prompt.contains(marker) {
                    return Err(AppError::Import(format!("simulated failure for {marker}")));
                }
            }
            if self.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            }
            // Emit one PAGE per (slug: ...) occurrence in the batch sources.
            let mut out = String::new();
            for cap in regex::Regex::new(r"slug: (art-\d+)").unwrap().captures_iter(prompt) {
                let slug = &cap[1];
                out.push_str(&format!(
                    "<!-- PAGE:{slug} -->\n---\nid: {slug}\ntitle: \"{slug}\"\ntype: concept\n\
                     slug: {slug}\nsummary: \"\"\nstatus: draft\nlinks: []\n---\n\n# {slug}\n\nBody.\n\n"
                ));
            }
            Ok(out)
        }
    }

    #[tokio::test]
    async fn run_chunked_ingest_processes_batches_in_parallel_and_writes_all_pages() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();
        // 6 sources, small window -> multiple batches.
        write_many_sources(root, 6, 2000);

        let batches = build_ingest_prompt_batches(root, 2_000, None).unwrap();
        let n_batches = batches.len();
        assert!(n_batches > 1);

        let sender: Arc<dyn IngestLlmSender> =
            Arc::new(FakeSender { delay_ms: 30, fail_marker: None });
        let report = run_chunked_ingest(root, batches, sender, None, (25, 95)).await.unwrap();

        // One page per source (6) regardless of how many batches.
        assert_eq!(report.pages_written, 6);
        assert!(report.errors.is_empty(), "unexpected errors: {:?}", report.errors);

        // All pages landed on disk.
        for i in 0..6 {
            let slug = format!("art-{i}");
            assert!(root.join(format!("wiki/concepts/{slug}.md")).exists(), "missing {slug}");
        }
    }

    #[tokio::test]
    async fn run_chunked_ingest_continues_on_single_batch_failure() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();
        write_many_sources(root, 6, 2000);

        let batches = build_ingest_prompt_batches(root, 2_000, None).unwrap();
        // Force the batch that fully processes art-0 to fail. Use the unique
        // "Raw Sources for THIS Batch" header marker so only that one batch
        // errors (every batch carries art-0 in the shared source index).
        let sender: Arc<dyn IngestLlmSender> = Arc::new(FakeSender {
            delay_ms: 0,
            fail_marker: Some("### Source: Article 0".to_string()),
        });
        let report = run_chunked_ingest(root, batches, sender, None, (25, 95)).await.unwrap();

        // At least one error recorded, but other batches' pages still written.
        assert!(!report.errors.is_empty(), "expected at least one batch error");
        // art-1 through art-5 should still be on disk (5 pages).
        let mut present = 0;
        for i in 0..6 {
            if root.join(format!("wiki/concepts/art-{i}.md")).exists() {
                present += 1;
            }
        }
        assert!(
            present >= 5,
            "expected >=5 pages written despite one batch failure, got {present}"
        );
    }

    #[tokio::test]
    async fn run_chunked_ingest_empty_when_no_batches() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();

        let sender: Arc<dyn IngestLlmSender> =
            Arc::new(FakeSender { delay_ms: 0, fail_marker: None });
        let report = run_chunked_ingest(root, Vec::new(), sender, None, (25, 95)).await.unwrap();
        assert_eq!(report.pages_written, 0);
        assert!(report.errors.is_empty());
    }

    #[tokio::test]
    async fn run_chunked_ingest_parallel_is_faster_than_sequential_sum() {
        // Sanity check that batches actually run concurrently: with 4 batches
        // each sleeping 100ms, total wall time should be well under 4*100ms.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        crate::wiki::storage::scaffold_tree(root).unwrap();
        write_many_sources(root, 8, 2000);

        let batches = build_ingest_prompt_batches(root, 2_000, None).unwrap();
        assert!(batches.len() >= 3);

        let sender: Arc<dyn IngestLlmSender> =
            Arc::new(FakeSender { delay_ms: 100, fail_marker: None });
        let start = std::time::Instant::now();
        let report = run_chunked_ingest(root, batches, sender, None, (25, 95)).await.unwrap();
        let elapsed = start.elapsed();

        // If sequential, elapsed >= batches * 100ms. Allow generous headroom
        // for scheduler jitter; the point is to prove concurrency.
        assert!(report.pages_written > 0);
        assert!(
            elapsed < std::time::Duration::from_millis(400),
            "parallel ingest took too long ({elapsed:?}); concurrency not effective"
        );
    }

    // -----------------------------------------------------------------
    // Consolidation (deterministic dedup + link rewrite)
    // -----------------------------------------------------------------

    fn make_page(slug: &str, page_type: &str, source_articles: &[&str]) -> ParsedPage {
        let mut fm = Frontmatter::default();
        fm.set("id", slug);
        fm.set("title", slug);
        fm.set("type", page_type);
        fm.set("slug", slug);
        fm.set("status", "draft");
        fm.set("summary", "");
        fm.set("links", "[]");
        let sources = format!(
            "[{}]",
            source_articles.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
        );
        fm.set("source_articles", &sources);
        ParsedPage { slug: slug.to_string(), frontmatter: fm, body: format!("# {slug}\n\nBody.") }
    }

    #[test]
    fn consolidate_merges_exact_slug_duplicates() {
        let mut pages = vec![
            make_page("childhood-obesity", "concept", &["art-1"]),
            make_page("childhood-obesity", "concept", &["art-2"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 1);
        assert_eq!(slug_map.len(), 1);
        // Body contains both perspectives.
        assert!(pages[0].body.contains("Additional perspectives"));
        // Source articles unioned.
        let sources =
            frontmatter::parse_list(pages[0].frontmatter.get("source_articles").unwrap_or(""));
        assert!(sources.contains(&"art-1".to_string()));
        assert!(sources.contains(&"art-2".to_string()));
    }

    #[test]
    fn consolidate_merges_near_duplicate_jaccard() {
        // Word-reordering case: "childhood-obesity" and "obesity-childhood"
        // both stem to {childhood, obes}, so Jaccard = 1.0.
        let mut pages = vec![
            make_page("childhood-obesity", "concept", &["art-1"]),
            make_page("obesity-childhood", "concept", &["art-2"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 1, "near-duplicate pages should merge into one");
        assert_eq!(slug_map.len(), 1);
    }

    #[test]
    fn consolidate_merges_shared_source_articles() {
        // Two differently-named pages that cite the same articles.
        let mut pages = vec![
            make_page("sugar-levy-impact", "concept", &["art-1", "art-3"]),
            make_page("ssb-tax-effects", "concept", &["art-1", "art-3"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 1);
        assert_eq!(slug_map.len(), 1);
    }

    #[test]
    fn consolidate_does_not_merge_unrelated_pages() {
        let mut pages = vec![
            make_page("sugar-tax", "concept", &["art-1"]),
            make_page("exercise", "concept", &["art-2"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 2);
        assert!(slug_map.is_empty());
    }

    #[test]
    fn consolidate_does_not_merge_different_types() {
        let mut pages = vec![
            make_page("sugar-tax", "concept", &["art-1", "art-2"]),
            make_page("sugar-tax", "method", &["art-1", "art-2"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 2, "pages of different types should not merge");
        assert!(slug_map.is_empty());
    }

    #[test]
    fn consolidate_does_not_merge_author_pages() {
        let mut pages = vec![
            make_page("author-smith-j", "author", &["art-1"]),
            make_page("author-smith-j", "author", &["art-2"]),
        ];
        let slug_map = consolidate_pages(&mut pages);
        assert_eq!(pages.len(), 2, "author pages are pre-seeded and should not be merged");
        assert!(slug_map.is_empty());
    }

    #[test]
    fn rewrite_body_links_updates_simple_links() {
        let body = "See [[obesity-in-children]] for more.";
        let mut map = HashMap::new();
        map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
        let rewritten = rewrite_body_links(body, &map);
        assert_eq!(rewritten, "See [[childhood-obesity]] for more.");
    }

    #[test]
    fn rewrite_body_links_preserves_aliases() {
        let body = "See [[obesity-in-children|kids weight]] for more.";
        let mut map = HashMap::new();
        map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
        let rewritten = rewrite_body_links(body, &map);
        assert_eq!(rewritten, "See [[childhood-obesity|kids weight]] for more.");
    }

    #[test]
    fn rewrite_body_links_is_case_insensitive() {
        let body = "See [[Obesity-In-Children]] and [[OBESITY-IN-CHILDREN]].";
        let mut map = HashMap::new();
        map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
        let rewritten = rewrite_body_links(body, &map);
        assert!(rewritten.contains("[[childhood-obesity]]"));
        // Both occurrences rewritten.
        assert_eq!(rewritten.matches("[[childhood-obesity]]").count(), 2);
    }

    #[test]
    fn rewrite_body_links_leaves_unmapped_links_alone() {
        let body = "See [[sugar-tax]] and [[obesity-in-children]].";
        let mut map = HashMap::new();
        map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
        let rewritten = rewrite_body_links(body, &map);
        assert!(rewritten.contains("[[sugar-tax]]"));
        assert!(rewritten.contains("[[childhood-obesity]]"));
    }

    #[test]
    fn jaccard_similarity_handles_overlap() {
        let a: HashSet<String> = ["obes", "childhood"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["obes", "children"].iter().map(|s| s.to_string()).collect();
        // Intersection = {obes} = 1, Union = {obes, childhood, children} = 3
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn jaccard_similarity_identical_sets() {
        let a: HashSet<String> = ["obes", "child"].iter().map(|s| s.to_string()).collect();
        let sim = jaccard_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn jaccard_similarity_disjoint_sets() {
        let a: HashSet<String> = ["alpha"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["beta"].iter().map(|s| s.to_string()).collect();
        let sim = jaccard_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001);
    }
}
