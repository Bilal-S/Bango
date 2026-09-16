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
/// 2M chars is about 500K input tokens: generous for large-window models
/// while still bounding worst-case prompt size.
pub const MAX_BATCH_INPUT_CHARS: usize = 2_000_000;

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

/// Measured wiki output demand per source article (tokens): about 158K output
/// tokens across 71 articles in the live 68-batch run. Re-measure from the
/// first summary-scale run and update (wikifix-final Change 4.4).
pub const ESTIMATED_OUTPUT_TOKENS_PER_ARTICLE: usize = 2_200;

/// Fraction of the estimated effective output budget a batch may target.
const OUTPUT_BUDGET_SAFETY: f64 = 0.7;

/// Continuation calls allowed per batch when a response truncates.
pub const MAX_CONTINUATIONS_PER_BATCH: usize = 2;

/// Two-sided sizing (wikifix-final Change 4.4): max source articles per batch
/// given the estimated effective output budget. `estimated_output_tokens = 0`
/// disables the output side (input budget only, legacy behavior). Prefers the
/// fewest calls in the 3-7 sweet spot whose per-batch expectation fits; never
/// forces 1-2 calls for corpora of 3+ sources (Change 4.7).
#[must_use]
pub fn max_articles_per_batch(n_sources: usize, estimated_output_tokens: usize) -> usize {
    if estimated_output_tokens == 0 {
        return usize::MAX;
    }
    let fit = ((OUTPUT_BUDGET_SAFETY * estimated_output_tokens as f64)
        / ESTIMATED_OUTPUT_TOKENS_PER_ARTICLE as f64)
        .floor() as usize;
    let fit = fit.max(1);
    if n_sources < 3 {
        return fit;
    }
    for calls in 3..=7 {
        let per = n_sources.div_ceil(calls);
        if per <= fit {
            return per;
        }
    }
    fit
}

/// Truncate `text` to at most `max_chars` at a word boundary, disclosing the
/// cut with a marker line (wikifix-final Change 3). Callers only invoke this
/// when the text exceeds the budget.
#[must_use]
fn truncate_at_word_boundary(text: &str, max_chars: usize) -> String {
    const MARKER: &str = "\n\n[truncated to fit the batch budget]";
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut end = max_chars.min(text.len());
    // Back off to a UTF-8 char boundary, then to the last whitespace.
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let slice = &text[..end];
    let cut = slice.rfind(char::is_whitespace).unwrap_or(end);
    format!("{}{}", &slice[..cut], MARKER)
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
    /// How many of this batch's sources were budget-truncated (Change 3);
    /// summed into `IngestReport.source_chars_truncated`.
    pub truncated_sources: usize,
    /// Run-level prompt sections (Change 4): lets `run_chunked_ingest` render
    /// continuation prompts for uncovered sources without re-reading disk.
    pub prompt_ctx: PromptContext,
    /// This batch's source entries (bodies already budget-truncated where
    /// needed), used to render continuation prompts.
    pub sources: Vec<RawSource>,
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
fn build_external_docs_section(batch_sources: &[RawSource]) -> String {
    // Heuristic: a source is an external document when its slug starts with
    // `user-`. Article exports use UUIDs as slugs; uploaded files use
    // `user-{kebab}`.
    let user_docs: Vec<&RawSource> =
        batch_sources.iter().filter(|s| s.slug.starts_with("user-")).collect();
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

/// Run-level prompt sections shared by every batch and its continuations
/// (wikifix-final Change 4): computed once per ingest run so continuation
/// calls re-render prompts without re-reading disk.
#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    /// wiki-root `AGENTS.md` contract text.
    pub contract: String,
    /// Metadata index of ALL raw sources (cross-batch linking).
    pub source_index: String,
    /// Existing Pages Index (Change 6.1): slug/type/title of every page
    /// already under `wiki/`, so regeneration reuses slugs instead of forking
    /// themes under near-duplicate slugs. Empty on a fresh scaffold.
    pub existing_pages_index: String,
    /// Author manifest section (empty when no manifest).
    pub manifest_section: String,
    /// Whether method pages were deterministically pre-seeded.
    pub methods_pre_seeded: bool,
}

/// Build the Existing Pages Index: one `- {type}: {title} [[{slug}]]` line per
/// page under `wiki/{authors,concepts,methods,frameworks,synthesis,sources}`
/// (Change 6.1 page-vocabulary pinning).
fn build_existing_pages_index(root: &Path) -> String {
    let mut out = String::new();
    for dir in ["authors", "concepts", "methods", "frameworks", "synthesis", "sources"] {
        let dir_path = root.join("wiki").join(dir);
        let Ok(entries) = std::fs::read_dir(&dir_path) else { continue };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok((fm, _)) = frontmatter::read_file(&path) else { continue };
            let slug = fm.get("slug").unwrap_or("");
            if slug.is_empty() {
                continue;
            }
            let title = fm.get("title").unwrap_or("");
            let ptype = fm.get("type").unwrap_or(dir.trim_end_matches('s'));
            out.push_str(&format!("- {ptype}: {title} [[{slug}]]\n"));
        }
    }
    out
}

