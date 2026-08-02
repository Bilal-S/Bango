//! Integration tests for the wiki module.
//!
//! Mirrors the `biblio_needs_refresh_test.rs` pattern: in-memory SQLite via
//! `run_migrations`, `tempfile` for the wiki-root directory.
//!
//! Covers:
//! - `wiki_needs_refresh` staleness-flag round-trip (mark/clear/absent default).
//! - `export_included_articles`: writes `.md` per included article; ignores
//!   rejected/working articles; honors the content fallback; idempotent.
//! - `list_raw_files`: returns parsed frontmatter.

use bango_lib::db::app_settings_repo::{
    clear_wiki_needs_refresh, get_wiki_needs_refresh, mark_wiki_needs_refresh,
};
use bango_lib::db::article_repo::{
    self, get_articles_by_status, insert_article, update_article_status,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::wiki::engine;
use bango_lib::wiki::frontmatter;
use bango_lib::wiki::fts;
use bango_lib::wiki::ingest;
use bango_lib::wiki::raw_export::{self, RawSourceKind};
use rusqlite::Connection;
use tempfile::TempDir;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn sample_new_article(title: &str, abstract_text: &str) -> NewArticle {
    NewArticle {
        title: title.to_string(),
        abstract_text: abstract_text.to_string(),
        authors: vec!["Doe, J".to_string()],
        publication_year: Some(2024),
        doi: Some(format!("10.1/{title}")),
        journal: Some("Nature".to_string()),
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec!["sugar-tax".to_string()],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        eissn: None,
        journal_index_id: None,
        reference_type: None,
        date: None,
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        ris_extras: None,
        import_source: None,
        data_length: None,
        token_estimate: None,
        num_cited: None,
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
    }
}

/// Insert an article and move it to the given status via the repo.
fn insert_with_status(conn: &Connection, article: NewArticle, status: &str) -> String {
    let inserted = insert_article(conn, &article).unwrap();
    // insert_article hardcodes 'duplicate'; resolve to working then set target.
    resolve_duplicate(conn, &inserted.id);
    if status != "working" {
        update_article_status(conn, &inserted.id, status).unwrap();
    }
    inserted.id
}

/// Resolve a duplicate to working (mirrors the dedup resolve path).
fn resolve_duplicate(conn: &Connection, id: &str) {
    conn.execute(
        "UPDATE articles SET status = 'working', changed_at = datetime('now') WHERE id = ?1",
        rusqlite::params![id],
    )
    .unwrap();
}

// -------------------------------------------------------------------------
// wiki_needs_refresh flag round-trip
// -------------------------------------------------------------------------

#[test]
fn wiki_needs_refresh_defaults_to_false_when_absent() {
    let conn = test_db();
    assert!(!get_wiki_needs_refresh(&conn).unwrap());
}

#[test]
fn wiki_needs_refresh_mark_and_clear_round_trip() {
    let conn = test_db();

    mark_wiki_needs_refresh(&conn);
    assert!(get_wiki_needs_refresh(&conn).unwrap());

    clear_wiki_needs_refresh(&conn);
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    // Re-marking flips it back; double-mark is idempotent.
    mark_wiki_needs_refresh(&conn);
    mark_wiki_needs_refresh(&conn);
    assert!(get_wiki_needs_refresh(&conn).unwrap());
}

#[test]
fn wiki_needs_refresh_persisted_in_app_settings() {
    let conn = test_db();
    mark_wiki_needs_refresh(&conn);
    let raw: Option<String> = conn
        .query_row("SELECT value FROM app_settings WHERE key = 'wiki_needs_refresh'", [], |r| {
            r.get(0)
        })
        .ok();
    assert_eq!(raw.as_deref(), Some("true"));
}

// -------------------------------------------------------------------------
// export_included_articles
// -------------------------------------------------------------------------

#[test]
fn export_writes_one_md_per_included_article() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    insert_with_status(&conn, sample_new_article("Alpha", "abstract alpha"), "included");
    insert_with_status(&conn, sample_new_article("Beta", "abstract beta"), "included");

    let report = raw_export::export_included_articles(&conn, root).unwrap();
    assert_eq!(report.articles_written, 2);
    assert_eq!(report.articles_skipped, 0);

    // Two .md files in raw/
    let count = std::fs::read_dir(root.join("raw")).unwrap().count();
    assert_eq!(count, 2);
}

