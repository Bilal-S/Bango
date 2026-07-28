//! End-to-end integration tests for the batch-import pipeline.
//!
//! Exercises the full Phase 1 (full text) and Phase 2 (citations) against a
//! real in-memory SQLite database + tempdir fixture files. Phase 3 (AI
//! summaries) requires a live LLM and is not covered here; see
//! `summary_engine_test.rs` for the LLM-mocked summary path.
//!
//! Phases 1 and 2 are now `async` and lock the DB mutex in short bursts
//! (Concern 3). The tests wrap the in-memory `Connection` in a `Mutex` and
//! drive the async phase functions via `#[tokio::test]`.

use std::io::Write;
use std::sync::Mutex;

use bango_lib::batch_import::{citations_phase, full_text_phase, translations_phase};
use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
use bango_lib::db::article_repo;
use bango_lib::db::article_repo::get_articles_with_doi_info;
use bango_lib::db::audit_repo;
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

#[tokio::test]
async fn phase1_attaches_full_text_to_matching_articles() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi_a = "10.1001/test-a";
    let doi_b = "10.1001/test-b";
    let _id_a = insert_article_with_doi(&conn, doi_a, "Article A");
    let _id_b = insert_article_with_doi(&conn, doi_b, "Article B");

    write_fulltext_txt(tmp.path(), doi_a, "This is the full text of Article A.");
    write_fulltext_txt(tmp.path(), doi_b, "This is the full text of Article B.");

    let conn_mutex = Mutex::new(conn);
    let (result, newly_attached) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");

    assert_eq!(result.total, 2, "should discover 2 files");
    assert_eq!(result.succeeded, 2, "should attach both");
    assert_eq!(newly_attached.len(), 2, "should return 2 newly-attached IDs");

    // Verify the DB flags are set.
    let conn = conn_mutex.into_inner().unwrap();
    let articles = get_articles_with_doi_info(&conn).unwrap();
    for a in &articles {
        assert!(a.has_full_text, "article {} should have full text", a.id);
    }
}

#[tokio::test]
async fn phase1_skips_articles_that_already_have_full_text() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/test-skip";
    let id = insert_article_with_doi(&conn, doi, "Skip Article");

    // Pre-attach full text so the article already has it.
    article_repo::update_full_text(&conn, &id, "existing text", "existing.txt", false).unwrap();

    write_fulltext_txt(tmp.path(), doi, "This should NOT be attached.");

    let conn_mutex = Mutex::new(conn);
    let (result, newly_attached) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");

    assert_eq!(result.total, 0, "should discover 0 importable files (already attached)");
    assert!(newly_attached.is_empty(), "no newly-attached IDs");
}

#[tokio::test]
async fn phase1_ignores_files_with_no_matching_doi() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    // Article has a DOI, but the file stem doesn't match.
    let _id = insert_article_with_doi(&conn, "10.1001/matched", "Matched");

    // Write a file with a non-matching stem.
    let path = tmp.path().join("fulltext").join("10.9999_unmatched.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "unmatched content").unwrap();

    let conn_mutex = Mutex::new(conn);
    let (result, _) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");

    assert_eq!(result.total, 0, "no files should match");
}

#[tokio::test]
async fn phase1_skips_article_without_doi() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    // Insert an article without a DOI.
    let article = NewArticle { title: "No DOI".to_string(), ..Default::default() };
    let inserted = article_repo::insert_article(&conn, &article).unwrap();
    article_repo::move_to_working(&conn, &inserted.id).unwrap();

    // Write a file that exists but has no matching article.
    write_fulltext_txt(tmp.path(), "10.1001/orphan", "orphan text");

    let conn_mutex = Mutex::new(conn);
    let (result, _) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");

    assert_eq!(result.total, 0, "no files should match (article has no DOI)");
}

