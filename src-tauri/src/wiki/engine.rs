//! Deterministic wiki lint engine.
//!
//! Walks `wiki/**/*.md`, parses frontmatter + `[[wikilinks]]`, builds a link
//! graph, and detects issues. No LLM required. Used by the `wiki_lint`
//! command (Phase 4) and to support orphan-companion detection for
//! user-added raw files (Phase 2B follow-up).

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
    StalePage,
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

        // Broken wikilinks: extract [[target]] from body, check slug set.
        // Comparison is case-insensitive (Obsidian convention): a link like
        // `[[Sugar-Reduction]]` resolves to a page whose slug is
        // `sugar-reduction`. This avoids false-positive broken links when the
        // LLM emits Title-Cased wikilinks.
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

    // Build slug set + title/type lookup.
    let mut slug_info: HashMap<String, (String, String)> = HashMap::new(); // slug -> (title, type)
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
        slug_info.insert(slug.clone(), (title, page_type));

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
        .map(|(slug, (title, page_type))| GraphNode {
            slug: slug.clone(),
            title: title.clone(),
            page_type: page_type.clone(),
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
            // Skip the system audit log (wiki/log.md). It is a bookkeeping file
            // appended by `ingest::finalize_ingest`, not a knowledge-base page;
            // it has no frontmatter, so linting it produces 7 spurious errors.
            if path.file_name().and_then(|n| n.to_str()) == Some("log.md") {
                continue;
            }
            let (fm, body) = frontmatter::read_file(&path)?;
            out.push((path, fm, body));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_page(root: &Path, subdir: &str, slug: &str, title: &str, body: &str) {
        let dir = root.join("wiki").join(subdir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut fm = Frontmatter::default();
        fm.set("id", slug);
        fm.set("title", title);
        fm.set("type", "concept");
        fm.set("slug", slug);
        fm.set("status", "draft");
        fm.set("links", "[]");
        frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
    }

    fn write_page_no_fm(root: &Path, subdir: &str, name: &str, body: &str) {
        let dir = root.join("wiki").join(subdir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.md")), body).unwrap();
    }

    #[test]
    fn clean_wiki_has_no_errors() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha\nSee [[beta]] for more.");
        write_page(root, "concepts", "beta", "Beta", "# Beta\nSee [[alpha]].");

        let report = lint(root).unwrap();
        assert_eq!(report.page_count, 2);
        assert_eq!(report.errors, 0);
        // No broken links, no missing fields, no duplicates.
        assert!(report.issues.iter().all(|i| i.severity != LintSeverity::Error));
    }

    #[test]
    fn log_md_is_exempt_from_lint() {
        // The system audit log (wiki/log.md) has no frontmatter; it must be
        // skipped entirely so it does not produce 7 spurious errors.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha");
        // Write a log.md with no frontmatter (mirrors ingest::finalize_ingest).
        std::fs::write(
            root.join("wiki/log.md"),
            "# Wiki Audit Log\n\nAppend-only record of ingest and lint runs.\n",
        )
        .unwrap();

        let report = lint(root).unwrap();
        // log.md is not counted as a page.
        assert_eq!(report.page_count, 1, "log.md should not be counted as a page");
        // No errors at all (log.md's missing fields are not flagged).
        assert_eq!(report.errors, 0, "log.md should not produce errors");
        // And no issue mentions log.md.
        assert!(
            !report.issues.iter().any(|i| i.page == "log.md"),
            "log.md should not appear in any issue"
        );
    }

    #[test]
    fn broken_link_check_is_case_insensitive() {
        // A Title-Cased wikilink like [[Sugar-Reduction]] should resolve to a
        // page whose slug is `sugar-reduction` (Obsidian convention), so it is
        // NOT flagged as broken.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(
            root,
            "concepts",
            "sugar-reduction",
            "Sugar Reduction",
            "# Sugar Reduction
See [[Sugar-Reduction]] and [[OBESITY]].",
        );
        // `obesity` page exists (lowercase slug); the link [[OBESITY]] should resolve.
        write_page(root, "concepts", "obesity", "Obesity", "# Obesity");

        let report = lint(root).unwrap();
        // No broken-link warnings: both [[Sugar-Reduction]] and [[OBESITY]] resolve.
        let broken: Vec<_> =
            report.issues.iter().filter(|i| i.kind == LintKind::BrokenLink).collect();
        assert!(
            broken.is_empty(),
            "case-insensitive links should not be flagged broken: {broken:?}"
        );

        // Sanity: a genuinely missing target IS still flagged.
        write_page(
            root,
            "concepts",
            "gamma",
            "Gamma",
            "# Gamma
See [[nonexistent]].",
        );
        let report2 = lint(root).unwrap();
        assert!(report2.issues.iter().any(|i| i.kind == LintKind::BrokenLink));
    }

    #[test]
    fn detects_broken_link() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha\nLinks to [[nonexistent]].");

        let report = lint(root).unwrap();
        assert!(report.issues.iter().any(|i| i.kind == LintKind::BrokenLink && i.slug == "alpha"));
    }

    #[test]
    fn detects_orphan_page() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // beta links to alpha; gamma is orphaned.
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha");
        write_page(root, "concepts", "beta", "Beta", "# Beta\nSee [[alpha]].");
        write_page(root, "concepts", "gamma", "Gamma", "# Gamma (orphan)");

        let report = lint(root).unwrap();
        assert!(report.issues.iter().any(|i| i.kind == LintKind::OrphanPage && i.slug == "gamma"));
        assert!(!report.issues.iter().any(|i| i.kind == LintKind::OrphanPage && i.slug == "alpha"));
    }

    #[test]
    fn detects_missing_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page_no_fm(root, "concepts", "bare", "# Bare\nNo frontmatter here.");

        let report = lint(root).unwrap();
        assert!(report.issues.iter().any(|i| i.kind == LintKind::MissingFrontmatter));
    }

    #[test]
    fn detects_missing_required_field() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Write a page missing the 'type' field.
        let dir = root.join("wiki/concepts");
        std::fs::create_dir_all(&dir).unwrap();
        let mut fm = Frontmatter::default();
        fm.set("id", "alpha");
        fm.set("title", "Alpha");
        // 'type' intentionally omitted
        fm.set("slug", "alpha");
        fm.set("status", "draft");
        frontmatter::write_file(&dir.join("alpha.md"), &fm, "# Alpha").unwrap();

        let report = lint(root).unwrap();
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == LintKind::MissingField && i.message.contains("'type'")));
    }

    #[test]
    fn detects_duplicate_slug() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "dup", "Dup One", "# One");
        write_page(root, "synthesis", "dup", "Dup Two", "# Two");

        let report = lint(root).unwrap();
        let dups: Vec<_> =
            report.issues.iter().filter(|i| i.kind == LintKind::DuplicateSlug).collect();
        assert_eq!(dups.len(), 2);
    }

    #[test]
    fn wikilink_with_alias_is_parsed() {
        let targets = extract_wikilinks("see [[sugar-tax|the levy]] and [[obesity]]");
        assert_eq!(targets, vec!["sugar-tax".to_string(), "obesity".to_string()]);
    }

    #[test]
    fn wikilink_extraction_ignores_single_brackets() {
        let targets = extract_wikilinks("[not a link] and [[real]]");
        assert_eq!(targets, vec!["real".to_string()]);
    }

    #[test]
    fn index_page_exempt_from_orphan_check() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // index.md with no inbound links should NOT be flagged orphan.
        let mut fm = Frontmatter::default();
        fm.set("id", "index");
        fm.set("title", "Index");
        fm.set("type", "synthesis");
        fm.set("slug", "index");
        fm.set("status", "draft");
        frontmatter::write_file(&root.join("wiki/index.md"), &fm, "# Index\n- [[alpha]]").unwrap();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha");

        let report = lint(root).unwrap();
        assert!(!report
            .issues
            .iter()
            .any(|i| i.kind == LintKind::OrphanPage && i.page == "index.md"));
    }

    #[test]
    fn empty_wiki_dir_returns_empty_report() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let report = lint(root).unwrap();
        assert_eq!(report.page_count, 0);
        assert!(report.issues.is_empty());
    }

    // ---- build_graph ----

    #[test]
    fn graph_builds_nodes_and_edges() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha\nSee [[beta]].");
        write_page(root, "concepts", "beta", "Beta", "# Beta\nSee [[alpha]] and [[gamma]].");
        write_page(root, "concepts", "gamma", "Gamma", "# Gamma (orphan)");

        let graph = build_graph(root).unwrap();
        assert_eq!(graph.nodes.len(), 3);
        // alpha -> beta, beta -> alpha, beta -> gamma => 3 edges
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn graph_counts_inbound_and_outbound() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "alpha", "Alpha", "# Alpha\n[[beta]] [[gamma]]");
        write_page(root, "concepts", "beta", "Beta", "# Beta");
        write_page(root, "concepts", "gamma", "Gamma", "# Gamma\n[[beta]]");

        let graph = build_graph(root).unwrap();
        let alpha = graph.nodes.iter().find(|n| n.slug == "alpha").unwrap();
        let beta = graph.nodes.iter().find(|n| n.slug == "beta").unwrap();
        assert_eq!(alpha.outbound, 2);
        assert_eq!(alpha.inbound, 0);
        assert_eq!(beta.inbound, 2); // from alpha and gamma
        assert_eq!(beta.outbound, 0);
    }

    #[test]
    fn graph_counts_orphans() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_page(root, "concepts", "linked", "Linked", "# Linked\n[[other]]");
        write_page(root, "concepts", "other", "Other", "# Other");
        write_page(root, "concepts", "orphan", "Orphan", "# Orphan (no inbound)");

        let graph = build_graph(root).unwrap();
        // "orphan" has 0 inbound; "linked" has 0 inbound too (it only links out).
        // So orphan_count = 2 (linked + orphan). "other" has 1 inbound from "linked".
        assert!(graph.orphan_count >= 1);
    }

    #[test]
    fn graph_empty_wiki_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let graph = build_graph(tmp.path()).unwrap();
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }
}
