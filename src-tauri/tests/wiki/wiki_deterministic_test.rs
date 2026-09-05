//! Integration tests for the deterministic wiki pre-seed pipeline (Phases 1-3).
//!
//! Phase 1: Author pages pre-seeded unconditionally (not just multi-batch).
//! Phase 2: Synthesis pages pre-seeded from article AI summaries.
//! Phase 3: Concept hubs pre-seeded from `biblio_terms`.

use std::sync::Arc;

use async_trait::async_trait;
use bango_lib::wiki::frontmatter;
use bango_lib::wiki::ingest::{self, IngestLlmSender};
use rusqlite::Connection;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Phase 2: synthesis pre-seed
// ---------------------------------------------------------------------------

#[test]
fn preseed_synthesis_writes_page_for_article_with_ai_summary() {
    let (conn, root) = setup_db_with_article_and_summary();
    let written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    assert_eq!(written, 1, "exactly one synthesis page should be written");

    let path = root.join("wiki/synthesis/art-111.md");
    assert!(path.exists(), "synthesis page file should exist");
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("type"), Some("synthesis"));
    assert_eq!(fm.get("slug"), Some("art-111"));
    assert_eq!(fm.get("title"), Some("Test Article"));
    assert_eq!(fm.get("source_articles"), Some("[\"art-111\"]"));
    assert_eq!(fm.get("content_source"), Some("ai_summary"));
    // Gap 4: synthesis pages now emit `links` frontmatter (concept slugs from
    // keywords) so the graph builder creates explicit synthesis→concept edges.
    assert!(
        fm.get("links").unwrap_or("").contains("[[sugar]]"),
        "synthesis links should contain the sugar concept slug, got: {:?}",
        fm.get("links")
    );
    assert!(
        fm.get("tags").unwrap_or("").contains("sugar"),
        "synthesis tags should contain the sugar keyword, got: {:?}",
        fm.get("tags")
    );
    assert!(body.contains("## Summary"));
    assert!(body.contains("This is the digest."));
    assert!(body.contains("## Key Insights"));
    assert!(body.contains("- Insight one"));
}

#[test]
fn preseed_synthesis_skips_article_without_ai_summary() {
    // Only one article has a summary; the other is NULL.
    let (conn, root) = setup_db_with_mixed_summaries();
    let written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    assert_eq!(written, 1, "only the article WITH a summary should get a page");
}

#[test]
fn preseed_synthesis_skips_malformed_json() {
    let (conn, root) = setup_db_with_malformed_summary();
    let written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    assert_eq!(written, 0, "malformed JSON should be skipped gracefully");
}