#[tokio::test]
async fn phase1_uses_doi_filename_when_article_has_doi() {
    // Concern 2: when the article has a DOI, the destination filename must be
    // `{clean_doi}.txt` (no UUID suffix), and the source file is already in
    // `fulltext/` with that exact name so no copy should occur (hard-link /
    // same-file short-circuit). After attach, the stored `full_text_file_name`
    // equals `{clean_doi}.txt`.
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/doi-name";
    let id = insert_article_with_doi(&conn, doi, "Named by DOI");

    write_fulltext_txt(tmp.path(), doi, "Full text content for DOI-named file.");

    let conn_mutex = Mutex::new(conn);
    let (result, _newly_attached) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");
    assert_eq!(result.succeeded, 1, "should attach the file");

    let conn = conn_mutex.into_inner().unwrap();
    let stored_name =
        article_repo::get_full_text_file_name(&conn, &id).unwrap().expect("file name");
    let expected = format!("{}.txt", clean_doi_filename(doi));
    assert_eq!(stored_name, expected, "destination filename must be DOI-based, not UUID-based");
}

// ── Phase 2: Citations ──────────────────────────────────────────────────────

#[tokio::test]
async fn phase2_imports_references_from_ris_files() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/cite-test";
    let _id = insert_article_with_doi(&conn, doi, "Citation Test");

    write_references_ris(tmp.path(), doi, &["Reference Paper 1", "Reference Paper 2"]);

    let conn_mutex = Mutex::new(conn);
    let result =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2");

    assert_eq!(result.total, 1, "should discover 1 references file");
    assert_eq!(result.succeeded, 1, "should import it");
    assert!(result.errors.is_empty(), "no errors");

    let conn = conn_mutex.into_inner().unwrap();
    // Verify reference papers were created.
    let paper_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM reference_papers", [], |row| row.get(0)).unwrap();
    assert_eq!(paper_count, 2, "2 reference papers should be inserted");

    // Verify the article now has reference details.
    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_reference_details, "article should have reference details");
}

#[tokio::test]
async fn phase2_imports_citations_from_ris_files() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/cite-fwd";
    let _id = insert_article_with_doi(&conn, doi, "Forward Citation Test");

    write_citations_ris(tmp.path(), doi, &["Citing Paper 1"]);

    let conn_mutex = Mutex::new(conn);
    let result =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2");

    assert_eq!(result.total, 1, "should discover 1 citations file");
    assert_eq!(result.succeeded, 1, "should import it");

    let conn = conn_mutex.into_inner().unwrap();
    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_citation_details, "article should have citation details");
}

#[tokio::test]
async fn phase2_imports_references_and_citations_independently() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/both";
    let _id = insert_article_with_doi(&conn, doi, "Both Refs and Cits");

    write_references_ris(tmp.path(), doi, &["Ref A", "Ref B"]);
    write_citations_ris(tmp.path(), doi, &["Citing C"]);

    let conn_mutex = Mutex::new(conn);
    let result =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2");

    // Both files should be discovered and imported.
    assert_eq!(result.total, 2, "should discover 2 files (refs + cits)");
    assert_eq!(result.succeeded, 2, "should import both");

    let conn = conn_mutex.into_inner().unwrap();
    let articles = get_articles_with_doi_info(&conn).unwrap();
    assert!(articles[0].has_reference_details, "should have reference details");
    assert!(articles[0].has_citation_details, "should have citation details");
}

#[tokio::test]
async fn phase2_skips_articles_that_already_have_reference_details() {
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

    let conn_mutex = Mutex::new(conn);
    let result =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2");

    // The references file should be skipped because the article already has refs.
    assert_eq!(result.total, 0, "should discover 0 files (refs already present)");
}

// ── Idempotency ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn full_pipeline_is_idempotent_on_second_run() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());

    let doi = "10.1001/idempotent";
    let _id = insert_article_with_doi(&conn, doi, "Idempotent Article");

    write_fulltext_txt(tmp.path(), doi, "Full text content.");
    write_references_ris(tmp.path(), doi, &["Ref 1", "Ref 2"]);

    let conn_mutex = Mutex::new(conn);

    // First run: should attach + import.
    let (ft1, attached1) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1 run 1");
    assert_eq!(ft1.succeeded, 1, "first run should attach 1 file");
    assert_eq!(attached1.len(), 1);

    let cit1 =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2 run 1");
    assert_eq!(cit1.succeeded, 1, "first run should import 1 refs file");

    // Second run: should find nothing to do (all flags set).
    let (ft2, attached2) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1 run 2");
    assert_eq!(ft2.total, 0, "second run should find 0 files to attach");
    assert!(attached2.is_empty());

    let cit2 =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2 run 2");
    assert_eq!(cit2.total, 0, "second run should find 0 refs to import");
}