#[test]
fn export_ignores_rejected_and_working_articles() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    insert_with_status(&conn, sample_new_article("Included", "abs-i"), "included");
    insert_with_status(&conn, sample_new_article("Rejected", "abs-r"), "rejected");
    insert_with_status(&conn, sample_new_article("Working", "abs-w"), "working");

    let report = raw_export::export_included_articles(&conn, root).unwrap();
    assert_eq!(report.articles_written, 1);

    // Only one .md file exists, and it's for the included article.
    let included = get_articles_by_status(&conn, "included").unwrap();
    assert_eq!(included.len(), 1);
    let expected_path = root.join(format!("raw/{}.md", included[0].id));
    assert!(expected_path.exists());

    // The rejected/working articles did NOT produce files.
    let rejected = get_articles_by_status(&conn, "rejected").unwrap();
    assert_eq!(rejected.len(), 1);
    let rejected_path = root.join(format!("raw/{}.md", rejected[0].id));
    assert!(!rejected_path.exists());
}

#[test]
fn export_content_fallback_uses_abstract_when_no_full_text() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let id =
        insert_with_status(&conn, sample_new_article("Fallback", "the abstract text"), "included");

    raw_export::export_included_articles(&conn, root).unwrap();

    let path = root.join(format!("raw/{id}.md"));
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("content_source"), Some("abstract"));
    assert!(body.contains("the abstract text"));
}

#[test]
fn export_is_idempotent_on_re_run() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    insert_with_status(&conn, sample_new_article("Stable", "stable abstract"), "included");

    let r1 = raw_export::export_included_articles(&conn, root).unwrap();
    assert_eq!(r1.articles_written, 1);
    assert_eq!(r1.articles_skipped, 0);

    // Second run with unchanged content: skipped.
    let r2 = raw_export::export_included_articles(&conn, root).unwrap();
    assert_eq!(r2.articles_written, 0);
    assert_eq!(r2.articles_skipped, 1);
}

#[test]
fn export_picks_up_changed_article_content() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let id = insert_with_status(&conn, sample_new_article("Changing", "v1 abstract"), "included");
    raw_export::export_included_articles(&conn, root).unwrap();

    // Mutate the article's abstract; re-export should rewrite.
    conn.execute(
        "UPDATE articles SET abstract_text = 'v2 abstract with more detail' WHERE id = ?1",
        rusqlite::params![id],
    )
    .unwrap();

    let r = raw_export::export_included_articles(&conn, root).unwrap();
    assert_eq!(r.articles_written, 1);
    assert_eq!(r.articles_skipped, 0);

    let path = root.join(format!("raw/{id}.md"));
    let (_, body) = frontmatter::read_file(&path).unwrap();
    assert!(body.contains("v2 abstract with more detail"));
}

#[test]
fn export_frontmatter_carries_article_metadata() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let id = insert_with_status(&conn, sample_new_article("Meta Article", "abs"), "included");
    raw_export::export_included_articles(&conn, root).unwrap();

    let path = root.join(format!("raw/{id}.md"));
    let (fm, _) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("title"), Some("Meta Article"));
    assert_eq!(fm.get("type"), Some("source"));
    assert_eq!(fm.get("status"), Some("draft"));
    assert_eq!(fm.get("doi"), Some("10.1/Meta Article"));
    assert_eq!(fm.get("journal"), Some("Nature"));
    assert_eq!(fm.get("content_source"), Some("abstract"));
    // source_articles list references the article id
    let sources = frontmatter::parse_list(fm.get("source_articles").unwrap_or(""));
    assert_eq!(sources, vec![id.clone()]);
}

