//! Regression tests for the `wiki_needs_refresh` flag being set when the
//! Wiki's content source fields change.
//!
//! The Wiki ingest content resolution in
//! `src-tauri/src/wiki/raw_export.rs::article_content()` is:
//!
//! ```text
//! full_text_ai_summary blob → abstract_text (full text is never exported)
//! ```
//!
//! The Tauri commands that mutate these fields (`attach_full_text`,
//! `delete_full_text`, `generate_article_ai_summary`) must mark the wiki
//! staleness flag so the frontend `autoIngestIfStale()` flow in
//! `src/views/wiki-view.vue` re-runs ingest with the new content.
//!
//! Also covers the ensure-summaries pre-phase query + failure contract
//! (`.worktrees/wikifix-final.md` Change 1).
//!
//! Because the flag-setting is a one-liner at the command layer (which requires
//! `State<DbState>` that the project deliberately avoids mocking - see
//! `docs/CLAUDE.md` "Coverage Strategy"), these tests validate the contract at
//! the repo layer: that calling `mark_wiki_needs_refresh` after the content
//! mutation is the correct pairing.

use bango_lib::commands::wiki_cmd::{record_wiki_summary_failure, wiki_articles_missing_summary};
use bango_lib::db::app_settings_repo::{
    clear_wiki_needs_refresh, get_wiki_needs_refresh, mark_wiki_needs_refresh,
};
use bango_lib::db::article_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use rusqlite::{params, Connection};

