//! Integration tests for the wiki ingest pipeline (core + batching +
//! consolidation + slugs + author/synthesis rendering).
//!
//! These tests were extracted from the former inline `#[cfg(test)] mod tests`
//! block in `src/wiki/ingest.rs` (lines 1951-2716 of the pre-refactor file) to
//! keep the source compact per `docs/CLAUDE.md` §Testing. The consolidation
//! pipeline end-to-end tests live in `wiki_consolidation_test.rs`; the
//! deterministic pre-seed integration tests (DB-backed) live in
//! `wiki_deterministic_test.rs`.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use bango_lib::error::AppError;
use bango_lib::wiki::frontmatter::{self, Frontmatter};
use bango_lib::wiki::fts;
use bango_lib::wiki::ingest::authors::{
    render_author_page, AuthorArticle, AuthorManifest, AuthorManifestEntry,
};
use bango_lib::wiki::ingest::batching::{build_ingest_prompt_batches, MAX_BATCH_INPUT_CHARS};
use bango_lib::wiki::ingest::consolidation::{jaccard_similarity, rewrite_body_links};
use bango_lib::wiki::ingest::slugs::{author_slug, sanitize_slug};
use bango_lib::wiki::ingest::{
    self, consolidate_pages, parse_llm_pages, run_chunked_ingest, IngestLlmSender, ParsedPage,
    MAX_SOURCE_CHARS,
};
use rusqlite::Connection;
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

    // write_page is exercised via write_pages_from_response (it is
    // pub(super) so not directly callable from the external test crate).
    let _page =
        ParsedPage { slug: "alpha".to_string(), frontmatter: fm, body: "# Alpha".to_string() };
    let response =
        "<!-- PAGE:alpha -->\n---\nid: alpha\ntitle: \"Alpha\"\ntype: concept\nslug: alpha\n\
         summary: \"\"\nstatus: draft\nlinks: []\n---\n\n# Alpha\n"
            .to_string();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(bango_lib::wiki::ingest::write_pages_from_response(root, &response, None)).unwrap();

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

    let response = "<!-- PAGE:jane-doe -->\n---\nid: jane-doe\ntitle: \"Jane Doe\"\ntype: author\n\
         slug: jane-doe\nsummary: \"\"\nstatus: draft\nlinks: []\n---\n\n# Jane Doe\n"
        .to_string();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(bango_lib::wiki::ingest::write_pages_from_response(root, &response, None)).unwrap();

    assert!(root.join("wiki/authors/jane-doe.md").exists());
}

#[tokio::test]
async fn run_ingest_from_response_writes_pages_and_clears_flag() {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();

    // Mark as needing refresh.
    bango_lib::db::app_settings_repo::mark_wiki_needs_refresh(&conn);
    assert!(bango_lib::db::app_settings_repo::get_wiki_needs_refresh(&conn).unwrap());

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
    let mut report =
        bango_lib::wiki::ingest::write_pages_from_response(root, response, None).await.unwrap();
    bango_lib::wiki::ingest::finalize_ingest(&conn, root, &mut report).unwrap();
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
    assert!(!bango_lib::db::app_settings_repo::get_wiki_needs_refresh(&conn).unwrap());

    // Log entry appended.
    let log = std::fs::read_to_string(root.join("wiki/log.md")).unwrap();
    assert!(log.contains("ingest"));
}

// NOTE: The legacy single-call `build_ingest_prompt` was deleted (Tier B2 of
// the wiki-hallucination plan). These two tests were migrated onto the
// production batch path `build_ingest_prompt_batches`, which produces
// equivalent single-batch output when given a large context window.

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

    // Single source + large window -> one batch carrying contract + source.
    let batches = build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert_eq!(batches.len(), 1);
    let prompt = &batches[0].prompt;
    assert!(prompt.contains("Agent Contract"));
    assert!(prompt.contains("Article One"));
    assert!(prompt.contains("Article content here"));
    assert!(prompt.contains("<!-- PAGE:slug -->"));
}

