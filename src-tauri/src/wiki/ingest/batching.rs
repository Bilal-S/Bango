//! Chunked/parallel ingest pipeline. Splits raw sources into batches sized to the configured
//! context window, dispatches concurrently via `JoinSet`, emits `wiki:progress` per batch
//! so the progress bar moves smoothly across 25-95%.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tauri::Emitter;

use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;
use crate::wiki::frontmatter;
use crate::wiki::raw_export;

use super::authors::AuthorManifest;
use super::consolidation::{consolidate_pages, rewrite_page_links};
use super::{parse_llm_pages, write_page, IngestReport, ParsedPage, MAX_SOURCE_CHARS};

/// Fraction of context window reserved for input. Remainder for output (wiki pages).
const INPUT_BUDGET_FRACTION: f64 = 0.4;

/// Hard cap on the number of input chars per batch, regardless of the
/// configured context window. Protects against pathological oversized calls.
pub const MAX_BATCH_INPUT_CHARS: usize = 80_000;

/// Approximate token count for a chunk of text (1 token ~= 4 chars).
#[must_use]
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Compute input char budget per batch from configured context window. Falls back to
/// `MAX_SOURCE_CHARS` when window is unusable (zero/negative). Clamped [4_000, MAX_BATCH_INPUT_CHARS].
#[must_use]
pub fn batch_input_char_budget(context_window_tokens: i32) -> usize {
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

/// Build a compact metadata-only index of ALL sources. Embedded in every batch prompt
/// so the model can `[[link]]` across batches without sequential slug-forwarding.
fn build_source_index(sources: &[RawSource]) -> String {
    let mut out = String::new();
    for s in sources {
        let slug = if s.slug.is_empty() { "unknown" } else { &s.slug };
        out.push_str(&format!("- {} [[{}]]\n", s.title, slug));
    }
    out
}

/// Build a prompt section listing external documents (Add Documents). Teaches LLM the exact
/// `[[user-slug]]` / `[^art-user-slug]` link forms. Empty string when batch has no user docs.
fn build_external_docs_section(batch_sources: &[&RawSource]) -> String {
    // Heuristic: a source is an external document when its slug starts with
    // `user-`. Article exports use UUIDs as slugs; uploaded files use
    // `user-{kebab}`.
    let user_docs: Vec<&RawSource> =
        batch_sources.iter().filter(|s| s.slug.starts_with("user-")).copied().collect();
    if user_docs.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("# External Documents (Pre-Seeded Source Pages)\n\n");
    out.push_str(
        "The following uploaded documents already have a pre-seeded wiki source page \
         (type: source) under /wiki/sources/{slug}.md. When you cite one of these, use the \
         document's slug in a [[wikilink]] or [^art-slug] footnote ref - it resolves to the \
         source page automatically. Do NOT create a duplicate source page for them:\n\n",
    );
    for doc in user_docs {
        out.push_str(&format!("- [[{}]] - {}\n", doc.slug, doc.title));
    }
    out.push_str(
        "\nUse these exact slugs when referencing the uploaded documents in your output.\n\n",
    );
    out
}

/// Build a single batch's prompt from contract, full source index, batch bodies, and instructions.
/// `methods_pre_seeded` controls the methods directive: link (don't duplicate) vs create.
fn build_batch_prompt(
    contract: &str,
    source_index: &str,
    batch_sources: &[&RawSource],
    author_manifest: Option<&AuthorManifest>,
    methods_pre_seeded: bool,
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
    let external_docs_section = build_external_docs_section(batch_sources);
    /* Methods directive: conditional on whether deterministic pre-seed wrote any method pages.
    When it did, tell LLM to link, not duplicate. When it didn't, tell LLM to create.
    Either way, focus list below asks LLM to create methods so gaps are filled. */
    let methods_directive = if methods_pre_seeded {
        "Method pages have ALSO been pre-seeded deterministically. Do NOT \
         create duplicate pages for them either - link to the existing pages."
    } else {
        "Method pages have NOT been pre-seeded for this corpus. You SHOULD \
         create method pages for research methodologies present in the sources."
    };
    format!(
        "{contract}\n\n\
         # Full Source Index (for cross-referencing)\n\n\
         The complete set of source documents in this wiki run is listed below. \
         You may create [[wikilinks]] to any of them, even if you are not asked to \
         fully process that source in this batch:\n\n\
         {source_index}\n\n\
         {manifest_section}\
         {external_docs_section}\
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
         source_articles: [\"<article-id>\"]\n\
         links: []\n\
         ---\n\
         <Markdown body with [[wikilinks]] to other pages>\n\n\
         IMPORTANT: Do NOT start the Markdown body with a `# <Title>` heading. \
         The title from the `title:` frontmatter field is rendered separately as \
         the page heading; repeating it as the first body line would display the \
         title twice on the rendered page. Start the body directly with the \
         opening prose or a `## Section` heading.\n\n\
         IMPORTANT: Author pages, synthesis pages, AND concept pages have \
         ALREADY been pre-seeded deterministically. Do NOT create duplicate \
         pages for them. Link to the existing pages instead using the slugs \
         shown in the source index and the author manifest above. \
         {methods_directive} \
         Focus your output on: \
         1. METHOD pages for research methodologies present in the sources \
         (e.g. randomized-controlled-trial, meta-analysis, systematic-review, \
         difference-in-differences). Only create pages for methods that \
         genuinely appear in the source material. \
         2. TOPICAL and THEMATIC pages that emerge from the sources. This \
         includes cross-cutting synthesis (e.g. 'Sugar Reformulation', 'Health \
         Inequalities Impact') AND section/aspect pages that a source naturally \
         covers (e.g. 'Study Population', 'Intervention Design', 'Policy \
         Context'). Use the synthesis template. Create pages for entities and \
         themes that genuinely appear in the source material. \
         3. Any NEW author pages for authors that appear only in uploaded \
         documents (see the author directive above). \
         Only create pages for entities that genuinely appear in the source \
         material. Do not invent topics to fill a quota. \
         Do NOT include raw file paths (/raw/...), file names, or source_file \
         references in your output. Use [^art-id] source references or \
         [[wikilinks]] instead. Use [[slug]] links to connect related pages \
         (you may link to sources from the Full Source Index). Each page must \
         start with the <!-- PAGE:slug --> delimiter."
    )
}

/// Split raw sources into batches sized to configured input budget. Each batch carries
/// the full source index for cross-linking, making batches independent and parallel-safe.
/// Returns empty `Vec` when no sources.
pub fn build_ingest_prompt_batches(
    root: &Path,
    context_window_tokens: i32,
    author_manifest: Option<&AuthorManifest>,
    methods_pre_seeded: bool,
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
            let prompt = build_batch_prompt(
                &contract,
                &source_index,
                &current,
                author_manifest,
                methods_pre_seeded,
            );
            let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
            batches.push((slugs, prompt));
            current.clear();
            current_len = 0;
        }
        current.push(src);
        current_len += entry_len;
    }
    if !current.is_empty() {
        let prompt = build_batch_prompt(
            &contract,
            &source_index,
            &current,
            author_manifest,
            methods_pre_seeded,
        );
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

/// Static system prompt shared by the chunked ingest path.
pub const INGEST_SYSTEM_PROMPT: &str = "You are a research knowledge-base synthesizer. Follow \
     the AGENTS.md contract strictly. Output wiki pages in the exact delimited format requested. \
     Use [[wikilinks]] to connect pages, and ALWAYS use the exact lowercase kebab-case slug of \
     the target page as the link text (e.g. [[sugar-tax]] NOT [[Sugar Tax]] or [[Sugar-Tax]]). \
     Do not use em dashes.";

/// Run chunked/parallel ingest. Single batch: writes immediately (LLM sees all sources,
/// self-consistent). Multi-batch: collects all parsed pages, runs deterministic dedup
/// + link rewrite to consolidate near-duplicates from independent batches, then writes.
///
/// `cancel_token` is polled between `join_next()` completions; on signal calls `abort_all`
///
/// (drops in-flight tasks), returns `Ok(report)` with `report.errors.push("Cancelled")`.
///
/// `progress_range` = `(start_pct, end_pct)` slice of the 0-100 pipeline bar.
pub async fn run_chunked_ingest(
    root: &Path,
    batches: Vec<IngestBatch>,
    sender: Arc<dyn IngestLlmSender>,
    app_handle: Option<&tauri::AppHandle>,
    progress_range: (usize, usize),
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<IngestReport, AppError> {
    let mut report = IngestReport::default();
    if batches.is_empty() {
        return Ok(report);
    }

    // Early cancel check: skip LLM calls entirely if token signalled during pre-seed phases.
    if cancel_token.is_some_and(|t| t.load(Ordering::SeqCst)) {
        report.errors.push("Cancelled".to_string());
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

    /* Collect results as they complete. Single-batch: write immediately.
    Multi-batch: collect all, consolidate after all batches finish.
    Cancel: poll token between join_next() completions. On signal, abort_all
    drops in-flight LLM tasks, returns early. Already-completed batches'
    pages are preserved (single-batch: on disk; multi-batch: in collected_pages
    but NOT written - consolidation + write skipped). */
    let mut collected_pages: Vec<ParsedPage> = Vec::new();
    let mut completed = 0usize;
    while let Some(res) = join_set.join_next().await {
        // Check for cancel between completions. When signalled, abort all
        // remaining tasks and return early.
        if cancel_token.is_some_and(|t| t.load(Ordering::SeqCst)) {
            join_set.abort_all();
            eprintln!("[wiki:diag] cancel detected during LLM batch; aborting remaining tasks");
            report.errors.push("Cancelled".to_string());
            // Drain any remaining join results to clean up the JoinSet.
            while join_set.join_next().await.is_some() {}
            return Ok(report);
        }
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

    /* Tier A1 grounding gate: lint pages after ingest, append ungrounded page
    errors to report. Non-fatal (pages already written) but surfaces in UI/Diagnostics.
    Author/source pages exempt (pre-seeded with different provenance shape). */
    if let Ok(lint_report) = crate::wiki::engine::lint(root) {
        let ungrounded: Vec<&crate::wiki::engine::LintIssue> = lint_report
            .issues
            .iter()
            .filter(|i| i.kind == crate::wiki::engine::LintKind::UngroundedPage)
            .collect();
        if !ungrounded.is_empty() {
            // Group by slug so the message reads cleanly: "sugar-tax (missing
            // source_articles), obesity (missing [^art-id] citations)".
            let mut by_slug: std::collections::BTreeMap<&str, Vec<&str>> =
                std::collections::BTreeMap::new();
            for issue in &ungrounded {
                let reason = if issue.severity == crate::wiki::engine::LintSeverity::Error {
                    "missing source_articles"
                } else {
                    "missing [^art-id] citations or [[wikilinks]]"
                };
                by_slug.entry(issue.slug.as_str()).or_default().push(reason);
            }
            let details: Vec<String> = by_slug
                .iter()
                .map(|(slug, reasons)| format!("{slug} ({})", reasons.join(", ")))
                .collect();
            report.errors.push(format!(
                "{} ungrounded page(s): {}",
                ungrounded.len(),
                details.join("; ")
            ));
        }
    }

    Ok(report)
}
