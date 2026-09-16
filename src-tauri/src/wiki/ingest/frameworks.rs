//! Framework page pre-seed (6th deterministic layer) + LLM synthesis pass.
//!
//! Root cause (2026-09-16): wiki input switched to AI summaries (Change 1),
//! which drop named-theory mentions, so the LLM ingest could no longer ground
//! framework pages (three fresh runs produced zero). Fix: extract
//! `theoretical_frameworks` into the summary blob, canonicalize names across
//! articles, pre-seed one page per canonical framework with complete
//! provenance, then optionally polish the body with one LLM call per
//! framework that sees every naming article (each paper alone may carry only
//! a partial explanation).

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use rusqlite::Connection;

use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::models::llm_config::LlmConfig;
use crate::wiki::frontmatter::{self, Frontmatter};

use super::slugs::framework_slug;
use super::synthesis::parse_ai_summary;

/// System prompt for the per-article backfill: extract named frameworks.
pub const FRAMEWORK_EXTRACTION_SYSTEM_PROMPT: &str = "You extract named theoretical \
     frameworks from a research paper. Respond with JSON ONLY: \
     {\"theoretical_frameworks\": [{\"name\": \"...\", \"usage\": \"...\"}]}. `name` is the \
     framework's established name (prefer the canonical published name, e.g. \"Theory of \
     Planned Behavior\" over \"TPB\"). `usage` is 1-2 sentences on how THIS paper applies, \
     tests, or extends it. Include ONLY theories/models/lenses the paper explicitly names \
     and uses - not general topics or the paper's own novel results. Use an empty list \
     when the paper names none. No code fences.";

/// System prompt for the per-framework body polish call.
pub const FRAMEWORK_SYNTHESIS_SYSTEM_PROMPT: &str = "You write wiki pages about theoretical \
     frameworks for a research knowledge base. Produce Markdown body ONLY (no page title \
     heading, no frontmatter): an opening paragraph defining the framework, a \
     '## Core Tenets' bullet list, and a '## Usage in This Review' section synthesizing how \
     the listed studies use it (cite each with its [^art-id]). Ground every claim in the \
     provided material; do not invent. Use plain hyphens, never em dashes.";

/// One article naming a framework (provenance + how it is used).
#[derive(Debug)]
pub struct FrameworkArticle {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// First-author display label ("Rogers et al." / "Rogers").
    pub author_label: String,
    /// Per-article usage note captured at extraction time.
    pub usage: Option<String>,
}

/// One canonical framework: display name, slug, every naming article.
#[derive(Debug)]
pub struct FrameworkRow {
    pub name: String,
    pub slug: String,
    pub articles: Vec<FrameworkArticle>,
}

/// Deterministic canonicalization (Stage 2.1): cluster raw names by slug; the
/// most frequent raw variant becomes the canonical display name. Returns
/// raw-name -> canonical-name. Acronym aliases need the LLM merge on top.
#[must_use]
pub fn canonical_name_map(raw_names: &[String]) -> HashMap<String, String> {
    let mut clusters: HashMap<String, Vec<String>> = HashMap::new();
    for name in raw_names {
        let trimmed = name.trim().to_string();
        let slug = framework_slug(&trimmed);
        if slug.is_empty() || slug == "framework-unnamed" {
            continue;
        }
        clusters.entry(slug).or_default().push(trimmed);
    }
    let mut map = HashMap::new();
    for (_, variants) in clusters {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for v in &variants {
            *counts.entry(v.as_str()).or_default() += 1;
        }
        let Some((canonical, _)) = counts.iter().max_by_key(|(name, count)| (**count, name.len()))
        else {
            continue;
        };
        let canonical = (*canonical).to_string();
        for v in variants {
            map.insert(v, canonical.clone());
        }
    }
    map
}

/// Apply corpus-level alias merges (Stage 2.2, from one LLM call): every
/// canonical name listed in `absorbed` is remapped onto `canonical`. Pure.
#[must_use]
pub fn apply_alias_merges(
    map: &HashMap<String, String>,
    merges: &[(String, Vec<String>)],
) -> HashMap<String, String> {
    let mut out = map.clone();
    for (canonical, absorbed) in merges {
        for raw in map.keys() {
            if absorbed.contains(map.get(raw).unwrap_or(raw)) {
                out.insert(raw.clone(), canonical.clone());
            }
        }
    }
    out
}

/// First-author label for the Publications list: "Rogers et al." when the
/// article has co-authors, else the family name; title words as fallback.
#[must_use]
fn first_author_label(authors: Option<&str>, title: &str) -> String {
    let raw = authors.unwrap_or("").trim();
    let list: Vec<String> = serde_json::from_str(raw).unwrap_or_else(|_| {
        raw.split(';').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
    });
    let Some(first) = list.first() else { return short_title(title) };
    let family = first.split(',').next().unwrap_or(first).trim();
    if family.is_empty() {
        return short_title(title);
    }
    if list.len() > 1 {
        format!("{family} et al.")
    } else {
        family.to_string()
    }
}