impl PromptContext {
    /// Render a batch (or continuation) prompt for `sources`. Carries the
    /// soft page-count budget with depth-preserving wording (Change 4.6) and
    /// NO per-page word cap (user ruling 3).
    fn render_batch_prompt(&self, sources: &[RawSource]) -> String {
        let mut sources_text = String::new();
        for s in sources {
            let slug = if s.slug.is_empty() { "unknown" } else { &s.slug };
            sources_text.push_str(&format!(
                "### Source: {} (slug: {})\n\n{}\n\n---\n\n",
                s.title, slug, s.body
            ));
        }
        let external_docs_section = build_external_docs_section(sources);
        /* Methods directive: conditional on whether deterministic pre-seed wrote any method pages.
        When it did, tell LLM to link, not duplicate. When it didn't, tell LLM to create.
        Either way, focus list below asks LLM to create methods so gaps are filled. */
        let methods_directive = if self.methods_pre_seeded {
            "Method pages have ALSO been pre-seeded deterministically. Do NOT \
         create duplicate pages for them either - link to the existing pages."
        } else {
            "Method pages have NOT been pre-seeded for this corpus. You SHOULD \
         create method pages for research methodologies present in the sources."
        };
        /* Existing Pages section: only when the wiki already has pages, so a
        fresh scaffold's prompt stays unchanged. */
        let existing_pages_section = if self.existing_pages_index.is_empty() {
            String::new()
        } else {
            format!(
                "# Existing Wiki Pages (reuse these slugs)\n\n\
             The pages below already exist in this wiki. Link to them with [[slug]] \
             instead of creating a near-duplicate page under a different slug. \
             Create a new page ONLY for a theme, method, or framework that is \
             genuinely absent from this list:\n\n{}\n\n",
                self.existing_pages_index
            )
        };
        // Named-arg bindings for the format! template below.
        let contract = &self.contract;
        let source_index = &self.source_index;
        let manifest_section = &self.manifest_section;
        format!(
            "{contract}\n\n\
         # Full Source Index (for cross-referencing)\n\n\
         The complete set of source documents in this wiki run is listed below. \
         You may create [[wikilinks]] to any of them, even if you are not asked to \
         fully process that source in this batch:\n\n\
         {source_index}\n\n\
         {existing_pages_section}\
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
         type: concept | author | method | framework | synthesis\n\
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
         2. FRAMEWORK pages for named theoretical frameworks, models, or \
         lenses that the sources explicitly use, test, or extend (e.g. a \
         named theory like the Theory of Planned Behavior, COM-B, or \
         realist evaluation - NOT a general topic). Each framework page \
         MUST end with a '## Publications Using This Framework' section \
         listing one [[article-id|Author et al. Year]] wikilink (alias \
         form) per source article in this batch that applies it, and its \
         source_articles frontmatter must list exactly those article ids. \
         Only create pages for frameworks genuinely named in the source \
         material. \
         3. TOPICAL and THEMATIC pages that emerge from the sources. This \
         includes cross-cutting synthesis (e.g. 'Sugar Reformulation', 'Health \
         Inequalities Impact') AND section/aspect pages that a source naturally \
         covers (e.g. 'Study Population', 'Intervention Design', 'Policy \
         Context'). Use the synthesis template. Create pages for entities and \
         themes that genuinely appear in the source material. \
         4. Any NEW author pages for authors that appear only in uploaded \
         documents (see the author directive above). \
         Only create pages for entities that genuinely appear in the source \
         material. Do not invent topics to fill a quota. \
         Do NOT include raw file paths (/raw/...), file names, or source_file \
         references in your output. Use [^art-id] source references or \
         [[wikilinks]] instead. Use [[slug]] links to connect related pages \
         (you may link to sources from the Full Source Index). Each page must \
         start with the <!-- PAGE:slug --> delimiter.\n\n\
         Page guidance: create a page for every distinct theme, research \
         method, or named theoretical framework that genuinely emerges from \
         these sources - a batch of this size typically yields several pages. \
         Give each page as much depth as the material warrants; do not pad \
         pages or invent topics to inflate the count, and ground every page \
         in the sources."
        )
    }
}