#[test]
fn build_ingest_prompt_splits_when_over_budget() {
    // Migrated from the legacy truncation test. The batch path SPLITS the
    // corpus into multiple batches instead of truncating, so we assert the
    // multi-batch outcome (the production behavior).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "Contract").unwrap();

    // Write many large sources to exceed a small context window's budget.
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

    // Tiny context window forces a multi-batch split (no truncation).
    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
    assert!(batches.len() > 1, "expected multiple batches, got {}", batches.len());
    // Every source appears in exactly one batch's source_slugs.
    let mut all: Vec<String> = batches.iter().flat_map(|b| b.source_slugs.clone()).collect();
    all.sort();
    assert_eq!(all.len(), 50, "all 50 sources must be covered across batches");
}

#[test]
fn sanitize_slug_replaces_special_chars() {
    assert_eq!(sanitize_slug("sugar-tax!"), "sugar-tax");
    assert_eq!(sanitize_slug("foo bar baz"), "foo-bar-baz");
    assert_eq!(sanitize_slug("---leading"), "leading");
}

// -----------------------------------------------------------------
// Author manifest + pre-seed
// -----------------------------------------------------------------

#[test]
fn author_slug_is_prefixed_and_kebab() {
    assert_eq!(author_slug("smith j"), "author-smith-j");
    // Punctuation becomes a single dash; consecutive dashes collapse.
    assert_eq!(author_slug("O'Brien, K."), "author-o-brien-k");
    assert_eq!(author_slug(""), "author-unnamed");
}

#[test]
fn preseed_authors_writes_pages_and_respects_reviewed() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();

    let manifest = AuthorManifest {
        entries: vec![AuthorManifestEntry {
            slug: "author-smith-j".to_string(),
            display_name: "Smith, J".to_string(),
            raw_variants: vec!["smith, j".to_string()],
            article_count: 3,
            ..Default::default()
        }],
    };
    let written = bango_lib::wiki::ingest::preseed_authors(root, &manifest).unwrap();
    assert_eq!(written, 1);
    let path = root.join("wiki/authors/author-smith-j.md");
    assert!(path.exists());
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("type"), Some("author"));
    assert_eq!(fm.get("status"), Some("draft"));
    // New template: "## Publications" header + empty articles list.
    assert!(body.contains("## Publications"));

    // Now mark it reviewed and re-seed - should skip.
    let mut fm2 = fm.clone();
    fm2.set("status", "reviewed");
    frontmatter::write_file(&path, &fm2, "# User edited").unwrap();
    let written2 = bango_lib::wiki::ingest::preseed_authors(root, &manifest).unwrap();
    assert_eq!(written2, 0, "reviewed author page should not be overwritten");
}

#[test]
fn render_author_page_emits_art_prefixed_refs_and_no_raw_lines() {
    // Regression: the pre-seeder previously emitted inline refs as
    // `[^{id}]` (no `art-` prefix) plus a `/raw/{id}.md` definition block.
    // The renderer only resolves `[^art-{uuid}]`, so the refs rendered as
    // literal `[^...]` text and the definitions leaked as duplicate
    // clutter. This test pins the new contract: refs carry the `art-`
    // prefix and the body contains zero `/raw/` lines.
    let entry = AuthorManifestEntry {
        slug: "author-doe-j".to_string(),
        display_name: "Doe, J".to_string(),
        article_count: 2,
        articles: vec![
            AuthorArticle {
                id: "11111111-1111-1111-1111-111111111111".to_string(),
                title: "Paper One".to_string(),
                year: Some(2020),
                journal: Some("Nature".to_string()),
            },
            AuthorArticle {
                id: "22222222-2222-2222-2222-222222222222".to_string(),
                title: "Paper Two".to_string(),
                year: Some(2023),
                journal: None,
            },
        ],
        h_index: Some(5),
        total_citations: 100,
        ..Default::default()
    };
    let (_fm, body) = render_author_page(&entry);

    // Each publication row carries an art-prefixed ref.
    assert!(body.contains("[^art-11111111-1111-1111-1111-111111111111]"));
    assert!(body.contains("[^art-22222222-2222-2222-2222-222222222222]"));

    // No bare (non-art) ref keys and no /raw/ artifact lines.
    assert!(!body.contains("/raw/"), "body must not contain /raw/ paths: {body}");
    // A bare `[^{uuid}]` (no art-) would indicate the old bug.
    assert!(
        !body.contains("[^11111111") && !body.contains("[^22222222"),
        "refs must be art-prefixed, body was: {body}"
    );

    // The publications list + metrics are still present.
    assert!(body.contains("## Publications"));
    assert!(body.contains("Paper One"));
    assert!(body.contains("h-index: 5"));
}

