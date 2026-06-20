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

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};
use crate::wiki::fts;
use crate::wiki::raw_export;
use tauri::Emitter;

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

    #[test]
    fn run_ingest_from_response_writes_pages_and_clears_flag() {
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
        let report = run_ingest_from_response(&conn, root, response).unwrap();
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
}