/// Legacy wrapper: input-budget-only batching (output side disabled). Keeps
/// the historical 4-arg signature for existing callers/tests.
pub fn build_ingest_prompt_batches(
    root: &Path,
    context_window_tokens: i32,
    author_manifest: Option<&AuthorManifest>,
    methods_pre_seeded: bool,
) -> Result<Vec<IngestBatch>, AppError> {
    build_ingest_prompt_batches_with_budgets(
        root,
        context_window_tokens,
        author_manifest,
        methods_pre_seeded,
        0,
    )
}

/// Split raw sources into batches sized to BOTH the input budget and the
/// estimated output budget (two-sided sizing, Change 4.4). Each batch carries
/// the full source index + Existing Pages Index for cross-linking, making
/// batches independent and parallel-safe. Oversize single sources are
/// word-boundary truncated to fit (Change 3). Returns empty `Vec` when no
/// sources. `estimated_output_tokens = 0` disables the output side.
#[allow(clippy::too_many_arguments)]
pub fn build_ingest_prompt_batches_with_budgets(
    root: &Path,
    context_window_tokens: i32,
    author_manifest: Option<&AuthorManifest>,
    methods_pre_seeded: bool,
    estimated_output_tokens: usize,
) -> Result<Vec<IngestBatch>, AppError> {
    let contract = std::fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let sources = load_raw_sources(root)?;
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    let budget = batch_input_char_budget(context_window_tokens);
    let source_index = build_source_index(&sources);
    let existing_pages_index = build_existing_pages_index(root);
    let manifest_section =
        author_manifest.map(AuthorManifest::to_prompt_section).unwrap_or_default();
    let ctx = PromptContext {
        contract,
        source_index: source_index.clone(),
        existing_pages_index: existing_pages_index.clone(),
        manifest_section,
        methods_pre_seeded,
    };
    // Reserve room for the contract + indexes + instructions overhead.
    let overhead = estimate_tokens(&ctx.contract)
        + estimate_tokens(&source_index)
        + estimate_tokens(&existing_pages_index)
        + 600;
    let overhead_chars = overhead.saturating_mul(4);
    let usable_budget = budget.saturating_sub(overhead_chars).max(2_000);
    let max_articles = max_articles_per_batch(sources.len(), estimated_output_tokens);

    // Accumulate batches as (slugs, prompt, sources, truncated-count).
    let mut batches: Vec<(Vec<String>, String, Vec<RawSource>, usize)> = Vec::new();
    let mut current: Vec<RawSource> = Vec::new();
    let mut current_len: usize = 0;
    let mut current_truncated: usize = 0;

    for src in &sources {
        let slug = if src.slug.is_empty() { "unknown" } else { &src.slug };
        // Entry header + body + separator.
        let entry_overhead = slug.len() + src.title.len() + 40;
        let mut entry_src = src.clone();
        let mut entry_len = src.body.len() + entry_overhead;
        if entry_len > usable_budget {
            // Change 3: a lone oversize source is word-boundary truncated to
            // fit instead of being sent whole (the old hole allowed up to
            // 283% of budget). Reserve room for the disclosure marker.
            let available = usable_budget.saturating_sub(entry_overhead + 64).max(200);
            entry_src.body = truncate_at_word_boundary(&src.body, available);
            entry_len = entry_src.body.len() + entry_overhead;
            current_truncated += 1;
        }
        let flush_by_chars = !current.is_empty() && current_len + entry_len > usable_budget;
        let flush_by_articles = !current.is_empty() && current.len() >= max_articles;
        if flush_by_chars || flush_by_articles {
            let prompt = ctx.render_batch_prompt(&current);
            let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
            let truncated = current_truncated;
            batches.push((slugs, prompt, std::mem::take(&mut current), truncated));
            current_len = 0;
            current_truncated = 0;
        }
        current.push(entry_src);
        current_len += entry_len;
    }
    if !current.is_empty() {
        let prompt = ctx.render_batch_prompt(&current);
        let slugs: Vec<String> = current.iter().map(|s| s.slug.clone()).collect();
        let truncated = current_truncated;
        batches.push((slugs, prompt, current, truncated));
    }

    let total = batches.len();
    Ok(batches
        .into_iter()
        .enumerate()
        .map(|(i, (slugs, prompt, sources, truncated_sources))| IngestBatch {
            index: i,
            total,
            source_slugs: slugs,
            prompt,
            truncated_sources,
            prompt_ctx: ctx.clone(),
            sources,
        })
        .collect())
}

