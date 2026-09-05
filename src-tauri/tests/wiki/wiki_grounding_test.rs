//! Integration tests for the Tier A1 grounding gate.
//!
//! The gate flags LLM-generated pages (concept/method/synthesis) that lack
//! provenance: either no `source_articles` frontmatter or no `[^art-]`
//! citations in the body. Author and source pages are exempt.

use bango_lib::wiki::engine::{lint, LintKind, LintSeverity};
use bango_lib::wiki::frontmatter::{self, Frontmatter};
use tempfile::TempDir;

fn write_grounded_page(root: &std::path::Path, subdir: &str, slug: &str, title: &str, body: &str) {
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", title);
    fm.set("type", "concept");
    fm.set("slug", slug);
    fm.set("status", "draft");
    fm.set("links", "[]");
    fm.set("source_articles", "[\"art-1\"]");
    frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
}

fn write_ungrounded_page_no_sources(
    root: &std::path::Path,
    subdir: &str,
    slug: &str,
    title: &str,
    body: &str,
) {
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", title);
    fm.set("type", "concept");
    fm.set("slug", slug);
    fm.set("status", "draft");
    fm.set("links", "[]");
    // No source_articles - should trigger the grounding gate.
    frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
}

#[test]
fn ungrounded_page_flagged_when_no_source_articles() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Page with no source_articles + no [^art-] refs -> two grounding issues.
    write_ungrounded_page_no_sources(
        root,
        "concepts",
        "phantom",
        "Phantom",
        "# Phantom\nNo citations.",
    );

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert_eq!(
        grounding.len(),
        2,
        "expected 2 grounding issues (no sources + no citations), got {}: {:?}",
        grounding.len(),
        grounding
    );
    // The no-sources one is an error; the no-citations one is a warning.
    assert!(grounding
        .iter()
        .any(|i| i.severity == LintSeverity::Error && i.message.contains("source_articles")));
    assert!(grounding
        .iter()
        .any(|i| i.severity == LintSeverity::Warning && i.message.contains("citations")));
}

#[test]
fn ungrounded_page_flagged_when_no_art_refs() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Page WITH source_articles but NO [^art-] refs -> one warning.
    let dir = root.join("wiki/concepts");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "alpha");
    fm.set("title", "Alpha");
    fm.set("type", "concept");
    fm.set("slug", "alpha");
    fm.set("status", "draft");
    fm.set("links", "[]");
    fm.set("source_articles", "[\"art-1\"]");
    // Body with no [^art-] citations.
    frontmatter::write_file(&dir.join("alpha.md"), &fm, "# Alpha\nBody with no citations.")
        .unwrap();

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert_eq!(
        grounding.len(),
        1,
        "expected 1 grounding issue (no citations only), got {}: {:?}",
        grounding.len(),
        grounding
    );
    assert!(grounding[0].message.contains("citations"));
    assert_eq!(grounding[0].severity, LintSeverity::Warning);
}

#[test]
fn grounded_page_not_flagged() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Page WITH both source_articles AND [^art-] refs -> no grounding issues.
    write_grounded_page(root, "concepts", "alpha", "Alpha", "# Alpha\nGrounded claim [^art-1].");

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert!(grounding.is_empty(), "grounded page should not be flagged, got: {grounding:?}");
}

#[test]
fn author_page_not_flagged_despite_no_art_refs() {
    // Author pages are exempt from the grounding gate (pre-seeded; provenance
    // is the publications list, not [^art-] refs).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let dir = root.join("wiki/authors");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "author-doe-j");
    fm.set("title", "Doe, J");
    fm.set("type", "author");
    fm.set("slug", "author-doe-j");
    fm.set("status", "draft");
    fm.set("links", "[]");
    // Author pages DO carry source_articles, but even if they didn't the gate
    // would not flag them (type author is exempt).
    fm.set("source_articles", "[\"art-1\"]");
    // Body with no [^art-] refs.
    frontmatter::write_file(&dir.join("author-doe-j.md"), &fm, "# Doe, J\nPublications.").unwrap();

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert!(
        grounding.is_empty(),
        "author page should be exempt from grounding gate, got: {grounding:?}"
    );
}

#[test]
fn source_page_not_flagged_despite_no_art_refs() {
    // Source pages (external documents) are exempt.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let dir = root.join("wiki/sources");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "user-doc");
    fm.set("title", "User Doc");
    fm.set("type", "source");
    fm.set("slug", "user-doc");
    fm.set("status", "draft");
    fm.set("links", "[]");
    fm.set("source_articles", "[\"user-doc\"]");
    frontmatter::write_file(&dir.join("user-doc.md"), &fm, "# User Doc\nBody.").unwrap();

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert!(
        grounding.is_empty(),
        "source page should be exempt from grounding gate, got: {grounding:?}"
    );
}

#[test]
fn method_page_flagged_when_ungrounded() {
    // Method pages ARE subject to the grounding gate (they are concept-shaped).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let dir = root.join("wiki/methods");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "rct");
    fm.set("title", "Randomized Controlled Trial");
    fm.set("type", "method");
    fm.set("slug", "randomized-controlled-trial");
    fm.set("status", "draft");
    fm.set("links", "[]");
    // No source_articles -> should be flagged.
    frontmatter::write_file(&dir.join("randomized-controlled-trial.md"), &fm, "# RCT\nNo sources.")
        .unwrap();

    let report = lint(root).unwrap();
    let grounding: Vec<_> =
        report.issues.iter().filter(|i| i.kind == LintKind::UngroundedPage).collect();
    assert!(!grounding.is_empty(), "ungrounded method page should be flagged, got: {grounding:?}");
}

#[test]
fn lint_run_after_ingest_reports_ungrounded_llm_pages() {
    // End-to-end: write a mix of grounded + ungrounded pages (simulating an
    // LLM ingest that produced some pages with provenance and some without),
    // then run the lint and confirm the ungrounded ones are flagged.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Grounded concept page (has source_articles + [^art-] ref).
    write_grounded_page(
        root,
        "concepts",
        "grounded",
        "Grounded",
        "# Grounded\nReal claim [^art-1].",
    );
    // Ungrounded concept page (LLM fabricated, no sources, no citations).
    write_ungrounded_page_no_sources(
        root,
        "concepts",
        "phantom",
        "Phantom",
        "# Phantom\nInvented.",
    );
    // Grounded method page.
    let dir = root.join("wiki/methods");
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = Frontmatter::default();
    fm.set("id", "rct");
    fm.set("title", "RCT");
    fm.set("type", "method");
    fm.set("slug", "rct");
    fm.set("status", "draft");
    fm.set("links", "[]");
    fm.set("source_articles", "[\"art-2\"]");
    frontmatter::write_file(&dir.join("rct.md"), &fm, "# RCT\nUsed in [^art-2].").unwrap();

    let report = lint(root).unwrap();
    let ungrounded_slugs: Vec<&str> = report
        .issues
        .iter()
        .filter(|i| i.kind == LintKind::UngroundedPage)
        .map(|i| i.slug.as_str())
        .collect();
    assert!(
        ungrounded_slugs.contains(&"phantom"),
        "phantom (ungrounded) should be flagged, got: {ungrounded_slugs:?}"
    );
    assert!(
        !ungrounded_slugs.contains(&"grounded"),
        "grounded page should NOT be flagged, got: {ungrounded_slugs:?}"
    );
    assert!(
        !ungrounded_slugs.contains(&"rct"),
        "grounded method page should NOT be flagged, got: {ungrounded_slugs:?}"
    );
}
