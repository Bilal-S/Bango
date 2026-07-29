//! LLM-powered wiki ingest.
//!
//! Takes the raw sources (`raw/*.md`) and asks the LLM to synthesize them into
//! wiki pages (concepts, authors, methods, synthesis) with `[[wikilinks]]` and
//! `summary` frontmatter. Writes the generated pages to `wiki/`, appends a log
//! entry, and rebuilds the FTS5 index.
//!
//! Workflow:
//! 1. `process_user_files` - ensure `raw/` is uniform `.md`.
//! 2. Read the `AGENTS.md` contract + concatenate raw sources.
//! 3. Build a prompt instructing the LLM to output pages in a parseable format.
//! 4. Parse the LLM response into pages (delimited by `<!-- PAGE:slug -->`).
//! 5. Write pages to `wiki/{type}/`.
//! 6. Append a run entry to `wiki/log.md`.
//! 7. Rebuild the FTS5 index.
//!
//! ## Module layout
//!
//! This is a directory module. The concerns are split across focused submodules:
//! - [`mod`] (this file): core types (`IngestReport`, `ParsedPage`), the core
//!   pipeline (`write_pages_from_response`, `finalize_ingest`,
//!   `parse_llm_pages`, `write_page`), and re-exports.
//! - [`batching`]: chunked/parallel batch building + the LLM sender trait +
//!   `run_chunked_ingest`.
//! - [`consolidation`]: deterministic dedup + `[[wikilink]]` rewrite
//!   (multi-batch only).
//! - [`authors`]: Phase 1 author manifest + pre-seed.
//! - [`synthesis`]: Phase 2 synthesis pre-seed from AI summaries.
//! - [`concepts`]: Phase 3 concept hub pre-seed from top user-curated tags
//!   (top-40 by included-article count) + `biblio_terms` (top-N by frequency),
//!   slug-deduped so tags win on collisions.
//! - [`methods`]: Phase 4 method hub pre-seed from AI-summary `study_design`
//!   with a `biblio_terms` fallback (abstracts-only corpora).
//! - [`sources`]: Layer 1 external-document source page pre-seed.
//! - [`slugs`]: shared slug squeezing utilities (dedupes the former
//!   `author_slug` / `concept_slug` near-duplicate logic).

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

/// The maximum number of characters of raw source to send to the LLM.
/// Conservative to stay within typical context windows.
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

/// Run the full ingest pipeline. Requires an LLM response string (the caller
/// obtains it via the orchestrator; this function is split for testability).
/// Write wiki pages from the LLM response (async, with per-page progress).
/// Does NOT touch the DB connection - caller must call `finalize_ingest` afterwards.
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
    // Rebuild the FTS5 index + the drift-detection manifest + dir hash in one
    // shot so the on-demand `wiki_check_for_updates` doesn't false-positive a
    // drift immediately after an ingest.
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
