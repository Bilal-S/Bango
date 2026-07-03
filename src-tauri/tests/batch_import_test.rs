//! End-to-end integration tests for the batch-import pipeline.
//!
//! Exercises the full Phase 1 (full text) and Phase 2 (citations) against a
//! real in-memory SQLite database + tempdir fixture files. Phase 3 (AI
//! summaries) requires a live LLM and is not covered here; see
//! `summary_engine_test.rs` for the LLM-mocked summary path.

use std::io::Write;

use bango_lib::batch_import::{citations_phase, full_text_phase};
use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
use bango_lib::db::article_repo;
use bango_lib::db::article_repo::get_articles_with_doi_info;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::reference_repo;
use bango_lib::models::article::NewArticle;
use bango_lib::scraping::citation_chaser::clean_doi_filename;
use rusqlite::Connection;
use tempfile::TempDir;

/// Create an in-memory SQLite database with all migrations applied.
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Configure the storage root to point at a temp dir so tests don't touch the
/// real documents folder. Creates `fulltext/` and `ris/` subdirs.
fn configure_storage_root(conn: &Connection, root: &std::path::Path) {
    set_setting(conn, STORAGE_ROOT_KEY, root.to_str()).unwrap();
    std::fs::create_dir_all(root.join("fulltext")).unwrap();
    std::fs::create_dir_all(root.join("ris")).unwrap();
}

/// Insert an article with the given DOI and return its id.
fn insert_article_with_doi(conn: &Connection, doi: &str, title: &str) -> String {
    let article =
        NewArticle { title: title.to_string(), doi: Some(doi.to_string()), ..Default::default() };
    let inserted = article_repo::insert_article(conn, &article).expect("insert article");
    // insert_article defaults to 'duplicate' status; move to working.
    article_repo::move_to_working(conn, &inserted.id).expect("move to working");
    inserted.id
}

/// Write a minimal `.txt` full-text file named after the cleaned DOI.
fn write_fulltext_txt(root: &std::path::Path, doi: &str, content: &str) {
    let stem = clean_doi_filename(doi);
    let path = root.join("fulltext").join(format!("{stem}.txt"));
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "{content}").unwrap();
}

/// Write a minimal `_references.ris` file named after the cleaned DOI.
fn write_references_ris(root: &std::path::Path, doi: &str, records: &[&str]) {
    let stem = clean_doi_filename(doi);
    let path = root.join("ris").join(format!("{stem}_references.ris"));
    let mut f = std::fs::File::create(&path).unwrap();
    for title in records {
        writeln!(f, "TY  - JOUR").unwrap();
        writeln!(f, "TI  - {title}").unwrap();
        writeln!(f, "ER  -").unwrap();
        writeln!(f).unwrap();
    }
}

/// Write a minimal `_citations.ris` file named after the cleaned DOI.
fn write_citations_ris(root: &std::path::Path, doi: &str, records: &[&str]) {
    let stem = clean_doi_filename(doi);
    let path = root.join("ris").join(format!("{stem}_citations.ris"));
    let mut f = std::fs::File::create(&path).unwrap();
    for title in records {
        writeln!(f, "TY  - JOUR").unwrap();
        writeln!(f, "TI  - {title}").unwrap();
        writeln!(f, "ER  -").unwrap();
        writeln!(f).unwrap();
    }
}

/// A closure that never reports cancellation.
fn never_cancel() -> impl Fn() -> bool {
    || false
}

/// A no-op progress callback for tests.
fn noop_progress() -> impl FnMut(usize, usize, &str) {
    |_, _, _| {}
}

// ── Phase 1: Full Text ──────────────────────────────────────────────────────

#[test]
fn phase1_attaches_full_text_to_matching_articles() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi_a = "10.1001/test-a";
    let doi_b = "10.1001/test-b";
    let _id_a = insert_article_with_doi(&conn, doi_a, "Article A");
    let _id_b = insert_article_with_doi(&conn, doi_b, "Article B");

    write_fulltext_txt(tmp.path(), doi_a, "This is the full text of Article A.");
    write_fulltext_txt(tmp.path(), doi_b, "This is the full text of Article B.");

    let (result, newly_attached) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1");

    assert_eq!(result.total, 2, "should discover 2 files");
    assert_eq!(result.succeeded, 2, "should attach both");
    assert_eq!(newly_attached.len(), 2, "should return 2 newly-attached IDs");

    // Verify the DB flags are set.
    let articles = get_articles_with_doi_info(&conn).unwrap();
    for a in &articles {
        assert!(a.has_full_text, "article {} should have full text", a.id);
    }
}