/// First few title words, used when no author metadata exists.
#[must_use]
fn short_title(title: &str) -> String {
    title.split_whitespace().take(5).collect::<Vec<_>>().join(" ")
}

/// Fetch + canonicalize framework rows from the included corpus' blobs.
/// Groups every naming article under one canonical framework.
pub fn fetch_framework_rows(conn: &Connection) -> Result<Vec<FrameworkRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, publication_year, authors, full_text_ai_summary \
         FROM articles \
         WHERE status = 'included' \
           AND full_text_ai_summary IS NOT NULL AND full_text_ai_summary != ''",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i32>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut all_names: Vec<String> = Vec::new();
    let mut entries: Vec<(String, FrameworkArticle)> = Vec::new();
    for (id, title, year, authors, blob) in rows.filter_map(Result::ok) {
        let Some(parsed) = parse_ai_summary(&blob) else { continue };
        for fw in &parsed.theoretical_frameworks {
            all_names.push(fw.name.clone());
            entries.push((
                framework_slug(&fw.name),
                FrameworkArticle {
                    id: id.clone(),
                    title: title.clone(),
                    year,
                    author_label: first_author_label(authors.as_deref(), &title),
                    usage: fw.usage.clone(),
                },
            ));
        }
    }
    let map = canonical_name_map(&all_names);
    let mut by_slug: HashMap<String, FrameworkRow> = HashMap::new();
    for (slug, article) in entries {
        let canonical = map
            .iter()
            .find(|(raw, _)| framework_slug(raw) == slug)
            .map(|(_, canonical)| canonical.clone())
            .unwrap_or_else(|| slug.clone());
        by_slug
            .entry(slug.clone())
            .or_insert_with(|| FrameworkRow { name: canonical, slug, articles: Vec::new() })
            .articles
            .push(article);
    }
    let mut rows: Vec<FrameworkRow> = by_slug.into_values().collect();
    rows.sort_by(|a, b| b.articles.len().cmp(&a.articles.len()).then_with(|| a.name.cmp(&b.name)));
    Ok(rows)
}

/// Injectable framework-body synthesizer (production wraps the orchestrator;
/// tests inject fakes). One call per framework, seeing every naming article.
#[async_trait]
pub trait FrameworkSynthesizer: Send + Sync {
    async fn synthesize(&self, row: &FrameworkRow) -> Result<String, AppError>;
}

/// Production synthesizer: one orchestrator call per framework.
pub struct OrchestratorFrameworkSynthesizer {
    pub orchestrator: std::sync::Arc<LlmOrchestrator>,
    pub config: LlmConfig,
}

#[async_trait]
impl FrameworkSynthesizer for OrchestratorFrameworkSynthesizer {
    async fn synthesize(&self, row: &FrameworkRow) -> Result<String, AppError> {
        let mut usage = String::new();
        for a in &row.articles {
            let year = a.year.map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
            let note = a.usage.as_deref().unwrap_or("no usage note captured").trim();
            usage.push_str(&format!("- \"{}\" ({}), id {}: {}\n", a.title, year, a.id, note));
        }
        let user_prompt = format!(
            "Framework: {}\n\nArticles in this review that name and use it:\n\n{usage}\n\
             Write the page body now. Cite each article inline as [^art-<id>] using the ids above.",
            row.name
        );
        let (body, _tokens) = self
            .orchestrator
            .send(
                &self.config,
                FRAMEWORK_SYNTHESIS_SYSTEM_PROMPT,
                &user_prompt,
                LlmRequestType::WikiIngest,
            )
            .await?;
        if body.trim().is_empty() {
            return Err(AppError::Import("empty framework synthesis".to_string()));
        }
        Ok(body)
    }
}

/// Deterministic skeleton body (fallback when the LLM is off or fails).
#[must_use]
fn skeleton_body(row: &FrameworkRow) -> String {
    let mut out = format!(
        "{} is a theoretical framework named by {} article(s) in this review.\n",
        row.name,
        row.articles.len()
    );
    let with_usage: Vec<&FrameworkArticle> = row
        .articles
        .iter()
        .filter(|a| a.usage.as_deref().is_some_and(|u| !u.trim().is_empty()))
        .collect();
    if !with_usage.is_empty() {
        out.push_str("\n## Usage in This Review\n\n");
        for a in with_usage {
            out.push_str(&format!(
                "- **{}**: {} [^art-{}]\n",
                a.title,
                a.usage.clone().unwrap_or_default(),
                a.id
            ));
        }
    }
    out
}