/// Injectable LLM sender for the chunked ingest. Production wraps the
/// `LlmOrchestrator`; tests provide a deterministic, latency-simulating fake.
#[async_trait]
pub trait IngestLlmSender: Send + Sync {
    /// Send one batch prompt and return the raw LLM response text.
    async fn send(&self, prompt: &str) -> Result<String, AppError>;

    /// Same as [`Self::send`], plus the provider truncation flag
    /// (`CallMeta::truncated_by_output_budget`; Change 4). Default wraps
    /// `send` (non-truncated) so test fakes keep working; the production
    /// sender overrides it via `LlmOrchestrator::send_with_meta`.
    async fn send_with_truncation(&self, prompt: &str) -> Result<(String, bool), AppError> {
        Ok((self.send(prompt).await?, false))
    }
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

    async fn send_with_truncation(&self, prompt: &str) -> Result<(String, bool), AppError> {
        let (response, _tokens, meta) = self
            .orchestrator
            .send_with_meta(&self.config, self.system_prompt, prompt, LlmRequestType::WikiIngest)
            .await?;
        Ok((response, meta.truncated_by_output_budget()))
    }
}

/// Static system prompt shared by the chunked ingest path.
pub const INGEST_SYSTEM_PROMPT: &str = "You are a research knowledge-base synthesizer. Follow \
     the AGENTS.md contract strictly. Output wiki pages in the exact delimited format requested. \
     Use [[wikilinks]] to connect pages, and ALWAYS use the exact lowercase kebab-case slug of \
     the target page as the link text (e.g. [[sugar-tax]] NOT [[Sugar Tax]] or [[Sugar-Tax]]). \
     Do not use em dashes.";

/// Run-over-run ingest metrics (Change 6.2), persisted at
/// `wiki-root/.ingest-metrics.json` and compared against the next run.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestRunMetrics {
    pub llm_batches: usize,
    pub pages_written: usize,
    pub pages_by_type: std::collections::BTreeMap<String, usize>,
    pub total_output_chars: usize,
    pub truncated_batches: usize,
    pub continuation_calls: usize,
    pub uncovered_sources: usize,
    pub timestamp: String,
}

/// Path of the metrics sidecar relative to the wiki root (outside `wiki/` so
/// the .md drift-detection hash is unaffected).
const METRICS_FILE: &str = ".ingest-metrics.json";