// -----------------------------------------------------------------
// Chunked / parallel ingest
// -----------------------------------------------------------------

/// Write `n` raw source files with bodies of roughly `body_chars` each.
fn write_many_sources(root: &std::path::Path, n: usize, body_chars: usize) {
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
    use bango_lib::wiki::ingest::batching::batch_input_char_budget;
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

    let batches = build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
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

    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
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

    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
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

    let batches = build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert!(batches.is_empty());
}

#[test]
fn batch_prompt_contains_no_quota_language() {
    // Tier D1 guard: the batch directive must NOT contain "at least N" quota
    // language (it causes hallucination - the LLM invents entities to hit the
    // count). Asserts the production batch prompt is quota-free.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_many_sources(root, 2, 500);

    let batches = build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert_eq!(batches.len(), 1);
    let prompt = &batches[0].prompt;
    assert!(
        !prompt.to_lowercase().contains("at least "),
        "batch prompt must not contain quota language, got: {prompt}"
    );
    assert!(
        !prompt.contains("Generate 3-5") && !prompt.contains("Generate 1-2"),
        "batch prompt must not contain numeric page-count quotas, got: {prompt}"
    );
}

#[test]
fn batch_prompt_methods_directive_when_pre_seeded() {
    // When methods were pre-seeded, the directive tells the LLM to link, not
    // duplicate. The focus list still asks for methods (so gaps are filled).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_many_sources(root, 2, 500);

    let batches = build_ingest_prompt_batches(root, 50_000, None, true).unwrap();
    assert_eq!(batches.len(), 1);
    let prompt = &batches[0].prompt;
    assert!(
        prompt.contains("Method pages have ALSO been pre-seeded"),
        "when methods_pre_seeded=true, directive must say methods are pre-seeded, got: {prompt}"
    );
    assert!(
        prompt.contains("METHOD pages for research methodologies"),
        "focus list must still mention methods even when pre-seeded, got: {prompt}"
    );
}

#[test]
fn batch_prompt_methods_directive_when_not_pre_seeded() {
    // When methods were NOT pre-seeded, the directive tells the LLM to create
    // them. This is the critical fix: the LLM is always asked to produce method
    // pages when the pre-seed didn't.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_many_sources(root, 2, 500);

    let batches = build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert_eq!(batches.len(), 1);
    let prompt = &batches[0].prompt;
    assert!(
        prompt.contains("Method pages have NOT been pre-seeded"),
        "when methods_pre_seeded=false, directive must say methods are NOT pre-seeded, got: {prompt}"
    );
    assert!(
        prompt.contains("You SHOULD create method pages"),
        "when methods_pre_seeded=false, directive must tell the LLM to create methods, got: {prompt}"
    );
    assert!(
        prompt.contains("METHOD pages for research methodologies"),
        "focus list must mention methods, got: {prompt}"
    );
}