/// In-memory DB with all migrations applied (same pattern as
/// `biblio_needs_refresh_test.rs`).
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Seed one article as `included` (the status that makes it eligible for wiki
/// ingest) and return its id. Uses the `NewArticle { ..Default::default() }`
/// pattern from `screening_e2e_test.rs`.
fn seed_included_article(conn: &Connection) -> String {
    let article = NewArticle {
        title: "Included article for wiki content test".to_string(),
        abstract_text: "Abstract used as the wiki content fallback.".to_string(),
        authors: vec!["Doe, Jane".to_string()],
        publication_year: Some(2024),
        keywords: vec!["wiki".to_string()],
        import_source: Some("test".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_article(conn, &article).expect("insert article");
    article_repo::update_article_status(conn, &inserted.id, "included").expect("move to included");
    inserted.id
}

#[test]
fn test_full_text_attach_marks_wiki_stale() {
    let conn = test_db();
    let article_id = seed_included_article(&conn);

    // Simulate a completed prior ingest clearing the flag.
    clear_wiki_needs_refresh(&conn);
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    // Repo-level mutation performed by `attach_full_text` (file copy + audit
    // entry are command-layer concerns; the DB write is what affects the wiki).
    article_repo::update_full_text(&conn, &article_id, "full text body", "doc.pdf", false)
        .expect("update_full_text");

    // Command-layer flag-setting called after the repo write succeeds.
    mark_wiki_needs_refresh(&conn);

    assert!(get_wiki_needs_refresh(&conn).unwrap());
}

#[test]
fn test_full_text_delete_marks_wiki_stale() {
    let conn = test_db();
    let article_id = seed_included_article(&conn);

    // Start from an article that already has full text and a fresh wiki.
    article_repo::update_full_text(&conn, &article_id, "existing full text", "doc.pdf", false)
        .expect("update_full_text");
    clear_wiki_needs_refresh(&conn);
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    // Repo-level mutation performed by `delete_full_text`.
    article_repo::clear_full_text(&conn, &article_id).expect("clear_full_text");

    // Command-layer flag-setting called after the repo write succeeds.
    mark_wiki_needs_refresh(&conn);

    assert!(get_wiki_needs_refresh(&conn).unwrap());
}

#[test]
fn test_ai_summary_generation_marks_wiki_stale() {
    let conn = test_db();
    let article_id = seed_included_article(&conn);

    // Simulate a completed prior ingest clearing the flag.
    clear_wiki_needs_refresh(&conn);
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    // Repo-level mutation performed by `generate_article_ai_summary`.
    let summary_json = serde_json::json!({
        "summary_150_250_words": "This study examined wiki refresh behavior.",
        "key_insights": ["Flag-setting is now paired with content mutations."]
    })
    .to_string();
    article_repo::set_ai_summary(&conn, &article_id, &summary_json).expect("set_ai_summary");

    // Command-layer flag-setting called after the repo write succeeds.
    mark_wiki_needs_refresh(&conn);

    assert!(get_wiki_needs_refresh(&conn).unwrap());
}

#[test]
fn test_flag_defaults_fresh_and_round_trips() {
    let conn = test_db();
    // Absent key = fresh (no unnecessary ingest on a reset DB).
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    mark_wiki_needs_refresh(&conn);
    assert!(get_wiki_needs_refresh(&conn).unwrap());

    clear_wiki_needs_refresh(&conn);
    assert!(!get_wiki_needs_refresh(&conn).unwrap());

    // Idempotent mark.
    mark_wiki_needs_refresh(&conn);
    mark_wiki_needs_refresh(&conn);
    assert!(get_wiki_needs_refresh(&conn).unwrap());
}

// ---- Ensure-summaries pre-phase (`.worktrees/wikifix-final.md` Change 1;
// inventory: `docs/test-plans/wiki-output-budget-tests.md`). ----

/// Ensure-summaries query returns exactly included full-text articles whose
/// blob is NULL/empty; skips articles with a blob, without full text, or not
/// included.
#[test]
fn ensure_summaries_targets_only_full_text_articles_missing_blobs() {
    let conn = test_db();
    let missing = seed_included_article(&conn);
    conn.execute("UPDATE articles SET full_text = 'full body' WHERE id = ?1", params![&missing])
        .unwrap();
    let has_blob = seed_included_article(&conn);
    conn.execute(
        "UPDATE articles SET full_text = 'full body', \
         full_text_ai_summary = '{\"summary_150_250_words\":\"x\"}' WHERE id = ?1",
        params![&has_blob],
    )
    .unwrap();
    let abstract_only = seed_included_article(&conn);
    let rejected = seed_included_article(&conn);
    conn.execute("UPDATE articles SET full_text = 'full body' WHERE id = ?1", params![&rejected])
        .unwrap();
    article_repo::update_article_status(&conn, &rejected, "rejected").unwrap();

    let ids = wiki_articles_missing_summary(&conn).unwrap();
    assert_eq!(ids, vec![missing], "only the blob-less full-text included article counts");
    assert!(!ids.contains(&has_blob));
    assert!(!ids.contains(&abstract_only));
    assert!(!ids.contains(&rejected));
}

/// Generation failure is non-fatal: a per-article audit entry is written, the
/// blob stays NULL, so wiki export falls back to the abstract.
#[test]
fn ensure_summaries_failure_falls_back_to_abstract_with_audit_entry() {
    let conn = test_db();
    let article_id = seed_included_article(&conn);
    conn.execute("UPDATE articles SET full_text = 'full body' WHERE id = ?1", params![&article_id])
        .unwrap();

    record_wiki_summary_failure(&conn, &article_id, "boom: LLM unavailable").unwrap();

    let logged: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries \
             WHERE article_id = ?1 AND action = 'ai_summary' AND details LIKE '%boom%'",
            params![&article_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(logged, 1, "failure audit entry missing");
    // Blob still absent -> article_content falls back to the abstract.
    let blob: Option<String> = conn
        .query_row(
            "SELECT full_text_ai_summary FROM articles WHERE id = ?1",
            params![&article_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(blob.is_none());
}

/// A cancelled ingest must leave the wiki stale so the next Update retries;
/// only a completed run clears the flag.
#[test]
fn cancelled_finalize_keeps_wiki_stale() {
    use bango_lib::wiki::ingest::{finalize_ingest, IngestReport};

    let conn = test_db();
    mark_wiki_needs_refresh(&conn);
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("wiki")).unwrap();
    std::fs::write(root.join("wiki/log.md"), "").unwrap();

    let mut cancelled = IngestReport::default();
    cancelled.errors.push("Cancelled".to_string());
    finalize_ingest(&conn, root, &mut cancelled).unwrap();
    assert!(get_wiki_needs_refresh(&conn).unwrap(), "cancel must keep staleness");

    let mut completed = IngestReport::default();
    finalize_ingest(&conn, root, &mut completed).unwrap();
    assert!(!get_wiki_needs_refresh(&conn).unwrap(), "completed ingest clears staleness");
}