// -------------------------------------------------------------------------
// load + write article exports + user files (lock-release split)
// -------------------------------------------------------------------------

#[test]
fn load_and_write_included_articles_and_process_user_files() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();

    insert_with_status(&conn, sample_new_article("Art", "art abstract"), "included");
    std::fs::write(root.join("raw/notes.txt"), "user notes").unwrap();

    let articles = raw_export::load_included_articles(&conn).unwrap();
    let article_report = raw_export::write_article_exports(root, &articles, None, None).unwrap();
    let user_report = raw_export::process_user_files(root).unwrap();
    assert_eq!(article_report.articles_written, 1);
    assert_eq!(user_report.user_files_written, 1);
    assert!(root.join("raw/user-notes.md").exists());
}

// -------------------------------------------------------------------------
// cancel aborts write_article_exports mid-loop (Phase A cancel)
// -------------------------------------------------------------------------

#[test]
fn cancel_aborts_write_article_exports_mid_loop() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Insert articles via the DB, then load them as proper `Article` values.
    let conn = test_db();
    let id1 = insert_with_status(&conn, sample_new_article("Article 1", "abstract 1"), "included");
    let id2 = insert_with_status(&conn, sample_new_article("Article 2", "abstract 2"), "included");
    let id3 = insert_with_status(&conn, sample_new_article("Article 3", "abstract 3"), "included");
    let articles = raw_export::load_included_articles(&conn).unwrap();
    assert_eq!(articles.len(), 3, "should have three included articles");
    // Confirm the IDs match.
    let ids: Vec<&str> = articles.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&id1.as_str()));
    assert!(ids.contains(&id2.as_str()));
    assert!(ids.contains(&id3.as_str()));

    // Cancel signalled from the start.
    let cancel = Arc::new(AtomicBool::new(true));
    let report = raw_export::write_article_exports(root, &articles, None, Some(&cancel)).unwrap();
    assert!(report.cancelled, "report should be cancelled");
    assert_eq!(
        report.articles_written, 0,
        "no articles should be written when cancelled before first iteration"
    );

    // Not-yet-cancelled: the first article writes, then cancel fires.
    let cancel2 = Arc::new(AtomicBool::new(false));
    let count = std::cell::Cell::new(0usize);
    let report2 = raw_export::write_article_exports(
        root,
        &articles,
        Some(&|_idx, _total, _article_id| {
            let n = count.get() + 1;
            count.set(n);
            // Signal cancel after the first article is written.
            if n >= 1 {
                cancel2.store(true, Ordering::SeqCst);
            }
        }),
        Some(&cancel2),
    )
    .unwrap();
    assert!(report2.cancelled, "report should be cancelled after first article");
    assert_eq!(
        report2.articles_written, 1,
        "first article should be written before cancel took effect"
    );
}

// -------------------------------------------------------------------------
// list_raw_files
// -------------------------------------------------------------------------

#[test]
fn list_raw_files_returns_parsed_frontmatter() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    insert_with_status(&conn, sample_new_article("Listed", "abs"), "included");
    raw_export::export_included_articles(&conn, root).unwrap();

    let files = raw_export::list_raw_files(root).unwrap();
    assert!(!files.is_empty());
    // Each entry has a title from frontmatter.
    let titles: Vec<&str> = files.iter().map(|(_, fm)| fm.get("title").unwrap_or("")).collect();
    assert!(titles.iter().any(|t| t == &"Listed"));
}

// -------------------------------------------------------------------------
// RawSourceKind classification (integration-flavored sanity check)
// -------------------------------------------------------------------------

