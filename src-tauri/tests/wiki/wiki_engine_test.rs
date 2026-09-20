//! Tests for `wiki::engine` (deterministic lint + link-graph builder).
//!
//! Extracted from the inline `#[cfg(test)] mod tests` in
//! `src/wiki/engine.rs` to keep the source file compact.

use std::path::Path;

use bango_lib::wiki::engine::{build_graph, extract_wikilinks, lint, LintKind, LintSeverity};
use bango_lib::wiki::frontmatter::{self, Frontmatter};

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
    // Grounding contract (Tier A1): concept pages must carry provenance.
    fm.set("source_articles", "[\"art-1\"]");
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
    let broken: Vec<_> = report.issues.iter().filter(|i| i.kind == LintKind::BrokenLink).collect();
    assert!(broken.is_empty(), "case-insensitive links should not be flagged broken: {broken:?}");

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
    let dups: Vec<_> = report.issues.iter().filter(|i| i.kind == LintKind::DuplicateSlug).collect();
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
    assert!(!report.issues.iter().any(|i| i.kind == LintKind::OrphanPage && i.page == "index.md"));
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

#[test]
fn graph_node_includes_summary_from_frontmatter() {
    // The graph hover tooltip shows the page summary, so build_graph must
    // propagate the frontmatter `summary` field onto each GraphNode.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let dir = root.join("wiki/concepts");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "alpha");
    fm.set("title", "Alpha");
    fm.set("type", "concept");
    fm.set("slug", "alpha");
    fm.set("status", "draft");
    fm.set("summary", "A short overview of the alpha concept.");
    frontmatter::write_file(&dir.join("alpha.md"), &fm, "# Alpha").unwrap();

    let graph = build_graph(root).unwrap();
    let alpha = graph.nodes.iter().find(|n| n.slug == "alpha").unwrap();
    assert_eq!(alpha.summary, "A short overview of the alpha concept.");
}

#[test]
fn graph_node_summary_empty_when_frontmatter_omits_it() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_page(root, "concepts", "alpha", "Alpha", "# Alpha");

    let graph = build_graph(root).unwrap();
    let alpha = graph.nodes.iter().find(|n| n.slug == "alpha").unwrap();
    assert_eq!(alpha.summary, "");
}
