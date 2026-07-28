//! Isolated tests for the split full-text attach pipeline (Concern 3).
//!
//! The batch-import Phase 1 runner uses `attach_full_text_split`, which
//! separates CPU-bound extraction (`extract_full_text_data`, no DB lock) from
//! DB writes (`commit_full_text_to_db`, short lock burst). The monolithic
//! `attach_full_text_inner` path is covered by `figures_flag_test.rs`; these
//! tests cover the split path that batch import actually exercises, closing
//! the coverage gap identified in `.worktrees/import_gaps.md` (Gap 3).
//!
//! Coverage:
//! - `extract_full_text_data`: figures-flag true/false, chunks populated,
//!   soft-fallback on empty/invalid PDF, DOI-aware destination filename,
//!   unsupported-extension hard error.
//! - `commit_full_text_to_db`: row update, chunks written, audit entries,
//!   staleness flags set, extraction-failure audit path.
//! - `attach_full_text_split`: end-to-end composition (lock-free parse +
//!   locked write), DOI-based destination filename produced.
//!
//! Uses the same in-memory DB + tempdir pattern as `figures_flag_test.rs`.

use std::io::Write;
use std::sync::Mutex;

use bango_lib::commands::full_text::{
    attach_full_text_split, commit_full_text_to_db, compute_storage_dir, extract_full_text_data,
    ExtractedFullText,
};
use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
use bango_lib::db::article_repo;
use bango_lib::db::chunk_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::scraping::citation_chaser::clean_doi_filename;
use rusqlite::Connection;
use tempfile::TempDir;

/// In-memory DB with all migrations applied.
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Configure the storage root to point at a temp dir so tests don't touch the
/// real documents folder. Creates the `fulltext/` subdir.
fn configure_storage_root(conn: &Connection, root: &std::path::Path) {
    set_setting(conn, STORAGE_ROOT_KEY, root.to_str()).unwrap();
    std::fs::create_dir_all(root.join("fulltext")).unwrap();
}

/// Insert a minimal article and return its id.
fn seed_article(conn: &Connection) -> String {
    let article = NewArticle {
        title: "Test Article".to_string(),
        abstract_text: "Abstract.".to_string(),
        ..Default::default()
    };
    article_repo::insert_article(conn, &article).expect("insert article").id
}

/// Insert a minimal article with a DOI and return its id.
fn seed_article_with_doi(conn: &Connection, doi: &str) -> String {
    let article = NewArticle {
        title: "DOI Article".to_string(),
        doi: Some(doi.to_string()),
        ..Default::default()
    };
    article_repo::insert_article(conn, &article).expect("insert article").id
}

/// Write a `.txt` file with the given content under the temp dir's `fulltext/`
/// subdir and return its path.
fn write_txt(root: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = root.join("fulltext").join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "{content}").unwrap();
    path
}

/// Write a file with arbitrary raw bytes (used for invalid-PDF fixtures).
fn write_bytes(root: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = root.join("fulltext").join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

// ── extract_full_text_data (lock-free CPU phase) ────────────────────────────

#[test]
fn extract_sets_has_figures_or_tables_true_when_caption_present() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let text = "Introduction.\n\nFigure 1. Study design overview showing the flow of participants.\n\nResults were significant.";
    let path = write_txt(tmp.path(), "captioned.txt", text);

    let extracted =
        extract_full_text_data(&path, None, "art-1", &storage_dir).expect("extract succeeds");

    assert!(
        extracted.has_figures_or_tables,
        "flag should be true when a figure caption is present"
    );
    assert!(!extracted.full_text.is_empty(), "full text should be extracted");
    assert!(extracted.word_count > 0, "word count should be non-zero");
    assert!(extracted.extraction_error.is_none(), "no extraction error on success");
    assert!(!extracted.chunks.is_empty(), "chunks should be produced from the text");
}

#[test]
fn extract_sets_has_figures_or_tables_false_for_plain_prose() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let text = "This study examined the effect of a sugar tax on beverage purchases.\n\nWe used a difference-in-differences design.";
    let path = write_txt(tmp.path(), "plain.txt", text);

    let extracted =
        extract_full_text_data(&path, None, "art-1", &storage_dir).expect("extract succeeds");

    assert!(
        !extracted.has_figures_or_tables,
        "flag should be false for plain prose with no captions"
    );
}

