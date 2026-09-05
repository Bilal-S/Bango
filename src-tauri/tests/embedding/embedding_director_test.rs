//! Integration tests for the embedding director (`embedding::director`).
//!
//! Covers the `compute_work_list` eligibility + staleness logic against an
//! in-memory SQLite DB with the full migration chain. The base-condition
//! gates (LLM unconfigured, Disabled, Unknown, NoTargets, AllFresh) + the
//! staleness comparison + the `force` override are all exercised here.

use bango_lib::db::app_settings_repo::{self, EmbeddingStatus};
use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow, TITLE_ABSTRACT_CHUNK_INDEX};
use bango_lib::db::llm_config_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::director::{compute_work_list, EmbeddingScope, SkipReason};
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};
use rusqlite::Connection;

fn seed_article(conn: &Connection, id: &str, status: &str, title: &str, abstract_text: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, ?2, 'Author', ?3, ?4, 'test')",
        rusqlite::params![id, title, abstract_text, status],
    )
    .expect("seed article");
}

fn seed_config(conn: &Connection) {
    let cfg = LlmConfig {
        provider: LlmProvider::Openai,
        endpoint_url: "https://api.openai.com/v1".to_string(),
        api_key_encrypted: Some("sk-test".to_string()),
        model_name: "gpt-4o".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    };
    llm_config_repo::save_config(conn, &cfg).expect("save config");
}

fn set_enabled(conn: &Connection) {
    app_settings_repo::set_embedding_status(
        conn,
        EmbeddingStatus::Enabled,
        "text-embedding-3-small",
        1536,
    )
    .expect("set enabled");
}

// ── Base-condition gates ────────────────────────────────────────────────────

#[test]
fn director_llm_not_configured_returns_skip() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    // No config seeded.
    set_enabled(&conn);
    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.skip_reason, Some(SkipReason::LlmNotConfigured));
    assert!(list.rows.is_empty());
}

#[test]
fn director_disabled_returns_skip() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    app_settings_repo::set_embedding_status(&conn, EmbeddingStatus::Disabled, "", 0).unwrap();
    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.skip_reason, Some(SkipReason::Disabled));
}

#[test]
fn director_unknown_returns_skip() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    // Default status is Unknown (no set call).
    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.skip_reason, Some(SkipReason::UnknownNotProbed));
}

#[test]
fn director_no_targets_returns_skip() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    // No articles seeded.
    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.skip_reason, Some(SkipReason::NoTargets));
}

// ── Work-list computation ───────────────────────────────────────────────────

#[test]
fn director_emits_title_abstract_row_for_new_article() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "a1", "included", "Sugar Tax", "We studied obesity.");

    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert!(list.skip_reason.is_none() || list.skip_reason == Some(SkipReason::AllFresh));
    assert_eq!(list.rows.len(), 1, "one row: title+abstract (no full text)");
    assert_eq!(list.rows[0].chunk_index, TITLE_ABSTRACT_CHUNK_INDEX);
}

#[test]
fn director_skips_fresh_rows_when_hash_matches() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "a1", "included", "Title", "Abstract");

    // Pre-insert a matching embedding row so the hash matches AND the model
    // name matches the current `embedding_model` setting (`set_enabled` sets
    // "text-embedding-3-small"). A stored model mismatch marks the row stale
    // regardless of the hash (see `director_detects_model_mismatch_as_stale`).
    let text = bango_lib::embedding::text::format_embedding_text("Title", "Abstract", None);
    let hash = bango_lib::embedding::text::hash_text(&text);
    embedding_repo::insert_embedding(
        &conn,
        &NewEmbeddingRow {
            article_id: "a1",
            chunk_index: TITLE_ABSTRACT_CHUNK_INDEX,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: &hash,
            model_name: "text-embedding-3-small",
            provider: "p",
            generated_at: 1,
        },
    )
    .unwrap();

    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.skip_reason, Some(SkipReason::AllFresh));
    assert!(list.rows.is_empty(), "fresh rows are skipped");
    assert_eq!(list.skipped_fresh, 1);
}