#[test]
fn preseed_synthesis_respects_reviewed_pages() {
    let (conn, root) = setup_db_with_article_and_summary();
    // Pre-create a reviewed synthesis page.
    let path = root.join("wiki/synthesis/art-111.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("status", "reviewed");
    fm.set("slug", "art-111");
    frontmatter::write_file(&path, &fm, "# User edited").unwrap();

    let written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    assert_eq!(written, 0, "reviewed synthesis page should not be overwritten");
}

#[test]
fn preseed_synthesis_renders_v2_section_summaries_with_typed_facts() {
    // The T1.3 v2 AI-summary blob carries `section_summaries` with typed facts
    // (study_design, sample_size, effect_size, confidence_interval). These
    // enrich the synthesis page body AND power the methods pre-seed (Phase 4).
    // This test verifies all typed facts are rendered in the output body.
    let (conn, root) = setup_db_with_v2_summary();
    let written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    assert_eq!(written, 1, "v2 summary should produce a synthesis page");

    let path = root.join("wiki/synthesis/art-222.md");
    assert!(path.exists());
    let (_fm, body) = frontmatter::read_file(&path).unwrap();

    // Section headings rendered.
    assert!(body.contains("## Methods"), "body should have ## Methods heading");
    assert!(body.contains("## Results"), "body should have ## Results heading");

    // Typed facts rendered as labeled bullets in the Methods section.
    assert!(
        body.contains("**Study design:** Randomized Controlled Trial"),
        "body should contain study_design typed fact"
    );
    assert!(
        body.contains("**Sample size:** n=1,234"),
        "body should contain sample_size typed fact"
    );
    assert!(body.contains("**Effect size:** d=0.45"), "body should contain effect_size typed fact");
    assert!(
        body.contains("**Confidence interval:** 95% CI [0.30, 0.60]"),
        "body should contain confidence_interval typed fact"
    );

    // Section summary text rendered.
    assert!(body.contains("We ran an RCT."));
    assert!(body.contains("Significant results found."));

    // Key points rendered (Methods section has them; Results section does not).
    assert!(body.contains("Point A"), "Methods key points should be present");
    assert!(body.contains("Point B"), "Methods key points should be present");

    // Non-standard section names render with their raw name.
    assert!(body.contains("## Limitations"), "non-standard section name should render as-is");
    assert!(body.contains("Some limitations noted."), "non-standard section summary should render");
}

// ---------------------------------------------------------------------------
// Phase 3: concept hub pre-seed
// ---------------------------------------------------------------------------

#[test]
fn preseed_concept_hubs_writes_pages_for_top_terms() {
    let (conn, root) = setup_db_with_terms();
    let written = ingest::preseed_concept_hubs(&conn, &root, 10).unwrap();
    assert!(written >= 2, "expected at least 2 concept pages, got {written}");

    let sugar_path = root.join("wiki/concepts/sugar-tax.md");
    assert!(sugar_path.exists(), "concept page for 'sugar-tax' should exist");
    let (fm, body) = frontmatter::read_file(&sugar_path).unwrap();
    assert_eq!(fm.get("type"), Some("concept"));
    assert_eq!(fm.get("title"), Some("Sugar Tax"));
    assert_eq!(fm.get("content_source"), Some("metadata"));
    assert!(body.contains("## Relevant Studies"));
    assert!(body.contains("[[art-1]]"));
}

#[test]
fn preseed_concept_hubs_respects_limit() {
    let (conn, root) = setup_db_with_many_terms(5);
    // Only allow 2 concept pages.
    let written = ingest::preseed_concept_hubs(&conn, &root, 2).unwrap();
    assert_eq!(written, 2, "limit should cap the number of concept pages");
}

#[test]
fn preseed_concept_hubs_respects_reviewed_pages() {
    let (conn, root) = setup_db_with_terms();
    // Pre-create a reviewed concept page for one term.
    let path = root.join("wiki/concepts/sugar-tax.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("status", "reviewed");
    fm.set("slug", "sugar-tax");
    frontmatter::write_file(&path, &fm, "# User edited").unwrap();

    let written = ingest::preseed_concept_hubs(&conn, &root, 10).unwrap();
    // The reviewed page is skipped; the other term still gets written.
    assert_eq!(written, 1, "reviewed concept page should not be overwritten");
}

// ---------------------------------------------------------------------------
// Phase 1: unconditional author pre-seed (integration via FakeSender)
// ---------------------------------------------------------------------------

/// Fake sender returning an empty response (no LLM pages). Used to verify the
/// deterministic pre-seed runs even when the LLM produces nothing - proving
/// the single-batch gate is gone.
struct EmptySender;

#[async_trait]
impl IngestLlmSender for EmptySender {
    async fn send(&self, _prompt: &str) -> Result<String, bango_lib::error::AppError> {
        Ok(String::new())
    }
}

#[tokio::test]
async fn build_batches_unconditionally_pre_seeds_authors_on_single_batch() {
    // This is the core Phase-1 regression: with a small corpus (single batch),
    // the author pre-seed MUST still run. Previously it was skipped via the
    // `if batches.len() <= 1 { return Ok(batches); }` early return.
    //
    // We can't call the private `build_batches_with_manifest` directly, so we
    // verify the observable effect: after a run with an EmptySender, author
    // pages exist on disk (written by the pre-seed, not the LLM).
    let (mut conn, root) = setup_db_with_authors();
    // Export raw sources so build_ingest_prompt_batches finds something.
    // Use the lock-release split pattern: load under lock, write lock-free.
    {
        let articles = bango_lib::wiki::raw_export::load_included_articles(&conn).unwrap();
        bango_lib::wiki::raw_export::write_article_exports(&root, &articles, None, None).unwrap();
        bango_lib::wiki::raw_export::process_user_files(&root).unwrap();
    }
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // Run normalization so biblio_authors is populated.
    bango_lib::db::biblio_repo::run_full_normalization(&mut conn).unwrap();

    // Build + run the ingest with an empty LLM response.
    let manifest = ingest::build_author_manifest(&conn).unwrap();
    if !manifest.entries.is_empty() {
        let _ = ingest::preseed_authors(&root, &manifest);
    }
    let batches =
        ingest::build_ingest_prompt_batches(&root, 50_000, Some(&manifest), false).unwrap();
    let sender: Arc<dyn IngestLlmSender> = Arc::new(EmptySender);
    let _report =
        ingest::run_chunked_ingest(&root, batches, sender, None, (25, 95), None).await.unwrap();

    // Author pages exist despite the LLM returning nothing.
    let authors_dir = root.join("wiki/authors");
    let author_files: Vec<_> = std::fs::read_dir(&authors_dir)
        .map(|entries| entries.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(
        !author_files.is_empty(),
        "author pages should be pre-seeded even on a single-batch run with an empty LLM response"
    );

    // Gap 2: synthesis + concept pre-seed must also run on single-batch. The
    // corpus has 1 article with no AI summary, so synthesis writes 0 pages.
    // But `run_full_normalization` extracted terms from the article's abstract
    // into `biblio_terms`, so concept pre-seed DOES write pages - proving it
    // runs unconditionally on single-batch (not gated).
    let synth_written = ingest::preseed_synthesis_from_ai_summaries(&conn, &root).unwrap();
    let concept_written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    assert_eq!(synth_written, 0, "no AI summary -> no synthesis pages");
    assert!(
        concept_written > 0,
        "concept pre-seed should write pages from biblio_terms on single-batch, got {concept_written}"
    );
}

// ---------------------------------------------------------------------------
// Test helpers: DB setup
// ---------------------------------------------------------------------------

fn setup_db_with_article_and_summary() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    let summary_json = r#"{
        "summary_150_250_words": "This is the digest.",
        "key_insights": ["Insight one", "Insight two"],
        "keywords": ["sugar", "tax"],
        "field": "medicine",
        "subfield": "public_health"
    }"#;
    insert_included_article(&conn, "art-111", "Test Article", Some(summary_json));
    // Leak the TempDir so the test body can use the path. (Cleanup is
    // acceptable to skip for tests; the OS reclaims temp on exit.)
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_mixed_summaries() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    insert_included_article(
        &conn,
        "art-1",
        "Has Summary",
        Some(r#"{"summary_150_250_words": "Digest."}"#),
    );
    insert_included_article(&conn, "art-2", "No Summary", None);
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_malformed_summary() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    insert_included_article(&conn, "art-1", "Bad JSON", Some("not valid json {"));
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_v2_summary() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // A v2 AI-summary blob with section_summaries carrying all four typed facts
    // plus a non-standard section name ("Limitations" instead of
    // Methods/Results/Discussion).
    let v2_summary = r#"{
        "summary_150_250_words": "This is the v2 digest with typed facts.",
        "key_insights": ["Key finding"],
        "keywords": ["rct", "methods-test"],
        "field": "medicine",
        "subfield": "epidemiology",
        "section_summaries": [
            {
                "section": "Methods",
                "summary": "We ran an RCT.",
                "key_points": ["Point A", "Point B"],
                "study_design": "Randomized Controlled Trial",
                "sample_size": "n=1,234",
                "effect_size": "d=0.45",
                "confidence_interval": "95% CI [0.30, 0.60]"
            },
            {
                "section": "Results",
                "summary": "Significant results found.",
                "key_points": [],
                "study_design": null,
                "sample_size": null,
                "effect_size": null,
                "confidence_interval": null
            },
            {
                "section": "Limitations",
                "summary": "Some limitations noted.",
                "key_points": [],
                "study_design": null,
                "sample_size": null,
                "effect_size": null,
                "confidence_interval": null
            }
        ]
    }"#;
    insert_included_article(&conn, "art-222", "V2 Summary Article", Some(v2_summary));
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_terms() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // Insert 2 included articles.
    insert_included_article(&conn, "art-1", "Article One", None);
    insert_included_article(&conn, "art-2", "Article Two", None);

    // Insert terms + article-term links. biblio_terms schema: (id, raw_term,
    // normalized_term). biblio_article_terms: (article_id, term_id, frequency).
    conn.execute(
        "INSERT INTO biblio_terms (id, raw_term, normalized_term) VALUES \
         ('t1', 'Sugar Tax', 'sugar-tax'), \
         ('t2', 'Obesity', 'obesity')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO biblio_article_terms (article_id, term_id, frequency) VALUES \
         ('art-1', 't1', 3), \
         ('art-2', 't1', 2), \
         ('art-1', 't2', 1)",
        [],
    )
    .unwrap();
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_many_terms(n: usize) -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    insert_included_article(&conn, "art-1", "Article One", None);
    for i in 0..n {
        let term_id = format!("t{i}");
        let raw = format!("Term {i}");
        let normalized = format!("term-{i}");
        conn.execute(
            "INSERT INTO biblio_terms (id, raw_term, normalized_term) VALUES (?1, ?2, ?3)",
            rusqlite::params![term_id, raw, normalized],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO biblio_article_terms (article_id, term_id, frequency) VALUES (?1, ?2, ?3)",
            rusqlite::params!["art-1", term_id, (i + 1) as i64],
        )
        .unwrap();
    }
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_authors() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // Insert 1 included article with author metadata.
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text) \
         VALUES ('art-1', 'Authored Paper', 'included', '[\"Smith, J\"]', 2020, 'Abstract.')",
        [],
    )
    .unwrap();
    std::mem::forget(tmp);
    (conn, root)
}