fn read_last_metrics(root: &Path) -> Option<IngestRunMetrics> {
    let text = std::fs::read_to_string(root.join(METRICS_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_metrics(root: &Path, metrics: &IngestRunMetrics) {
    if let Ok(json) = serde_json::to_string_pretty(metrics) {
        let _ = std::fs::write(root.join(METRICS_FILE), json);
    }
}

/// Change 6.2 regression warning: `Some` when this run wrote more than 20
/// percent fewer LLM pages than the previous run.
#[must_use]
fn regression_warning(prev: &IngestRunMetrics, current_pages: usize) -> Option<String> {
    if prev.pages_written > 0 && current_pages * 5 < prev.pages_written * 4 {
        return Some(format!(
            "LLM page output dropped from {} to {} pages vs the previous ingest run",
            prev.pages_written, current_pages
        ));
    }
    None
}

/// Article ids covered by the parsed pages' `source_articles` frontmatter.
fn covered_article_ids(pages: &[ParsedPage]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for page in pages {
        if let Some(list) = page.frontmatter.get("source_articles") {
            for id in list.split(',') {
                let id = id.trim().trim_matches(|c| c == '"' || c == '[' || c == ']').trim();
                if !id.is_empty() {
                    out.insert(id.to_string());
                }
            }
        }
    }
    out
}

/// Result of one batch task: parsed pages plus truncation/coverage telemetry.
struct BatchOutcome {
    batch_index: usize,
    pages: Vec<ParsedPage>,
    /// Provider-reported or structural truncation occurred at least once.
    truncated: bool,
    continuations: usize,
    /// Batch sources no page's `source_articles` covers after the bound.
    uncovered: Vec<String>,
}

/// Process one batch end to end (Change 4): send, parse, drop a partial
/// trailing page on truncation, then bounded continuation calls carrying only
/// the uncovered sources.
async fn process_batch(
    batch: IngestBatch,
    sender: Arc<dyn IngestLlmSender>,
) -> Result<BatchOutcome, AppError> {
    let (mut response, mut provider_truncated) = sender.send_with_truncation(&batch.prompt).await?;
    let mut pages: Vec<ParsedPage> = Vec::new();
    let mut truncated = false;
    let mut continuations = 0usize;
    let uncovered: Vec<String>;

    loop {
        let delimiter_count = response.matches("<!-- PAGE:").count();
        let mut parsed = parse_llm_pages(&response);
        /* Truncation detection: provider flag, or a structural mismatch (more
        PAGE delimiters than parseable pages = the trailing block was cut
        inside its frontmatter). A provider-truncated response drops its last
        page even when it parsed: its body may be cut mid-way. */
        let structural = delimiter_count > parsed.len();
        if provider_truncated || structural {
            truncated = true;
            eprintln!(
                "[wiki:diag] batch {} response truncated (provider={provider_truncated}, structural={structural})",
                batch.index + 1
            );
            if provider_truncated && !parsed.is_empty() {
                parsed.pop();
            }
        }
        pages.append(&mut parsed);

        // Source-coverage guard: which batch sources are still unrepresented?
        let covered = covered_article_ids(&pages);
        let missing: Vec<String> = batch
            .source_slugs
            .iter()
            .filter(|slug| !covered.contains(slug.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() || continuations >= MAX_CONTINUATIONS_PER_BATCH {
            uncovered = missing;
            break;
        }
        // Bounded continuation: re-dispatch ONLY the uncovered sources.
        continuations += 1;
        let subset: Vec<RawSource> =
            batch.sources.iter().filter(|s| missing.contains(&s.slug)).cloned().collect();
        if subset.is_empty() {
            uncovered = missing;
            break;
        }
        let prompt = batch.prompt_ctx.render_batch_prompt(&subset);
        let (resp, trunc) = sender.send_with_truncation(&prompt).await?;
        response = resp;
        provider_truncated = trunc;
    }

    Ok(BatchOutcome { batch_index: batch.index, pages, truncated, continuations, uncovered })
}

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

    // Change 3 telemetry: any budget-truncated source surfaces in the report.
    report.source_chars_truncated = batches.iter().any(|b| b.truncated_sources > 0);
    // Change 6.2: read the previous run's metrics before this run overwrites.
    let prev_metrics = read_last_metrics(root);

    // Spawn one task per batch. Each task sends its prompt, parses pages,
    // detects truncation, and runs bounded continuations for uncovered
    // sources (`process_batch` keeps the main loop tight).
    let mut join_set: tokio::task::JoinSet<Result<BatchOutcome, AppError>> =
        tokio::task::JoinSet::new();
    for batch in batches {
        let sender = Arc::clone(&sender);
        join_set.spawn(process_batch(batch, sender));
    }

    /* Collect results as they complete. Single-batch: write immediately.
    Multi-batch: collect all, consolidate after all batches finish.
    Cancel: poll token between join_next() completions. On signal, abort_all
    drops in-flight LLM tasks, returns early. Already-completed batches'
    pages are preserved (single-batch: on disk; multi-batch: in collected_pages
    but NOT written - consolidation + write skipped). */
    let mut collected_pages: Vec<ParsedPage> = Vec::new();
    // (type, body-chars) telemetry for single-batch runs; multi-batch derives
    // it from `collected_pages` after consolidation.
    let mut page_meta: Vec<(String, usize)> = Vec::new();
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
            Ok(Ok(outcome)) => {
                let batch_index = outcome.batch_index;
                let page_count = outcome.pages.len();
                if outcome.truncated {
                    report.truncated_batches += 1;
                }
                report.continuation_calls += outcome.continuations;
                if !outcome.uncovered.is_empty() {
                    report.uncovered_sources.extend(outcome.uncovered.iter().cloned());
                    report.errors.push(format!(
                        "Batch {}: {} source(s) not covered by any page after \
                         {MAX_CONTINUATIONS_PER_BATCH} continuations: {}",
                        batch_index + 1,
                        outcome.uncovered.len(),
                        outcome.uncovered.join(", ")
                    ));
                }
                // Never silently accept a 0-page parse: an empty or
                // non-delimited LLM response must surface as an error so the
                // report/log discloses that no pages were generated (stale
                // pages otherwise stay on disk unnoticed).
                if page_count == 0 {
                    report.errors.push(format!(
                        "Batch {} returned 0 parseable pages (response empty or missing <!-- PAGE:slug --> delimiters)",
                        batch_index + 1
                    ));
                }
                report.pages_written += page_count;
                if total_batches > 1 {
                    // Multi-batch: defer writing until consolidation.
                    collected_pages.extend(outcome.pages);
                } else {
                    // Single-batch: write immediately.
                    for page in &outcome.pages {
                        if let Err(e) = write_page(root, page) {
                            report
                                .errors
                                .push(format!("Failed to write page {}: {}", page.slug, e));
                        }
                    }
                    page_meta.extend(outcome.pages.iter().map(|p| {
                        (p.frontmatter.get("type").unwrap_or("concept").to_string(), p.body.len())
                    }));
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

    /* Change 6.2 telemetry: record volume metrics for this run, compare with
    the previous run, and surface a regression warning (non-fatal). */
    if total_batches > 1 {
        page_meta = collected_pages
            .iter()
            .map(|p| (p.frontmatter.get("type").unwrap_or("concept").to_string(), p.body.len()))
            .collect();
    }
    let mut pages_by_type: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (ptype, _) in &page_meta {
        *pages_by_type.entry(ptype.clone()).or_default() += 1;
    }
    let metrics = IngestRunMetrics {
        llm_batches: total_batches,
        pages_written: report.pages_written,
        pages_by_type,
        total_output_chars: page_meta.iter().map(|(_, chars)| chars).sum(),
        truncated_batches: report.truncated_batches,
        continuation_calls: report.continuation_calls,
        uncovered_sources: report.uncovered_sources.len(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    if let Some(prev) = prev_metrics {
        if let Some(warning) = regression_warning(&prev, report.pages_written) {
            report.warnings.push(warning);
        }
    }
    write_metrics(root, &metrics);

    Ok(report)
}