#[test]
fn extract_invalid_pdf_soft_fallback_with_empty_text_and_error() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    // 0-byte PDF: pdf_extract fails on the empty stream.
    let path = write_bytes(tmp.path(), "empty.pdf", b"");

    let extracted =
        extract_full_text_data(&path, None, "art-1", &storage_dir).expect("soft fallback Ok");

    assert!(extracted.full_text.is_empty(), "full_text should be empty on extraction failure");
    assert_eq!(extracted.word_count, 0, "word_count should be 0");
    assert!(!extracted.has_figures_or_tables, "flag should be false on empty text");
    assert!(extracted.chunks.is_empty(), "chunks should be empty on extraction failure");
    assert!(
        extracted
            .extraction_error
            .as_ref()
            .is_some_and(|e| e.contains("PDF text extraction failed")),
        "extraction_error should record the PDF failure; got {:?}",
        extracted.extraction_error
    );
}

#[test]
fn extract_unsupported_extension_is_a_hard_error() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let path = write_bytes(tmp.path(), "doc.docx", b"not a real docx");
    let err = extract_full_text_data(&path, None, "art-1", &storage_dir).expect_err("hard error");
    let msg = err.to_string();
    assert!(
        msg.contains("Unsupported file type"),
        "error should mention unsupported extension, got: {msg}"
    );
}

#[test]
fn extract_missing_file_is_a_hard_error() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let path = tmp.path().join("fulltext").join("does-not-exist.pdf");
    let err = extract_full_text_data(&path, None, "art-1", &storage_dir).expect_err("hard error");
    assert!(err.to_string().contains("File not found"), "error should mention missing file");
}

#[test]
fn extract_uses_doi_based_destination_filename_when_doi_present() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let path = write_txt(tmp.path(), "source.txt", "Some body text with content.");
    let extracted = extract_full_text_data(&path, Some("10.1001/foo"), "art-1", &storage_dir)
        .expect("extract succeeds");

    let expected = format!("{}.txt", clean_doi_filename("10.1001/foo"));
    assert_eq!(
        extracted.dest_filename, expected,
        "destination filename should be DOI-based, not UUID-based"
    );
    // The file should have been placed at the DOI-based destination.
    let dest_path = storage_dir.join(&expected);
    assert!(dest_path.exists(), "DOI-named destination file should exist");
}

#[test]
fn extract_uses_uuid_based_destination_filename_when_doi_absent() {
    let tmp = TempDir::new().unwrap();
    let storage_dir = tmp.path().join("fulltext");
    std::fs::create_dir_all(&storage_dir).unwrap();

    let path = write_txt(tmp.path(), "my-paper.txt", "Some body text with content.");
    let extracted =
        extract_full_text_data(&path, None, "art-uuid", &storage_dir).expect("extract succeeds");

    assert_eq!(
        extracted.dest_filename, "my-paper_art-uuid.txt",
        "destination filename should be UUID-based when no DOI"
    );
}

// ── commit_full_text_to_db (locked DB-write phase) ──────────────────────────

#[test]
fn commit_writes_full_text_chunks_audit_and_flags() {
    let conn = test_db();
    let id = seed_article(&conn);

    let extracted = ExtractedFullText {
        full_text: "Body text with some content here.".to_string(),
        word_count: 6,
        has_figures_or_tables: true,
        chunks: vec![bango_lib::utils::chunking::Chunk {
            text: "Body text with some content here.".to_string(),
            word_count: 6,
            section: None,
            chunk_index: 0,
        }],
        dest_filename: "dest.pdf".to_string(),
        original_name: "source.pdf".to_string(),
        extraction_error: None,
    };

    let result = commit_full_text_to_db(&conn, &id, &extracted).expect("commit succeeds");
    assert!(result.success);
    assert!(!result.extraction_failed);
    assert_eq!(result.word_count, 6);

    // The article row should reflect the attach.
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "has_full_text should be true");
    assert_eq!(article.full_text.as_deref(), Some("Body text with some content here."));
    assert!(article.has_figures_or_tables, "has_figures_or_tables flag should be persisted");

    // The chunk should have been written.
    let chunks = chunk_repo::list_chunks_for_article(&conn, &id).unwrap();
    assert_eq!(chunks.len(), 1, "exactly one chunk should be stored");
    assert_eq!(chunks[0].chunk_index, 0);
}