#[test]
fn raw_source_kind_tokens_are_stable() {
    assert_eq!(RawSourceKind::UserPdf.as_token(), "user_pdf");
    assert_eq!(RawSourceKind::UserMarkdown.as_token(), "user_markdown");
    assert_eq!(RawSourceKind::Unsupported.as_token(), "unsupported");
}

// Silence unused import warning for the deep-link helper.
#[test]
fn article_repo_compiles_in_integration_context() {
    let conn = test_db();
    let _ = article_repo::get_article_counts(&conn);
}

// -------------------------------------------------------------------------
// FTS5 search (Phase 3 foundation)
// -------------------------------------------------------------------------

fn write_wiki_page(root: &std::path::Path, subdir: &str, slug: &str, title: &str, body: &str) {
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("id", slug);
    fm.set("slug", slug);
    fm.set("title", title);
    fm.set("type", "concept");
    fm.set("summary", &format!("{title} summary"));
    fm.set("status", "draft");
    // Grounding contract (Tier A1): carry provenance so the grounding gate
    // does not flag these test pages. Concept pages with empty
    // source_articles are flagged as ungrounded.
    fm.set("source_articles", "[\"art-1\"]");
    fm.set("links", "[]");
    frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
}

#[test]
fn fts_rebuild_indexes_wiki_pages_from_real_tree() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax beverage levy");
    write_wiki_page(root, "concepts", "exercise", "Exercise", "physical activity health");

    fts::ensure_table(&conn).unwrap();
    let count = fts::rebuild_index(&conn, root).unwrap();
    assert_eq!(count, 2);
}

#[test]
fn fts_search_finds_relevant_pages() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax beverage levy obesity");
    write_wiki_page(root, "concepts", "exercise", "Exercise", "physical activity health");

    fts::ensure_table(&conn).unwrap();
    fts::rebuild_index(&conn, root).unwrap();

    let hits = fts::search(&conn, "sugar", 5).unwrap();
    assert!(!hits.is_empty());
    let slugs: Vec<&str> = hits.iter().map(|h| h.slug.as_str()).collect();
    assert!(slugs.contains(&"sugar-tax"));
}

#[test]
fn fts_search_returns_empty_for_no_matches() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    fts::ensure_table(&conn).unwrap();
    fts::rebuild_index(&conn, root).unwrap();

    let hits = fts::search(&conn, "zzznonexistent", 5).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn fts_rebuild_is_idempotent_no_duplicates() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    fts::ensure_table(&conn).unwrap();
    fts::rebuild_index(&conn, root).unwrap();
    fts::rebuild_index(&conn, root).unwrap();

    let hits = fts::search(&conn, "alpha", 10).unwrap();
    assert_eq!(hits.len(), 1, "no duplicate entries after re-rebuild");
}

#[test]
fn fts_collect_wiki_pages_lists_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
    write_wiki_page(root, "authors", "beta", "Beta", "b");

    let pages = fts::collect_wiki_pages(root).unwrap();
    assert_eq!(pages.len(), 2);
}

// -------------------------------------------------------------------------
// Lint engine (Phase 4)
// -------------------------------------------------------------------------

#[test]
fn lint_detects_broken_link_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "alpha",
        "Alpha",
        "# Alpha
Links to [[nonexistent]].",
    );

    let report = engine::lint(root).unwrap();
    assert!(report.issues.iter().any(|i| i.kind == engine::LintKind::BrokenLink));
}

#[test]
fn lint_detects_orphan_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "# Alpha");
    write_wiki_page(root, "concepts", "orphan", "Orphan", "# Orphan");

    let report = engine::lint(root).unwrap();
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == engine::LintKind::OrphanPage && i.slug == "orphan"));
}

#[test]
fn lint_detects_duplicate_slug_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "dup", "Dup One", "# One");
    write_wiki_page(root, "synthesis", "dup", "Dup Two", "# Two");

    let report = engine::lint(root).unwrap();
    assert!(report.issues.iter().any(|i| i.kind == engine::LintKind::DuplicateSlug));
}