#[test]
fn phase1_skips_articles_that_already_have_full_text() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/test-skip";
    let id = insert_article_with_doi(&conn, doi, "Skip Article");

    // Pre-attach full text so the article already has it.
    article_repo::update_full_text(&conn, &id, "existing text", "existing.txt", false).unwrap();

    write_fulltext_txt(tmp.path(), doi, "This should NOT be attached.");

    let (result, newly_attached) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1");

    assert_eq!(result.total, 0, "should discover 0 importable files (already attached)");
    assert!(newly_attached.is_empty(), "no newly-attached IDs");
}

#[test]
fn phase1_ignores_files_with_no_matching_doi() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    // Article has a DOI, but the file stem doesn't match.
    let _id = insert_article_with_doi(&conn, "10.1001/matched", "Matched");

    // Write a file with a non-matching stem.
    let path = tmp.path().join("fulltext").join("10.9999/unmatched.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "unmatched content").unwrap();

    let (result, _) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1");

    assert_eq!(result.total, 0, "no files should match");
}

#[test]
fn phase1_skips_article_without_doi() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    // Insert an article without a DOI.
    let article = NewArticle { title: "No DOI".to_string(), ..Default::default() };
    let inserted = article_repo::insert_article(&conn, &article).unwrap();
    article_repo::move_to_working(&conn, &inserted.id).unwrap();

    // Write a file that exists but has no matching article.
    write_fulltext_txt(tmp.path(), "10.1001/orphan", "orphan text");

    let (result, _) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1");

    assert_eq!(result.total, 0, "no files should match (article has no DOI)");
}

// ── Phase 2: Citations ──────────────────────────────────────────────────────

#[test]
fn phase2_imports_references_from_ris_files() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/cite-test";
    let _id = insert_article_with_doi(&conn, doi, "Citation Test");

    write_references_ris(tmp.path(), doi, &["Reference Paper 1", "Reference Paper 2"]);

    let result = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2");

    assert_eq!(result.total, 1, "should discover 1 references file");
    assert_eq!(result.succeeded, 1, "should import it");
    assert!(result.errors.is_empty(), "no errors");

    // Verify reference papers were created.
    let paper_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM reference_papers", [], |row| row.get(0)).unwrap();
    assert_eq!(paper_count, 2, "2 reference papers should be inserted");

    // Verify the article now has reference details.
    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_reference_details, "article should have reference details");
}

#[test]
fn phase2_imports_citations_from_ris_files() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/cite-fwd";
    let _id = insert_article_with_doi(&conn, doi, "Forward Citation Test");

    write_citations_ris(tmp.path(), doi, &["Citing Paper 1"]);

    let result = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2");

    assert_eq!(result.total, 1, "should discover 1 citations file");
    assert_eq!(result.succeeded, 1, "should import it");

    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_citation_details, "article should have citation details");
}

#[test]
fn phase2_imports_references_and_citations_independently() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/both";
    let _id = insert_article_with_doi(&conn, doi, "Both Refs and Cits");

    write_references_ris(tmp.path(), doi, &["Ref A", "Ref B"]);
    write_citations_ris(tmp.path(), doi, &["Citing C"]);

    let result = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2");

    // Both files should be discovered and imported.
    assert_eq!(result.total, 2, "should discover 2 files (refs + cits)");
    assert_eq!(result.succeeded, 2, "should import both");

    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_reference_details, "should have reference details");
    assert!(articles[0].has_citation_details, "should have citation details");
}

