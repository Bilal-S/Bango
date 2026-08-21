//! Integration tests for the async chunk-rebuild pipeline
//! (`commands::full_text::rebuild_chunks_loop` + pure cascade helpers).
//!
//! The loop is driven directly with an in-memory `Connection` behind a
//! `Mutex` (mirroring `batch_import_test.rs`); `app_handle: None` = test
//! mode, so no Tauri app is required. The embedding cascade itself needs a
//! live `AppHandle` + orchestrator and is covered by the runner's own tests;
//! here we cover its pure scope/summary helpers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bango_lib::commands::full_text::{
    embedding_cascade_scopes, embedding_summary_line, rebuild_chunks_loop, rebuild_summary_message,
    RebuildChunksProgress, RebuildChunksState,
};
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::runner::EmbeddingRunReport;
use bango_lib::utils::chunking::Chunk;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");
    conn
}

fn insert_article(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, 'Test Article', 'Author', 'Abstract text', 'working', 'test.ris')",
        rusqlite::params![id],
    )
    .expect("Insert article failed");
}

/// Mark an article as having full text stored under `file_name` (raw SQL so
/// the test controls `full_text_file_name` + `is_translated` directly).
fn mark_full_text(conn: &Connection, id: &str, file_name: Option<&str>, is_translated: bool) {
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = 'body text', \
         full_text_file_name = ?2, is_translated = ?3 WHERE id = ?1",
        rusqlite::params![id, file_name, if is_translated { 1 } else { 0 }],
    )
    .expect("mark full text failed");
}

/// Seed English chunk rows the way the translation worker leaves them.
fn seed_english_chunks(conn: &Connection, id: &str) {
    let chunks = vec![Chunk {
        chunk_index: 0,
        section: Some("Methods".to_string()),
        text: "translated English methods text".to_string(),
        word_count: 4,
    }];
    chunk_repo::replace_chunks_for_article(conn, id, &chunks).expect("seed chunks");
}

/// Seed one embedding row (sentinel -1 title/abstract row) for an article.
fn seed_embedding(conn: &Connection, id: &str) {
    embedding_repo::insert_embedding(
        conn,
        &NewEmbeddingRow {
            article_id: id,
            chunk_index: -1,
            embedding: &[0.1, 0.2, 0.3],
            dimensions: 3,
            input_hash: "stale-hash",
            model_name: "test-model",
            provider: "openai",
            generated_at: 0,
        },
    )
    .expect("seed embedding");
}

fn count_embeddings(conn: &Connection, id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM article_embeddings WHERE article_id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    )
    .expect("count embeddings")
}

fn count_error_audits(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM audit_entries WHERE action = 'error'", [], |row| {
        row.get(0)
    })
    .expect("count error audits")
}

fn state_harness() -> (Arc<AtomicBool>, Arc<Mutex<RebuildChunksProgress>>) {
    let state = RebuildChunksState::default();
    (state.cancel_handle(), state.progress_handle())
}

fn snapshot(progress: &Arc<Mutex<RebuildChunksProgress>>) -> RebuildChunksProgress {
    progress.lock().unwrap().clone()
}

fn write_txt(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        "## Methods\nSome body text with enough content to chunk.\n\n## Results\nMore body text here for the second section.",
    )
    .expect("write txt");
    path
}

// ── Core loop ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn loop_chunks_txt_articles_and_reports_progress() {
    let conn = setup_db();
    insert_article(&conn, "art-1");
    let tmp = tempfile::tempdir().expect("tempdir");
    write_txt(tmp.path(), "art-1.txt");
    mark_full_text(&conn, "art-1", Some("art-1.txt"), false);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    assert_eq!(candidates.len(), 1);

    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    let outcome =
        rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
            .await
            .expect("loop ok");

    assert_eq!(outcome.chunked_ids, vec!["art-1".to_string()]);
    assert!(outcome.translated_ids.is_empty());
    let snap = snapshot(&progress);
    assert_eq!(snap.total, 1);
    assert_eq!(snap.completed, 1);
    assert_eq!(snap.chunked, 1);
    assert_eq!(snap.failed, 0);
    assert_eq!(snap.percent, 100);
    assert_eq!(snap.phase, "chunks");
    // Chunks were actually written from the .txt sections.
    let listed = chunk_repo::list_chunks_for_article(&conn_mutex.lock().unwrap(), "art-1")
        .expect("list chunks");
    assert!(!listed.is_empty(), "chunks must be populated");
}