#[test]
fn lint_clean_wiki_has_no_errors_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "alpha",
        "Alpha",
        "# Alpha
See [[beta]].",
    );
    write_wiki_page(
        root,
        "concepts",
        "beta",
        "Beta",
        "# Beta
See [[alpha]].",
    );

    let report = engine::lint(root).unwrap();
    assert_eq!(report.errors, 0);
}

#[test]
fn lint_counts_are_consistent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "# Alpha\n[[bad-link]]");

    let report = engine::lint(root).unwrap();
    // issue_count should equal the sum of errors + warnings + infos.
    assert_eq!(report.issue_count, report.errors + report.warnings + report.infos);
    assert_eq!(report.issue_count, report.issues.len());
}

// -------------------------------------------------------------------------
// Page CRUD + delete (Phase 5)
// -------------------------------------------------------------------------

#[test]
fn get_page_finds_by_slug() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "alpha",
        "Alpha",
        "# Alpha
body text",
    );

    let pages = fts::collect_wiki_pages(root).unwrap();
    let mut found = None;
    for path in &pages {
        let (fm, body) = frontmatter::read_file(path).unwrap();
        if fm.get("slug") == Some("alpha") {
            found = Some((fm, body));
            break;
        }
    }
    let (fm, body) = found.expect("page not found");
    assert_eq!(fm.get("title"), Some("Alpha"));
    assert!(body.contains("body text"));
}

#[test]
fn update_page_preserves_other_frontmatter() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "# Alpha");

    // Find the page path.
    let pages = fts::collect_wiki_pages(root).unwrap();
    let path = pages
        .iter()
        .find(|p| {
            frontmatter::read_file(p)
                .map(|(fm, _)| fm.get("slug") == Some("alpha"))
                .unwrap_or(false)
        })
        .cloned()
        .unwrap();

    // Read, update title + body, write back (mirrors wiki_update_page).
    let (mut fm, _old) = frontmatter::read_file(&path).unwrap();
    fm.set("title", "Alpha Updated");
    fm.set("summary", "new summary");
    frontmatter::write_file(
        &path,
        &fm,
        "# Alpha Updated
new body",
    )
    .unwrap();

    let (fm2, body2) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm2.get("title"), Some("Alpha Updated"));
    assert_eq!(fm2.get("summary"), Some("new summary"));
    assert_eq!(fm2.get("type"), Some("concept")); // preserved
    assert!(body2.contains("new body"));
}

#[test]
fn delete_page_removes_file() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "# Alpha");

    let pages_before = fts::collect_wiki_pages(root).unwrap();
    assert_eq!(pages_before.len(), 1);

    // Find and delete (mirrors wiki_delete_page).
    let path = &pages_before[0];
    std::fs::remove_file(path).unwrap();

    let pages_after = fts::collect_wiki_pages(root).unwrap();
    assert_eq!(pages_after.len(), 0);
}

#[test]
fn delete_wiki_clears_wiki_subtree() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "# Alpha");
    write_wiki_page(root, "authors", "beta", "Beta", "# Beta");

    let wiki_dir = root.join("wiki");
    assert!(wiki_dir.exists());

    // Delete the wiki subtree (mirrors wiki_delete_wiki).
    std::fs::remove_dir_all(&wiki_dir).unwrap();
    assert!(!wiki_dir.exists());

    // raw/ and templates/ are unaffected (they live at wiki-root level).
    // After delete, collect returns empty.
    let pages = fts::collect_wiki_pages(root).unwrap();
    assert_eq!(pages.len(), 0);
}

// -------------------------------------------------------------------------
// Graph view (Phase 6)
// -------------------------------------------------------------------------

#[test]
fn graph_builds_nodes_and_edges_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "alpha",
        "Alpha",
        "# Alpha
