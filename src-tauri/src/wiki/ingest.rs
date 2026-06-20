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
// Chunked / parallel ingest
// ---------------------------------------------------------------------------

/// Fraction of the configured context window reserved for the input prompt.
/// The remainder is available for the model's output (the wiki pages).
const INPUT_BUDGET_FRACTION: f64 = 0.4;

/// Hard cap on the number of input chars per batch, regardless of the
/// configured context window. Protects against pathological oversized calls.
const MAX_BATCH_INPUT_CHARS: usize = 80_000;

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
/// process — this is what keeps batches independent and parallelizable.
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
fn build_batch_prompt(contract: &str, source_index: &str, batch_sources: &[&RawSource]) -> String {
    let mut sources_text = String::new();
    for s in batch_sources {
        let slug = if s.slug.is_empty() { "unknown" } else { &s.slug };
        sources_text.push_str(&format!(
            "### Source: {} (slug: {})\n\n{}\n\n---\n\n",
            s.title, slug, s.body
        ));
    }
    format!(
        "{contract}\n\n\
         # Full Source Index (for cross-referencing)\n\n\
         The complete set of source documents in this wiki run is listed below. \
         You may create [[wikilinks]] to any of them, even if you are not asked to \
         fully process that source in this batch:\n\n\
         {source_index}\n\n\
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
            let prompt = build_batch_prompt(&contract, &source_index, &current);
            let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
            batches.push((slugs, prompt));
            current.clear();
            current_len = 0;
        }
        current.push(src);
        current_len += entry_len;
    }
    if !current.is_empty() {
        let prompt = build_batch_prompt(&contract, &source_index, &current);
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
/// - Dispatches all batches concurrently via a `tokio::task::JoinSet`. The
///   orchestrator's semaphore bounds actual in-flight LLM calls
///   (`max_concurrent_requests`), so no extra concurrency knob is needed.
/// - Collects results as they complete (`join_next`), writing each batch's
///   pages to disk immediately and emitting `wiki:progress` on every completion
///   (so the progress bar moves smoothly regardless of batch finish order).
/// - Tolerates per-batch failures: a failed batch is recorded in
///   `report.errors`; the remaining batches still write their pages.
///
/// `progress_range` is `(start_pct, end_pct)` — the slice of the 0–100 pipeline
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

    // Collect as they complete. Track how many have finished so the progress
    // bar advances monotonically regardless of completion order.
    let mut completed = 0usize;
    while let Some(res) = join_set.join_next().await {
        completed += 1;
        match res {
            Ok(Ok((batch_index, pages))) => {
                report.pages_written += pages.len();
                for page in &pages {
                    if let Err(e) = write_page(root, page) {
                        report.errors.push(format!("Failed to write page {}: {}", page.slug, e));
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
                                pages.len()
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

    Ok(report)
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

        let batches = build_ingest_prompt_batches(root, 50_000).unwrap();
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

        let batches = build_ingest_prompt_batches(root, 2_000).unwrap();
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

        let batches = build_ingest_prompt_batches(root, 2_000).unwrap();
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

        let batches = build_ingest_prompt_batches(root, 50_000).unwrap();
        assert!(batches.is_empty());
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

        let batches = build_ingest_prompt_batches(root, 2_000).unwrap();
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

        let batches = build_ingest_prompt_batches(root, 2_000).unwrap();
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

        let batches = build_ingest_prompt_batches(root, 2_000).unwrap();
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
}