/// Sync skeleton pass: write deterministic framework pages (no LLM). Used
/// inside the DB-locked pre-seed scope; the async polish pass runs after.
pub fn preseed_framework_pages(rows: &[FrameworkRow], root: &Path) -> Result<usize, AppError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let dir = root.join("wiki").join("frameworks");
    std::fs::create_dir_all(&dir)?;
    let mut written = 0;
    for row in rows {
        let path = dir.join(format!("{}.md", row.slug));
        if is_reviewed(&path) {
            continue;
        }
        write_framework_page(&path, row, &skeleton_body(row))?;
        written += 1;
    }
    Ok(written)
}

/// Async polish pass: rewrite each framework page's body with one LLM
/// synthesis call that sees every naming article. Skeleton stays in place on
/// failure (non-fatal per page). Runs OUTSIDE the DB lock at the call sites
/// so no `MutexGuard` crosses an await.
pub async fn polish_framework_pages(
    rows: Vec<FrameworkRow>,
    root: &Path,
    orchestrator: &std::sync::Arc<LlmOrchestrator>,
    config: &LlmConfig,
) -> Result<usize, AppError> {
    let synthesizer = OrchestratorFrameworkSynthesizer {
        orchestrator: std::sync::Arc::clone(orchestrator),
        config: config.clone(),
    };
    let synth: &dyn FrameworkSynthesizer = &synthesizer;
    let dir = root.join("wiki").join("frameworks");
    std::fs::create_dir_all(&dir)?;
    let mut polished = 0;
    for row in rows {
        let path = dir.join(format!("{}.md", row.slug));
        if is_reviewed(&path) {
            continue;
        }
        match synth.synthesize(&row).await {
            Ok(body) if !body.trim().is_empty() => {
                write_framework_page(&path, &row, &body)?;
                polished += 1;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[wiki:diag] framework synthesis failed for {}: {e}", row.slug);
            }
        }
    }
    Ok(polished)
}

/// True when the page exists and is user-reviewed (never overwrite).
fn is_reviewed(path: &Path) -> bool {
    matches!(
        frontmatter::read_file(path),
        Ok((ref fm, _)) if fm.get("status") == Some("reviewed")
    )
}

/// Compose + write one framework page: deterministic frontmatter + body +
/// complete Publications section (provenance never LLM-truncated).
fn write_framework_page(path: &Path, row: &FrameworkRow, body: &str) -> Result<(), AppError> {
    let mut fm = Frontmatter::default();
    fm.set("id", &row.slug);
    fm.set("title", &row.name);
    fm.set("type", "framework");
    fm.set("slug", &row.slug);
    fm.set(
        "summary",
        &format!("{}: theoretical framework named by {} article(s).", row.name, row.articles.len()),
    );
    fm.set("status", "draft");
    let ids: Vec<String> = row.articles.iter().map(|a| format!("\"{}\"", a.id)).collect();
    fm.set("source_articles", &format!("[{}]", ids.join(", ")));
    fm.set("links", "[]");
    let mut full_body = body.trim_end().to_string();
    full_body.push_str("\n\n## Publications Using This Framework\n\n");
    for a in &row.articles {
        let year = a.year.map(|y| y.to_string()).unwrap_or_else(|| "n.d.".to_string());
        full_body.push_str(&format!("- [[{}|{} {}]]\n", a.id, a.author_label, year));
    }
    frontmatter::write_file(path, &fm, &full_body)
}

/// Pre-seed `wiki/frameworks/{slug}.md` from pre-fetched rows: deterministic
/// frontmatter + complete Publications section; the body is the LLM synthesis
/// when available, else the deterministic skeleton. Reviewed pages preserved.
/// Takes owned `FrameworkRow`s (not `&Connection`) so no DB reference crosses
/// an await point (tauri command futures must be `Send`).
/// Returns pages written.
pub async fn preseed_frameworks(
    rows: Vec<FrameworkRow>,
    root: &Path,
    synthesizer: Option<&dyn FrameworkSynthesizer>,
) -> Result<usize, AppError> {
    let written = preseed_framework_pages(&rows, root)?;
    if let Some(synth) = synthesizer {
        let dir = root.join("wiki").join("frameworks");
        for row in &rows {
            let path = dir.join(format!("{}.md", row.slug));
            if is_reviewed(&path) {
                continue;
            }
            match synth.synthesize(row).await {
                Ok(body) if !body.trim().is_empty() => {
                    write_framework_page(&path, row, &body)?;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[wiki:diag] framework synthesis failed for {}: {e}", row.slug);
                }
            }
        }
    }
    Ok(written)
}