#[test]
fn build_ingest_prompt_batches_injects_manifest_section() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_many_sources(root, 2, 500);

    let manifest = AuthorManifest {
        entries: vec![AuthorManifestEntry {
            slug: "author-smith-j".to_string(),
            display_name: "Smith, J".to_string(),
            raw_variants: vec!["smith, j".to_string()],
            article_count: 5,
            ..Default::default()
        }],
    };
    let batches = build_ingest_prompt_batches(root, 50_000, Some(&manifest), false).unwrap();
    assert_eq!(batches.len(), 1);
    assert!(batches[0].prompt.contains("Author Pages (Pre-Seeded"));
    assert!(batches[0].prompt.contains("[[author-smith-j]]"));
    // Phase 6: the directive splits known (link, don't duplicate) from
    // unknown (create new) authors.
    assert!(batches[0].prompt.contains("LINK, DON'T DUPLICATE"));
    assert!(batches[0].prompt.contains("New Authors from Uploaded Documents"));
    assert!(batches[0].prompt.contains("you SHOULD create a new author page"));
    // Gap 1: Phase-4 prompt wording must also be present so a revert is
    // caught. The thematic-only directive narrows the LLM's output to
    // cross-cutting synthesis + new-author pages.
    assert!(batches[0].prompt.contains("ALREADY been pre-seeded"));
    assert!(batches[0].prompt.contains("THEMATIC CROSS-CUTTING"));
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
        // Each page carries source_articles + a [^art-] ref so it passes the
        // Tier A1 grounding gate (the post-ingest lint flags ungrounded pages).
        let mut out = String::new();
        for cap in regex::Regex::new(r"slug: (art-\d+)").unwrap().captures_iter(prompt) {
            let slug = &cap[1];
            out.push_str(&format!(
                "<!-- PAGE:{slug} -->\n---\nid: {slug}\ntitle: \"{slug}\"\ntype: concept\n\
                 slug: {slug}\nsummary: \"\"\nstatus: draft\nlinks: []\n\
                 source_articles: [\"{slug}\"]\n---\n\n# {slug}\n\nBody. [^art-{slug}]\n\n"
            ));
        }
        Ok(out)
    }
}

#[tokio::test]
async fn run_chunked_ingest_processes_batches_in_parallel_and_writes_all_pages() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();
    // 6 sources, small window -> multiple batches.
    write_many_sources(root, 6, 2000);

    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
    let n_batches = batches.len();
    assert!(n_batches > 1);

    let sender: Arc<dyn IngestLlmSender> = Arc::new(FakeSender { delay_ms: 30, fail_marker: None });
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
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();
    write_many_sources(root, 6, 2000);

    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
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
    assert!(present >= 5, "expected >=5 pages written despite one batch failure, got {present}");
}

#[tokio::test]
async fn run_chunked_ingest_empty_when_no_batches() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();

    let sender: Arc<dyn IngestLlmSender> = Arc::new(FakeSender { delay_ms: 0, fail_marker: None });
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
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();
    write_many_sources(root, 8, 2000);

    let batches = build_ingest_prompt_batches(root, 2_000, None, false).unwrap();
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

// -----------------------------------------------------------------
// Consolidation (deterministic dedup + link rewrite)
// -----------------------------------------------------------------

fn make_page(slug: &str, page_type: &str, source_articles: &[&str]) -> ParsedPage {
    let mut fm = Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", slug);
    fm.set("type", page_type);
    fm.set("slug", slug);
    fm.set("status", "draft");
    fm.set("summary", "");
    fm.set("links", "[]");
    let sources = format!(
        "[{}]",
        source_articles.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
    );
    fm.set("source_articles", &sources);
    ParsedPage { slug: slug.to_string(), frontmatter: fm, body: format!("# {slug}\n\nBody.") }
}