See [[beta]].",
    );
    write_wiki_page(
        root,
        "concepts",
        "beta",
        "Beta",
        "# Beta
See [[alpha]] and [[gamma]].",
    );
    write_wiki_page(root, "concepts", "gamma", "Gamma", "# Gamma (orphan)");

    let graph = engine::build_graph(root).unwrap();
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 3);
}

#[test]
fn graph_counts_inbound_outbound_integration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "alpha",
        "Alpha",
        "# Alpha
[[beta]] [[gamma]]",
    );
    write_wiki_page(root, "concepts", "beta", "Beta", "# Beta");
    write_wiki_page(
        root,
        "concepts",
        "gamma",
        "Gamma",
        "# Gamma
[[beta]]",
    );

    let graph = engine::build_graph(root).unwrap();
    let alpha = graph.nodes.iter().find(|n| n.slug == "alpha").unwrap();
    let beta = graph.nodes.iter().find(|n| n.slug == "beta").unwrap();
    assert_eq!(alpha.outbound, 2);
    assert_eq!(alpha.inbound, 0);
    assert_eq!(beta.inbound, 2);
    assert_eq!(beta.outbound, 0);
}

#[test]
fn graph_empty_wiki_returns_empty_integration() {
    let tmp = TempDir::new().unwrap();
    let graph = engine::build_graph(tmp.path()).unwrap();
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
}

// -------------------------------------------------------------------------
// Ingest (Phase 6 - LLM page generation)
// -------------------------------------------------------------------------

#[test]
fn ingest_parse_llm_pages_extracts_pages() {
    let response = "<!-- PAGE:alpha -->\n---\nid: alpha\ntitle: \"Alpha\"\ntype: concept\nslug: alpha\nsummary: \"\"\nstatus: draft\nlinks: []\n---\n\n# Alpha\n\nBody text.";
    let pages = ingest::parse_llm_pages(response);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].slug, "alpha");
    assert_eq!(pages[0].frontmatter.get("title"), Some("Alpha"));
}

#[tokio::test]
async fn ingest_run_from_response_writes_and_indexes() {
    let conn = test_db();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    bango_lib::wiki::storage::scaffold_tree(root).unwrap();

    let response = "<!-- PAGE:concept-1 -->\n---\nid: concept-1\ntitle: \"Concept One\"\ntype: concept\nslug: concept-1\nsummary: \"A concept\"\nstatus: draft\nlinks: []\n---\n\n# Concept One\n\nA test concept.";
    let mut report = ingest::write_pages_from_response(root, response, None).await.unwrap();
    ingest::finalize_ingest(&conn, root, &mut report).unwrap();
    assert_eq!(report.pages_written, 1);
    assert!(root.join("wiki/concepts/concept-1.md").exists());

    // FTS index was rebuilt.
    bango_lib::wiki::fts::ensure_table(&conn).unwrap();
    let hits = bango_lib::wiki::fts::search(&conn, "concept", 5).unwrap();
    assert!(!hits.is_empty());
}

#[test]
fn ingest_build_prompt_includes_contract_and_sources() {
    // Migrated from the deleted legacy `build_ingest_prompt` onto the
    // production batch path `build_ingest_prompt_batches` (Tier B2).
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("raw")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "# Contract").unwrap();

    let mut fm = frontmatter::Frontmatter::default();
    fm.set("id", "src-1");
    fm.set("title", "Source One");
    fm.set("type", "source");
    fm.set("slug", "src-1");
    fm.set("status", "draft");
    fm.set("summary", "");
    fm.set("links", "[]");
    frontmatter::write_file(&root.join("raw/src-1.md"), &fm, "Source content").unwrap();

    // Single source + large window -> one batch carrying contract + source.
    let batches = ingest::build_ingest_prompt_batches(root, 50_000, None, false).unwrap();
    assert_eq!(batches.len(), 1);
    let prompt = &batches[0].prompt;
    assert!(prompt.contains("Contract"));
    assert!(prompt.contains("Source One"));
}