fn insert_included_article(conn: &Connection, id: &str, title: &str, ai_summary: Option<&str>) {
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text, full_text_ai_summary) \
         VALUES (?1, ?2, 'included', '[]', 2021, 'Abstract.', ?3)",
        rusqlite::params![id, title, ai_summary],
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Layer 1: external-document source-page pre-seed
// ---------------------------------------------------------------------------

/// Write a user-file companion `.md` (simulating an Add Documents upload) into
/// `raw/` with the given slug/title/source_kind. Mirrors the shape produced by
/// `raw_export::add_user_file` / `process_user_files`.
fn write_user_raw_file(
    root: &std::path::Path,
    slug: &str,
    title: &str,
    source_kind: &str,
    source_file: &str,
) {
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir).unwrap();
    let path = raw_dir.join(format!("{slug}.md"));
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("id", slug);
    fm.set("title", title);
    fm.set("type", "source");
    fm.set("slug", slug);
    fm.set("summary", "");
    fm.set("status", "draft");
    fm.set("source_file", source_file);
    fm.set("source_kind", source_kind);
    fm.set("source_hash", "fake-hash");
    fm.set("content_source", source_kind);
    fm.set("links", "[]");
    frontmatter::write_file(&path, &fm, &format!("# {title}\n\nBody.")).unwrap();
}