#[test]
fn phase2_skips_articles_that_already_have_reference_details() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/already-refs";
    let id = insert_article_with_doi(&conn, doi, "Already Has Refs");

    // Pre-populate: set has_reference_details = true by importing once.
    write_references_ris(tmp.path(), doi, &["Pre-existing Ref"]);
    // Insert a reference paper and link it so the flag is set.
    let _ = reference_repo::create_link(
        &conn,
        &id,
        "fake-paper-id",
        &bango_lib::models::reference::ReferenceType::Reference,
    );
    // Manually set the flag (simulating a prior import).
    conn.execute(
        "UPDATE articles SET has_reference_details = 1 WHERE id = ?1",
        rusqlite::params![&id],
    )
    .unwrap();

    let result = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2");

    // The references file should be skipped because the article already has refs.
    assert_eq!(result.total, 0, "should discover 0 files (refs already present)");
}

// ── Idempotency ─────────────────────────────────────────────────────────────

#[test]
fn full_pipeline_is_idempotent_on_second_run() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/idempotent";
    let _id = insert_article_with_doi(&conn, doi, "Idempotent Article");

    write_fulltext_txt(tmp.path(), doi, "Full text content.");
    write_references_ris(tmp.path(), doi, &["Ref 1", "Ref 2"]);

    // First run: should attach + import.
    let (ft1, attached1) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1 run 1");
    assert_eq!(ft1.succeeded, 1, "first run should attach 1 file");
    assert_eq!(attached1.len(), 1);

    let cit1 = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2 run 1");
    assert_eq!(cit1.succeeded, 1, "first run should import 1 refs file");

    // Second run: should find nothing to do (all flags set).
    let (ft2, attached2) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1 run 2");
    assert_eq!(ft2.total, 0, "second run should find 0 files to attach");
    assert!(attached2.is_empty());

    let cit2 = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2 run 2");
    assert_eq!(cit2.total, 0, "second run should find 0 refs to import");
}

// ── Multiple articles ───────────────────────────────────────────────────────

#[test]
fn pipeline_handles_multiple_articles_with_mixed_files() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    // Three articles: one with fulltext only, one with refs only, one with both.
    let doi_a = "10.1001/multi-a";
    let doi_b = "10.1001/multi-b";
    let doi_c = "10.1001/multi-c";
    let _id_a = insert_article_with_doi(&conn, doi_a, "Multi A");
    let _id_b = insert_article_with_doi(&conn, doi_b, "Multi B");
    let _id_c = insert_article_with_doi(&conn, doi_c, "Multi C");

    // A: fulltext only
    write_fulltext_txt(tmp.path(), doi_a, "Full text A.");
    // B: references only
    write_references_ris(tmp.path(), doi_b, &["B Ref 1"]);
    // C: both
    write_fulltext_txt(tmp.path(), doi_c, "Full text C.");
    write_references_ris(tmp.path(), doi_c, &["C Ref 1", "C Ref 2"]);

    // Phase 1: should attach A and C (2 files), skip B (no fulltext file).
    let (ft, attached) =
        full_text_phase::run_full_text_phase(&conn, &never_cancel(), &mut noop_progress())
            .expect("phase 1");
    assert_eq!(ft.succeeded, 2, "should attach 2 files (A and C)");
    assert_eq!(attached.len(), 2);

    // Phase 2: should import refs for B and C (2 files), skip A (no refs file).
    let cit = citations_phase::run_citations_phase(&conn, &never_cancel(), &mut noop_progress())
        .expect("phase 2");
    assert_eq!(cit.succeeded, 2, "should import 2 refs files (B and C)");

    // Verify per-article state.
    let articles = get_articles_with_doi_info(&conn).unwrap();
    let by_doi: std::collections::HashMap<&str, &_> =
        articles.iter().map(|a| (a.doi.as_str(), a)).collect();

    assert!(by_doi[doi_a].has_full_text, "A should have full text");
    assert!(!by_doi[doi_a].has_reference_details, "A should NOT have refs");

    assert!(!by_doi[doi_b].has_full_text, "B should NOT have full text");
    assert!(by_doi[doi_b].has_reference_details, "B should have refs");

    assert!(by_doi[doi_c].has_full_text, "C should have full text");
    assert!(by_doi[doi_c].has_reference_details, "C should have refs");
}
