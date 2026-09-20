//! Deterministic wiki lint engine. Walks `wiki/**/*.md`, parses frontmatter + `[[wikilinks]]`,
//! builds a link graph, detects issues. No LLM required. Used by `wiki_lint` (Phase 4).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::wiki::frontmatter::{self, Frontmatter};

/// A single lint issue.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintIssue {
    pub page: String,
    pub slug: String,
    pub severity: LintSeverity,
    pub kind: LintKind,
    pub message: String,
}

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

/// Issue category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LintKind {
    BrokenLink,
    OrphanPage,
    DuplicateSlug,
    MissingFrontmatter,
    MissingField,
    /// LLM-generated page without provenance: no `source_articles` frontmatter or no `[^art-`
    /// citations in the body. Author/source pages are exempt (different provenance shape).
    UngroundedPage,
}

/// A full lint report.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintReport {
    pub page_count: usize,
    pub issue_count: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub issues: Vec<LintIssue>,
    /// Slugs discovered (for graph view reuse).
    pub slugs: Vec<String>,
}

/// Run the lint over the `wiki/` directory.
pub fn lint(root: &Path) -> Result<LintReport, AppError> {
    let wiki_dir = root.join("wiki");
    let mut pages: Vec<(PathBuf, Frontmatter, String)> = Vec::new();
    if wiki_dir.exists() {
        collect_pages(&wiki_dir, &mut pages)?;
    }

    let mut report = LintReport { page_count: pages.len(), ..Default::default() };

    // Build slug -> page index. Track duplicates.
    let mut slug_to_path: HashMap<String, PathBuf> = HashMap::new();
    let mut slug_counts: HashMap<String, usize> = HashMap::new();
    let mut slugs = Vec::new();
    for (path, fm, _body) in &pages {
        let slug = fm.get("slug").unwrap_or("").to_string();
        slugs.push(slug.clone());
        *slug_counts.entry(slug.clone()).or_insert(0) += 1;
        if slug.is_empty() {
            report.push(
                path,
                "",
                LintSeverity::Error,
                LintKind::MissingField,
                "missing or empty 'slug' field".to_string(),
            );
            continue;
        }
        // First occurrence wins; duplicates reported below.
        slug_to_path.entry(slug.clone()).or_insert_with(|| path.clone());
    }
    report.slugs = slug_to_path.keys().cloned().collect();
    report.slugs.sort();

    // Required frontmatter fields (matches the templates).
    let required_fields = ["id", "title", "type", "slug", "status"];

    for (path, fm, body) in &pages {
        let slug = fm.get("slug").unwrap_or("").to_string();

        // Required fields.
        for field in required_fields {
            if fm.get(field).is_none() {
                report.push(
                    path,
                    &slug,
                    LintSeverity::Error,
                    LintKind::MissingField,
                    format!("missing required frontmatter field '{field}'"),
                );
            }
        }

        // If no frontmatter at all (status absent + title absent), flag it.
        if fm.fields.is_empty() {
            report.push(
                path,
                &slug,
                LintSeverity::Error,
                LintKind::MissingFrontmatter,
                "page has no frontmatter block".to_string(),
            );
        }

        // Duplicate slugs.
        if let Some(count) = slug_counts.get(&slug) {
            if *count > 1 {
                report.push(
                    path,
                    &slug,
                    LintSeverity::Warning,
                    LintKind::DuplicateSlug,
                    format!("slug '{slug}' is used by {count} pages"),
                );
            }
        }

        /* Broken wikilinks: extract [[target]] from body, check slug set.
        Case-insensitive (Obsidian convention): [[Sugar-Reduction]] resolves to
        `sugar-reduction`. Avoids false positives when the LLM uses Title-Case. */
        let slug_set_lower: std::collections::HashSet<String> =
            slug_to_path.keys().map(|s| s.to_lowercase()).collect();
        for target in extract_wikilinks(body) {
            if !slug_set_lower.contains(&target.to_lowercase()) {
                report.push(
                    path,
                    &slug,
                    LintSeverity::Warning,
                    LintKind::BrokenLink,
                    format!("[[{target}]] points to a non-existent page"),
                );
            }
        }

        /* Tier A1 grounding gate: LLM-generated concept/method/framework/synthesis pages must
        carry provenance. Author/source pages exempt (pre-seeded, different provenance shape).
        Deterministic pre-seeds all set `source_articles`, so only LLM fabrications trip this. */
        let page_type = fm.get("type").unwrap_or("");
        let is_grounded_type =
            matches!(page_type, "concept" | "method" | "framework" | "synthesis");
        if is_grounded_type {
            let sources = frontmatter::parse_list(fm.get("source_articles").unwrap_or(""));
            if sources.is_empty() {
                report.push(
                    path,
                    &slug,
                    LintSeverity::Error,
                    LintKind::UngroundedPage,
                    "page has no source_articles provenance".to_string(),
                );
            }
            if !body.contains("[^art-") && !body.contains("[[") {
                report.push(
                    path,
                    &slug,
                    LintSeverity::Warning,
                    LintKind::UngroundedPage,
                    "page body has no [^art-id] citations or [[wikilinks]]".to_string(),
                );
            }
        }
    }

    // Orphan detection: a page with zero inbound links and not in the index.
    // Build inbound count map.
    let mut inbound: BTreeMap<String, usize> = BTreeMap::new();
    for (_path, _fm, body) in &pages {
        for target in extract_wikilinks(body) {
            *inbound.entry(target).or_insert(0) += 1;
        }
    }
    // The index.md page is exempt (it's the catalog, not a content page).
    for (path, fm, _body) in &pages {
        let slug = fm.get("slug").unwrap_or("").to_string();
        if slug.is_empty() {
            continue;
        }
        let is_index = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n == "index.md");
        if is_index {
            continue;
        }
        if inbound.get(&slug).copied().unwrap_or(0) == 0 {
            report.push(
                path,
                &slug,
                LintSeverity::Info,
                LintKind::OrphanPage,
                "page has no inbound links (orphan)".to_string(),
            );
        }
    }

    report.issue_count = report.issues.len();
    report.errors = report.issues.iter().filter(|i| i.severity == LintSeverity::Error).count();
    report.warnings = report.issues.iter().filter(|i| i.severity == LintSeverity::Warning).count();
    report.infos = report.issues.iter().filter(|i| i.severity == LintSeverity::Info).count();
    Ok(report)
}