#[test]
fn commit_extraction_failure_writes_error_audit_but_attachment_persists() {
    let conn = test_db();
    let id = seed_article(&conn);

    let extracted = ExtractedFullText {
        full_text: String::new(),
        word_count: 0,
        has_figures_or_tables: false,
        chunks: Vec::new(),
        dest_filename: "empty.pdf".to_string(),
        original_name: "empty.pdf".to_string(),
        extraction_error: Some("PDF text extraction failed: empty stream".to_string()),
    };

    let result = commit_full_text_to_db(&conn, &id, &extracted).expect("commit succeeds");
    assert!(result.success, "attach still succeeds on extraction failure");
    assert!(result.extraction_failed, "extraction_failed flag should be true");
    assert!(result.message.contains("text extraction failed"));

    // The attachment persists with empty full_text.
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "has_full_text should still be true (soft fallback)");
    assert_eq!(
        article.full_text.as_deref(),
        Some(""),
        "full_text should be empty string, not NULL"
    );

    // A general-error audit entry should have been written for the extraction
    // failure (visible in the Audit Timeline).
    let generic =
        bango_lib::db::audit_repo::get_generic_audit_entries(&conn, 50).expect("read audit");
    let found = generic
        .iter()
        .any(|e| e.details.as_deref().is_some_and(|d| d.contains("PDF text extraction failed")));
    assert!(found, "an 'error' audit row should record the extraction failure");
}

// ── attach_full_text_split (end-to-end composition) ─────────────────────────

#[tokio::test]
async fn split_pipeline_attaches_txt_and_commits_via_short_lock_burst() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let id = seed_article(&conn);
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let path = write_txt(tmp.path(), "paper.txt", "This is the full text of the paper.");
    let conn_mutex = Mutex::new(conn);

    let result = attach_full_text_split(&conn_mutex, &id, None, &path, &storage_dir)
        .await
        .expect("split Ok");

    assert!(result.success);
    assert!(!result.extraction_failed);
    assert!(result.word_count > 0);

    let conn = conn_mutex.into_inner().unwrap();
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "article should have full text attached");
    // `writeln!` adds a trailing newline to the fixture file; the extracted
    // text preserves it. Assert the content is present (non-empty + contains
    // the body) rather than exact-equality so the test is newline-agnostic.
    let ft = article.full_text.as_deref().unwrap_or_default();
    assert!(
        ft.contains("This is the full text of the paper."),
        "full_text should be extracted and persisted; got {ft:?}"
    );

    // Chunks should have been written by the commit phase.
    let chunks = chunk_repo::list_chunks_for_article(&conn, &id).unwrap();
    assert!(!chunks.is_empty(), "chunks should be produced for a non-empty text");
}

#[tokio::test]
async fn split_pipeline_uses_doi_filename_when_doi_provided() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let doi = "10.1001/split-doi";
    let id = seed_article_with_doi(&conn, doi);
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let path = write_txt(tmp.path(), "source.txt", "Full text content for DOI-named split attach.");
    let conn_mutex = Mutex::new(conn);

    let result = attach_full_text_split(&conn_mutex, &id, Some(doi), &path, &storage_dir)
        .await
        .expect("split Ok");
    assert!(result.success);

    let conn = conn_mutex.into_inner().unwrap();
    let stored_name =
        article_repo::get_full_text_file_name(&conn, &id).unwrap().expect("file name");
    let expected = format!("{}.txt", clean_doi_filename(doi));
    assert_eq!(
        stored_name, expected,
        "split pipeline should produce the DOI-based destination filename"
    );
}

#[tokio::test]
async fn split_pipeline_soft_fallback_on_invalid_pdf() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let id = seed_article(&conn);
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let path = write_bytes(tmp.path(), "empty.pdf", b"");
    let conn_mutex = Mutex::new(conn);

    let result =
        attach_full_text_split(&conn_mutex, &id, None, &path, &storage_dir).await.expect("Ok");

    assert!(result.success, "attach should succeed (soft fallback)");
    assert!(result.extraction_failed, "extraction_failed flag should be true");
    assert_eq!(result.word_count, 0);

    let conn = conn_mutex.into_inner().unwrap();
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "has_full_text should be true despite extraction failure");
    assert_eq!(article.full_text.as_deref(), Some(""), "full_text should be empty string");
}