#[tokio::test]
async fn loop_missing_file_logs_error_and_counts_failed() {
    let conn = setup_db();
    insert_article(&conn, "art-2");
    mark_full_text(&conn, "art-2", Some("gone.pdf"), false);
    let before = count_error_audits(&conn);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    let tmp = tempfile::tempdir().expect("tempdir");
    let outcome =
        rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
            .await
            .expect("loop ok");

    assert!(outcome.chunked_ids.is_empty());
    let snap = snapshot(&progress);
    assert_eq!(snap.failed, 1);
    assert_eq!(snap.errors.len(), 1);
    assert!(snap.errors[0].contains("art-2"), "error message embeds the article id");
    assert!(snap.errors[0].contains("File not found"));
    // The previously-silent failure mode now writes an audit row.
    let after = count_error_audits(&conn_mutex.lock().unwrap());
    assert_eq!(after, before + 1, "log_error audit row must be written");
}

#[tokio::test]
async fn loop_missing_file_name_logs_error_and_counts_failed() {
    let conn = setup_db();
    insert_article(&conn, "art-3");
    mark_full_text(&conn, "art-3", None, false);
    let before = count_error_audits(&conn);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    let tmp = tempfile::tempdir().expect("tempdir");
    let outcome =
        rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
            .await
            .expect("loop ok");

    assert!(outcome.chunked_ids.is_empty());
    let snap = snapshot(&progress);
    assert_eq!(snap.failed, 1);
    assert!(snap.errors[0].contains("art-3"));
    assert!(snap.errors[0].contains("full_text_file_name"));
    let after = count_error_audits(&conn_mutex.lock().unwrap());
    assert_eq!(after, before + 1, "log_error audit row must be written");
}

#[tokio::test]
async fn loop_skips_translated_articles_preserving_chunks() {
    let conn = setup_db();
    // Translated article with English chunks + a normal article.
    insert_article(&conn, "translated");
    insert_article(&conn, "plain");
    let tmp = tempfile::tempdir().expect("tempdir");
    write_txt(tmp.path(), "translated.txt");
    write_txt(tmp.path(), "plain.txt");
    mark_full_text(&conn, "translated", Some("translated.txt"), true);
    mark_full_text(&conn, "plain", Some("plain.txt"), false);
    seed_english_chunks(&conn, "translated");

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    let outcome =
        rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
            .await
            .expect("loop ok");

    // The translated article is skipped, never re-chunked...
    assert_eq!(outcome.translated_ids, vec!["translated".to_string()]);
    assert_eq!(outcome.chunked_ids, vec!["plain".to_string()]);
    let snap = snapshot(&progress);
    assert_eq!(snap.skipped_translated, 1);
    assert_eq!(snap.chunked, 1);
    assert_eq!(snap.failed, 0);
    // ...and its English chunk rows are byte-identical.
    let conn = conn_mutex.lock().unwrap();
    let listed = chunk_repo::list_chunks_for_article(&conn, "translated").expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].text, "translated English methods text");
}

#[tokio::test]
async fn loop_cancel_token_stops_processing() {
    let conn = setup_db();
    insert_article(&conn, "a");
    insert_article(&conn, "b");
    let tmp = tempfile::tempdir().expect("tempdir");
    write_txt(tmp.path(), "a.txt");
    write_txt(tmp.path(), "b.txt");
    mark_full_text(&conn, "a", Some("a.txt"), false);
    mark_full_text(&conn, "b", Some("b.txt"), false);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    // Pre-set the token: the loop must break before processing anything.
    cancel.store(true, Ordering::Relaxed);

    let outcome =
        rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
            .await
            .expect("loop ok");

    assert!(outcome.chunked_ids.is_empty());
    let snap = snapshot(&progress);
    assert!(snap.is_cancelled);
    assert_eq!(snap.completed, 0);
    assert_eq!(snap.total, 2);
}

#[tokio::test]
async fn loop_deletes_stale_embeddings_for_rechunked_articles() {
    let conn = setup_db();
    insert_article(&conn, "art-4");
    let tmp = tempfile::tempdir().expect("tempdir");
    write_txt(tmp.path(), "art-4.txt");
    mark_full_text(&conn, "art-4", Some("art-4.txt"), false);
    seed_embedding(&conn, "art-4");
    assert_eq!(count_embeddings(&conn, "art-4"), 1);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    let conn_mutex = Mutex::new(conn);
    let (cancel, progress) = state_harness();
    rebuild_chunks_loop(&conn_mutex, tmp.path(), &candidates, &cancel, &progress, None)
        .await
        .expect("loop ok");

    // Re-chunking invalidated the embedding rows (the cascade regenerates).
    assert_eq!(count_embeddings(&conn_mutex.lock().unwrap(), "art-4"), 0);
}