impl LintReport {
    fn push(
        &mut self,
        path: &Path,
        slug: &str,
        severity: LintSeverity,
        kind: LintKind,
        message: String,
    ) {
        let page = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        self.issues.push(LintIssue { page, slug: slug.to_string(), severity, kind, message });
    }
}

/// A graph node (a wiki page).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub inbound: usize,
    pub outbound: usize,
    /// Page summary from frontmatter, for graph hover tooltip. Empty = "no summary".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

/// A directed graph edge (a `[[wikilink]]` from source to target).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// The full wiki link graph (nodes + edges).
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub orphan_count: usize,
}

/// Build the wiki link graph: nodes = pages, edges = `[[wikilinks]]`.
pub fn build_graph(root: &Path) -> Result<WikiGraph, AppError> {
    let wiki_dir = root.join("wiki");
    let mut pages: Vec<(PathBuf, Frontmatter, String)> = Vec::new();
    if wiki_dir.exists() {
        collect_pages(&wiki_dir, &mut pages)?;
    }

    // Build slug set + title/type/summary lookup.
    let mut slug_info: HashMap<String, (String, String, String)> = HashMap::new(); // slug -> (title, type, summary)
    let mut outbound_count: HashMap<String, usize> = HashMap::new();
    let mut inbound_count: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    for (_path, fm, body) in &pages {
        let slug = fm.get("slug").unwrap_or("").to_string();
        if slug.is_empty() {
            continue;
        }
        let title = fm.get("title").unwrap_or("").to_string();
        let page_type = fm.get("type").unwrap_or("").to_string();
        let summary = fm.get("summary").unwrap_or("").to_string();
        slug_info.insert(slug.clone(), (title, page_type, summary));

        let targets = extract_wikilinks(body);
        let out = targets.len();
        outbound_count.insert(slug.clone(), out);
        for target in targets {
            inbound_count.entry(target.clone()).and_modify(|c| *c += 1).or_insert(1);
            edges.push(GraphEdge { source: slug.clone(), target });
        }
    }

    // Build nodes (only for known slugs; edges to unknown slugs are "broken").
    let mut nodes: Vec<GraphNode> = slug_info
        .iter()
        .map(|(slug, (title, page_type, summary))| GraphNode {
            slug: slug.clone(),
            title: title.clone(),
            page_type: page_type.clone(),
            summary: summary.clone(),
            inbound: inbound_count.get(slug).copied().unwrap_or(0),
            outbound: outbound_count.get(slug).copied().unwrap_or(0),
        })
        .collect();
    nodes.sort_by(|a, b| a.slug.cmp(&b.slug));

    let orphan_count = nodes.iter().filter(|n| n.inbound == 0).count();

    Ok(WikiGraph { nodes, edges, orphan_count })
}

/// Extract `[[wikilink]]` targets from a Markdown body.
/// Supports `[[slug]]` and `[[slug|alias]]` (alias stripped).
pub fn extract_wikilinks(body: &str) -> Vec<String> {
    // Index-based scan to avoid the borrow checker conflict between
    // `chars.by_ref()` and `chars.peek()` inside the same loop.
    let bytes: Vec<char> = body.chars().collect();
    let n = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if bytes[i] == '[' && i + 1 < n && bytes[i + 1] == '[' {
            // Found opening [[. Scan for closing ]].
            let start = i + 2;
            let mut j = start;
            let mut target = String::new();
            let mut closed = false;
            let mut hit_alias = false;
            while j < n {
                if bytes[j] == '|' {
                    // Alias separator: the target so far is the slug; skip
                    // the rest until the closing ]].
                    hit_alias = true;
                    break;
                }
                if bytes[j] == ']' && j + 1 < n && bytes[j + 1] == ']' {
                    closed = true;
                    break;
                }
                target.push(bytes[j]);
                j += 1;
            }
            if closed || hit_alias {
                let t = target.trim().to_string();
                if !t.is_empty() {
                    out.push(t);
                }
                // Advance past the closing ]] (scan forward if we stopped at |).
                if hit_alias {
                    while j < n && !(bytes[j] == ']' && j + 1 < n && bytes[j + 1] == ']') {
                        j += 1;
                    }
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Recursively collect all `.md` pages under `dir` as `(path, frontmatter, body)`.
fn collect_pages(
    dir: &Path,
    out: &mut Vec<(PathBuf, Frontmatter, String)>,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_pages(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            /* Skip system audit log `wiki/log.md` - it's a bookkeeping file appended by
            ingest::finalize_ingest, not a knowledge-base page. No frontmatter → 7 spurious errors. */
            if path.file_name().and_then(|n| n.to_str()) == Some("log.md") {
                continue;
            }
            let (fm, body) = frontmatter::read_file(&path)?;
            out.push((path, fm, body));
        }
    }
    Ok(())
}