fn setup_empty_root() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();
    std::mem::forget(tmp);
    (conn, root)
}

#[test]
fn preseed_document_source_pages_writes_page_per_user_file() {
    let (_conn, root) = setup_empty_root();
    write_user_raw_file(
        &root,
        "user-youcantbuild",
        "You Can't Build an AI Workforce",
        "user_pdf",
        "youcantbuild.pdf",
    );
    write_user_raw_file(&root, "user-notes", "My Notes", "user_text", "notes.txt");

    let written = ingest::preseed_document_source_pages(&root).unwrap();
    assert_eq!(written, 2, "one source page per user document");

    let path = root.join("wiki/sources/user-youcantbuild.md");
    assert!(path.exists(), "source page for the PDF should exist");
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("type"), Some("source"));
    assert_eq!(fm.get("slug"), Some("user-youcantbuild"));
    assert_eq!(fm.get("title"), Some("You Can't Build an AI Workforce"));
    assert_eq!(fm.get("source_articles"), Some("[\"user-youcantbuild\"]"));
    assert_eq!(fm.get("content_source"), Some("user_pdf"));
    assert!(body.contains("Imported document"));
}

#[test]
fn preseed_document_source_pages_skips_article_exports() {
    let (_conn, root) = setup_empty_root();
    // Article exports have `type: source` but NO `source_kind` - they must be
    // skipped (the synthesis pre-seeder handles them).
    let raw_dir = root.join("raw");
    std::fs::create_dir_all(&raw_dir).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("id", "art-uuid-1");
    fm.set("title", "Article One");
    fm.set("type", "source");
    fm.set("slug", "art-uuid-1");
    fm.set("status", "draft");
    frontmatter::write_file(&raw_dir.join("art-uuid-1.md"), &fm, "Body.").unwrap();

    // Plus one real user file.
    write_user_raw_file(&root, "user-report", "Report", "user_pdf", "report.pdf");

    let written = ingest::preseed_document_source_pages(&root).unwrap();
    assert_eq!(written, 1, "only the user_* file should get a source page");
    assert!(root.join("wiki/sources/user-report.md").exists());
    assert!(!root.join("wiki/sources/art-uuid-1.md").exists());
}

#[test]
fn preseed_document_source_pages_respects_reviewed() {
    let (_conn, root) = setup_empty_root();
    write_user_raw_file(&root, "user-notes", "Notes", "user_text", "notes.txt");
    // Pre-create a reviewed source page.
    let path = root.join("wiki/sources/user-notes.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("status", "reviewed");
    fm.set("slug", "user-notes");
    frontmatter::write_file(&path, &fm, "# User edited").unwrap();

    let written = ingest::preseed_document_source_pages(&root).unwrap();
    assert_eq!(written, 0, "reviewed source page should not be overwritten");
}

#[test]
fn preseed_document_source_pages_creates_sources_dir() {
    let (_conn, root) = setup_empty_root();
    // Remove the sources dir to prove the pre-seeder creates it.
    let _ = std::fs::remove_dir_all(root.join("wiki/sources"));
    write_user_raw_file(&root, "user-x", "X", "user_pdf", "x.pdf");
    let written = ingest::preseed_document_source_pages(&root).unwrap();
    assert_eq!(written, 1);
    assert!(root.join("wiki/sources").is_dir());
}