// ── Multiple articles ───────────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_handles_multiple_articles_with_mixed_files() {
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

    let conn_mutex = Mutex::new(conn);

    // Phase 1: should attach A and C (2 files), skip B (no fulltext file).
    let (ft, attached) =
        full_text_phase::run_full_text_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 1");
    assert_eq!(ft.succeeded, 2, "should attach 2 files (A and C)");
    assert_eq!(attached.len(), 2);

    // Phase 2: should import refs for B and C (2 files), skip A (no refs file).
    let cit =
        citations_phase::run_citations_phase(&conn_mutex, &never_cancel(), &mut noop_progress())
            .await
            .expect("phase 2");
    assert_eq!(cit.succeeded, 2, "should import 2 refs files (B and C)");

    // Verify per-article state.
    let conn = conn_mutex.into_inner().unwrap();
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

// ── Phase 3: Translations pre-flight LLM check ──────────────────────────────

/// Count the system-level audit entries (`article_id IS NULL`,
/// `action = 'error'`) - the records written by `audit_repo::log_error`.
fn count_system_error_audit(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM audit_entries WHERE article_id IS NULL AND action = 'error'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn phase3_pre_flight_skips_when_llm_not_configured_and_writes_audit() {
    // When `auto_translate = true` but no LLM is configured, Phase 3 must
    // short-circuit with the "Skipped: LLM not configured" message and write a
    // system-level audit record so the skip surfaces in Diagnostics with an
    // actionable explanation (instead of silently churning every article
    // through the worker's per-article failure path).
    let conn = test_db();

    // No LLM config inserted -> `has_config` returns false.
    let skip = translations_phase::check_llm_configured_or_skip(&conn, 3);
    let result = skip.expect("pre-flight should return a skip result when LLM is not configured");

    assert_eq!(result.total, 3, "total echoes the input article count");
    assert_eq!(result.processed, 0, "nothing was processed");
    assert_eq!(result.succeeded, 0, "nothing succeeded");
    assert_eq!(result.failed, 0, "the skip is NOT a failure");
    assert!(
        result.errors.iter().any(|e| e == "Skipped: LLM not configured"),
        "errors must carry the canonical skip message; got {:?}",
        result.errors
    );

    // A system-level audit record must have been written so the skip is
    // visible in Diagnostics / Notification History.
    assert_eq!(
        count_system_error_audit(&conn),
        1,
        "exactly one system-level error audit row must be written on skip"
    );

    // The audit detail must mention Phase 3 and the actionable remedy so the
    // user understands what to do.
    let entries = audit_repo::get_generic_audit_entries(&conn, 10).expect("read audit entries");
    assert_eq!(entries.len(), 1, "one generic audit entry expected");
    let detail = entries[0].details.as_deref().unwrap_or("");
    assert!(
        detail.contains("Phase 3") && detail.contains("LLM not configured"),
        "audit detail must explain the Phase 3 skip; got {detail:?}"
    );
    assert!(
        detail.contains("Settings"),
        "audit detail must point the user to Settings; got {detail:?}"
    );
}

#[test]
fn phase3_pre_flight_proceeds_when_llm_is_configured() {
    // When an LLM IS configured, the pre-flight check returns `None` (the
    // phase should proceed normally) and writes NO system audit record.
    let conn = test_db();

    // Insert a minimal LLM config row so `has_config` returns true. The schema
    // is the singleton row `id = 1` with provider/endpoint_url/model_name +
    // api_key_encrypted (required for non-local providers per `has_config`).
    conn.execute(
        "INSERT INTO llm_config (id, provider, endpoint_url, model_name, \
         api_key_encrypted, temperature, skip_temperature, \
         max_concurrent_requests, request_delay_ms, context_window_tokens) \
         VALUES (1, 'openai', 'https://api.openai.com/v1', 'gpt-4o', \
         'enc-blob', 0.2, 0, 3, 500, 50000)",
        [],
    )
    .expect("insert llm config");

    let skip = translations_phase::check_llm_configured_or_skip(&conn, 5);
    assert!(skip.is_none(), "pre-flight must return None when LLM is configured");

    // No system-level audit record should have been written.
    assert_eq!(
        count_system_error_audit(&conn),
        0,
        "no system-level error audit row when LLM is configured"
    );
}