#[test]
fn director_detects_model_mismatch_as_stale() {
    // A stored row whose `model_name` differs from the current
    // `embedding_model` setting is marked stale regardless of the input_hash.
    // This is the fix for the silent zero-results bug: switching embedding
    // models (e.g. text-embedding-3-small → text-embedding-3-large) changes
    // the vector dimensions, so recall filters out every old row. Without
    // this check the director reports all rows "fresh" by hash, Phase B
    // produces zero work, coverage reads 100%, and every search returns zero
    // hits with no explanation.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn); // sets embedding_model = "text-embedding-3-small"
    seed_article(&conn, "a1", "included", "Title", "Abstract");

    // Insert a row with a matching hash BUT a different model name
    // ("text-embedding-3-large"). The hash alone would mark it fresh; the
    // model mismatch must override that.
    let text = bango_lib::embedding::text::format_embedding_text("Title", "Abstract", None);
    let hash = bango_lib::embedding::text::hash_text(&text);
    embedding_repo::insert_embedding(
        &conn,
        &NewEmbeddingRow {
            article_id: "a1",
            chunk_index: TITLE_ABSTRACT_CHUNK_INDEX,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: &hash,
            model_name: "text-embedding-3-large",
            provider: "p",
            generated_at: 1,
        },
    )
    .unwrap();

    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.rows.len(), 1, "model mismatch => needs re-embedding");
    assert_ne!(list.skip_reason, Some(SkipReason::AllFresh));
}

#[test]
fn director_force_overrides_fresh_hash() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "a1", "included", "Title", "Abstract");

    // Pre-insert a matching row.
    let text = bango_lib::embedding::text::format_embedding_text("Title", "Abstract", None);
    let hash = bango_lib::embedding::text::hash_text(&text);
    embedding_repo::insert_embedding(
        &conn,
        &NewEmbeddingRow {
            article_id: "a1",
            chunk_index: TITLE_ABSTRACT_CHUNK_INDEX,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: &hash,
            model_name: "m",
            provider: "p",
            generated_at: 1,
        },
    )
    .unwrap();

    // force=true marks it for embedding regardless of the hash.
    let list =
        compute_work_list(&conn, &EmbeddingScope { force: true, ..Default::default() }).unwrap();
    assert_eq!(list.rows.len(), 1, "force overrides the fresh hash");
}

#[test]
fn director_status_filter_defaults_to_included() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "inc", "included", "T1", "A1");
    seed_article(&conn, "wk", "working", "T2", "A2");

    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.total_articles, 1, "default status filter is 'included'");
    assert_eq!(list.rows[0].article_id, "inc");
}

#[test]
fn director_explicit_article_ids_override_status_filter() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "inc", "included", "T1", "A1");
    seed_article(&conn, "wk", "working", "T2", "A2");

    let list = compute_work_list(
        &conn,
        &EmbeddingScope { article_ids: Some(vec!["wk".to_string()]), ..Default::default() },
    )
    .unwrap();
    assert_eq!(list.total_articles, 1);
    assert_eq!(list.rows[0].article_id, "wk");
}

#[test]
fn director_detects_hash_mismatch_as_stale() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_config(&conn);
    set_enabled(&conn);
    seed_article(&conn, "a1", "included", "New Title", "Abstract");

    // Insert a row with a STALE hash (the title changed since the last embed).
    embedding_repo::insert_embedding(
        &conn,
        &NewEmbeddingRow {
            article_id: "a1",
            chunk_index: TITLE_ABSTRACT_CHUNK_INDEX,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: "stale-hash-that-does-not-match",
            model_name: "m",
            provider: "p",
            generated_at: 1,
        },
    )
    .unwrap();

    let list = compute_work_list(&conn, &EmbeddingScope::default()).unwrap();
    assert_eq!(list.rows.len(), 1, "hash mismatch => needs re-embedding");
}
