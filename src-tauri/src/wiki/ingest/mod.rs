//! LLM-powered wiki ingest. Raw sources → LLM prompt → `<!-- PAGE:slug -->` delimited pages →
//! write to `wiki/` → append log → rebuild FTS5.
//!
//! Submodules: `batching` (chunked/parallel), `consolidation` (dedup + link rewrite),
//! `authors` (Phase 1 manifest), `synthesis` (Phase 2 pre-seed), `concepts` (Phase 3 hubs),
//! `methods` (Phase 4), `sources` (Layer 1 external docs), `slugs` (shared slug helpers).

use std::path::Path;

use rusqlite::Connection;
use tauri::Emitter;

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};
use crate::wiki::fts;

pub mod authors;
pub mod batching;
pub mod concepts;
pub mod consolidation;
pub mod methods;
pub mod slugs;
pub mod sources;
pub mod synthesis;

pub use authors::{
    build_author_manifest, build_author_manifest_from_db, preseed_authors, AuthorArticle,
    AuthorManifest, AuthorManifestEntry, CoauthorLink,
};
pub use batching::{
    build_ingest_prompt_batches, load_raw_sources, run_chunked_ingest, IngestBatch,
    IngestLlmSender, OrchestratorIngestSender, RawSource, INGEST_SYSTEM_PROMPT,
};
pub use concepts::{preseed_concept_hubs, tag_to_display_name, TAG_CONCEPT_LIMIT};
pub use consolidation::{consolidate_pages, rewrite_page_links};
pub use methods::preseed_methods;
pub use sources::preseed_document_source_pages;
pub use synthesis::preseed_synthesis_from_ai_summaries;

/// Max chars of raw source sent to LLM (~12k tokens). Conservative for context windows.
pub const MAX_SOURCE_CHARS: usize = 48_000; // ~12k tokens

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

/// A parsed page from the LLM response.
#[derive(Debug, Clone)]
pub struct ParsedPage {
    pub slug: String,
    pub frontmatter: Frontmatter,
    pub body: String,
}

/// Write wiki pages from LLM response. Does NOT touch DB; caller runs `finalize_ingest` after.
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

/// Finalize: rebuild FTS5 + manifest + dir hash, append log entry, clear staleness flag.
/// Synchronous (uses `&Connection`, no `.await`).
pub fn finalize_ingest(
    conn: &Connection,
    root: &Path,
    report: &mut IngestReport,
) -> Result<(), AppError> {
    /* Rebuild FTS5 + drift-detection manifest + dir hash so `wiki_check_for_updates`
    doesn't false-positive drift immediately after ingest. */
    if let Err(e) = fts::ensure_table(conn) {
        report.errors.push(format!("FTS table creation failed: {e}"));
    }
    if let Err(e) = fts::rebuild_index_with_manifest(conn, root) {
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

/// Parse LLM response into pages. Each delimited by `<!-- PAGE:slug -->`, containing frontmatter + body.
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
pub(super) fn write_page(root: &Path, page: &ParsedPage) -> Result<(), AppError> {
    let page_type = page.frontmatter.get("type").unwrap_or("concept");
    let subdir = match page_type {
        "author" => "authors",
        "method" => "methods",
        "synthesis" => "synthesis",
        // External-document source pages (uploaded via Add Documents). Lives
        // under `wiki/sources/` so the sidebar filter + graph can group them.
        "source" => "sources",
        _ => "concepts",
    };
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir)?;
    let filename = format!("{}.md", slugs::sanitize_slug(&page.slug));
    let path = dir.join(&filename);
    frontmatter::write_file(&path, &page.frontmatter, &page.body)?;
    Ok(())
}