#[test]
fn consolidate_merges_exact_slug_duplicates() {
    let mut pages = vec![
        make_page("childhood-obesity", "concept", &["art-1"]),
        make_page("childhood-obesity", "concept", &["art-2"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 1);
    assert_eq!(slug_map.len(), 1);
    // Body contains both perspectives.
    assert!(pages[0].body.contains("Additional perspectives"));
    // Source articles unioned.
    let sources =
        frontmatter::parse_list(pages[0].frontmatter.get("source_articles").unwrap_or(""));
    assert!(sources.contains(&"art-1".to_string()));
    assert!(sources.contains(&"art-2".to_string()));
}

#[test]
fn consolidate_merges_near_duplicate_jaccard() {
    // Word-reordering case: "childhood-obesity" and "obesity-childhood"
    // both stem to {childhood, obes}, so Jaccard = 1.0.
    let mut pages = vec![
        make_page("childhood-obesity", "concept", &["art-1"]),
        make_page("obesity-childhood", "concept", &["art-2"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 1, "near-duplicate pages should merge into one");
    assert_eq!(slug_map.len(), 1);
}

#[test]
fn consolidate_merges_shared_source_articles() {
    // Two differently-named pages that cite the same articles.
    let mut pages = vec![
        make_page("sugar-levy-impact", "concept", &["art-1", "art-3"]),
        make_page("ssb-tax-effects", "concept", &["art-1", "art-3"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 1);
    assert_eq!(slug_map.len(), 1);
}

#[test]
fn consolidate_does_not_merge_unrelated_pages() {
    let mut pages = vec![
        make_page("sugar-tax", "concept", &["art-1"]),
        make_page("exercise", "concept", &["art-2"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 2);
    assert!(slug_map.is_empty());
}

#[test]
fn consolidate_does_not_merge_different_types() {
    let mut pages = vec![
        make_page("sugar-tax", "concept", &["art-1", "art-2"]),
        make_page("sugar-tax", "method", &["art-1", "art-2"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 2, "pages of different types should not merge");
    assert!(slug_map.is_empty());
}

#[test]
fn consolidate_does_not_merge_author_pages() {
    let mut pages = vec![
        make_page("author-smith-j", "author", &["art-1"]),
        make_page("author-smith-j", "author", &["art-2"]),
    ];
    let slug_map = consolidate_pages(&mut pages);
    assert_eq!(pages.len(), 2, "author pages are pre-seeded and should not be merged");
    assert!(slug_map.is_empty());
}

#[test]
fn rewrite_body_links_updates_simple_links() {
    let body = "See [[obesity-in-children]] for more.";
    let mut map = std::collections::HashMap::new();
    map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
    let rewritten = rewrite_body_links(body, &map);
    assert_eq!(rewritten, "See [[childhood-obesity]] for more.");
}

#[test]
fn rewrite_body_links_preserves_aliases() {
    let body = "See [[obesity-in-children|kids weight]] for more.";
    let mut map = std::collections::HashMap::new();
    map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
    let rewritten = rewrite_body_links(body, &map);
    assert_eq!(rewritten, "See [[childhood-obesity|kids weight]] for more.");
}

#[test]
fn rewrite_body_links_is_case_insensitive() {
    let body = "See [[Obesity-In-Children]] and [[OBESITY-IN-CHILDREN]].";
    let mut map = std::collections::HashMap::new();
    map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
    let rewritten = rewrite_body_links(body, &map);
    assert!(rewritten.contains("[[childhood-obesity]]"));
    // Both occurrences rewritten.
    assert_eq!(rewritten.matches("[[childhood-obesity]]").count(), 2);
}

#[test]
fn rewrite_body_links_leaves_unmapped_links_alone() {
    let body = "See [[sugar-tax]] and [[obesity-in-children]].";
    let mut map = std::collections::HashMap::new();
    map.insert("obesity-in-children".to_string(), "childhood-obesity".to_string());
    let rewritten = rewrite_body_links(body, &map);
    assert!(rewritten.contains("[[sugar-tax]]"));
    assert!(rewritten.contains("[[childhood-obesity]]"));
}

#[test]
fn jaccard_similarity_handles_overlap() {
    let a: HashSet<String> = ["obes", "childhood"].iter().map(|s| s.to_string()).collect();
    let b: HashSet<String> = ["obes", "children"].iter().map(|s| s.to_string()).collect();
    // Intersection = {obes} = 1, Union = {obes, childhood, children} = 3
    let sim = jaccard_similarity(&a, &b);
    assert!((sim - 1.0 / 3.0).abs() < 0.001);
}

#[test]
fn jaccard_similarity_identical_sets() {
    let a: HashSet<String> = ["obes", "child"].iter().map(|s| s.to_string()).collect();
    let sim = jaccard_similarity(&a, &a);
    assert!((sim - 1.0).abs() < 0.001);
}

#[test]
fn jaccard_similarity_disjoint_sets() {
    let a: HashSet<String> = ["alpha"].iter().map(|s| s.to_string()).collect();
    let b: HashSet<String> = ["beta"].iter().map(|s| s.to_string()).collect();
    let sim = jaccard_similarity(&a, &b);
    assert!((sim - 0.0).abs() < 0.001);
}

// Reference the ingest module so unused-import warnings stay clean when
// the build configuration changes.
#[allow(dead_code)]
fn _ensure_ingest_linked(_: &ingest::IngestReport) {}