// ── Pure cascade helpers ───────────────────────────────────────────────────

#[test]
fn embedding_cascade_scopes_maps_ids_to_scopes() {
    let (regen, backfill) = embedding_cascade_scopes(&[], &[]);
    assert!(regen.is_none(), "empty chunked set -> no regenerate call");
    assert!(backfill.is_none(), "empty translated set -> no backfill call");

    let chunked = vec!["a".to_string(), "b".to_string()];
    let translated = vec!["t".to_string()];
    let (regen, backfill) = embedding_cascade_scopes(&chunked, &translated);
    let regen = regen.expect("non-empty chunked set produces a scope");
    assert_eq!(regen.article_ids, Some(chunked));
    assert_eq!(regen.status_filter, None, "explicit ids must override status filtering");
    assert!(!regen.force, "deleted rows drive regeneration; force stays false");
    let backfill = backfill.expect("non-empty translated set produces a scope");
    assert_eq!(backfill.article_ids, Some(translated));
    assert!(!backfill.force, "translated backfill must never force re-embeds");

    // Translated-only run: regenerate scope absent, backfill present.
    let (regen, backfill) = embedding_cascade_scopes(&[], &["t".to_string()]);
    assert!(regen.is_none());
    assert!(backfill.is_some());
}

fn report(skip_reason: Option<&str>, generated: usize, model: &str) -> EmbeddingRunReport {
    EmbeddingRunReport {
        generated,
        skipped: 0,
        errors: 0,
        status: "enabled".to_string(),
        model: model.to_string(),
        skip_reason: skip_reason.map(str::to_string),
    }
}

#[test]
fn embedding_summary_line_maps_skip_reasons() {
    // LLM not configured -> friendly skip line (dominates the other report).
    let line = embedding_summary_line(
        Some(&report(Some("LlmNotConfigured"), 0, "")),
        Some(&report(Some("LlmNotConfigured"), 0, "")),
    );
    assert_eq!(line.as_deref(), Some("Embeddings skipped: LLM not configured"));

    // Provider cannot embed -> friendly skip line.
    let line = embedding_summary_line(Some(&report(Some("Disabled"), 0, "")), None);
    assert_eq!(line.as_deref(), Some("Embeddings skipped: provider does not support embeddings"));

    // Successes -> counts + model.
    let line = embedding_summary_line(
        Some(&report(None, 12, "text-embedding-3-small")),
        Some(&report(None, 3, "text-embedding-3-small")),
    );
    assert_eq!(
        line.as_deref(),
        Some("Embeddings: 12 regenerated, 3 backfilled (text-embedding-3-small)")
    );

    // Nothing ran -> no line.
    assert_eq!(embedding_summary_line(None, None), None);
}

#[test]
fn candidates_query_returns_translated_flag() {
    let conn = setup_db();
    insert_article(&conn, "plain");
    insert_article(&conn, "translated");
    insert_article(&conn, "no-fulltext");
    mark_full_text(&conn, "plain", Some("plain.pdf"), false);
    mark_full_text(&conn, "translated", Some("translated.pdf"), true);

    let candidates = chunk_repo::get_full_text_chunk_candidates(&conn).expect("candidates");
    assert_eq!(candidates.len(), 2, "only has_full_text = 1 articles");
    let plain = candidates.iter().find(|c| c.id == "plain").expect("plain present");
    let translated = candidates.iter().find(|c| c.id == "translated").expect("translated present");
    assert!(!plain.is_translated);
    assert_eq!(plain.file_name.as_deref(), Some("plain.pdf"));
    assert!(translated.is_translated, "translated flag must round-trip");
}

#[test]
fn summary_message_includes_translated_skip_note() {
    let msg = rebuild_summary_message(2, 1, 3, false);
    assert!(msg.contains("2 chunked"));
    assert!(msg.contains("1 failed"));
    assert!(msg.contains("3 skipped (translated, English chunks preserved)"));
    let cancelled = rebuild_summary_message(1, 0, 0, true);
    assert!(cancelled.starts_with("Cancelled after "), "cancelled runs are labelled");
}
