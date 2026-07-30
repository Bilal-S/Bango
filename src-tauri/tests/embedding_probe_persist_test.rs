//! Regression tests for the Test Connection embedding-probe persistence path
//! (`commands::llm_config::persist_embedding_probe_to_conn`).
//!
//! Bug fixed: `persist_embedding_probe` previously hardcoded `dimensions = 0`,
//! dropping the `ProbeOutcome.dimensions` value returned by the probe. This
//! left `app_settings.embedding_dimensions` at 0 after Test Connection, which
//! gated `recall` off (`dimensions <= 0`) until the first
//! `generate_embeddings` call populated the real value. The standalone
//! `probe_embeddings` command was unaffected; only the Test Connection path
//! lost the dimensions.
//!
//! These tests exercise the extracted DB-write core directly (no Tauri
//! `State<DbState>` construction needed) against an in-memory SQLite DB with
//! the full migration chain.

use bango_lib::commands::llm_config::persist_embedding_probe_to_conn;
use bango_lib::db::app_settings_repo::{self, EmbeddingStatus};
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

#[test]
fn persist_probe_forwards_real_dimensions_when_enabled() {
    // Regression: the prior shape hardcoded 0 here, so `recall` (which gates
    // on `dimensions > 0`) returned empty until the first generate_embeddings.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let status = Some("enabled".to_string());
    let model = Some("text-embedding-3-small".to_string());
    persist_embedding_probe_to_conn(&conn, &status, &model, 1536).unwrap();

    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);
    assert_eq!(
        app_settings_repo::get_embedding_model(&conn).unwrap().as_deref(),
        Some("text-embedding-3-small")
    );
    // The key assertion: dimensions are forwarded, not hardcoded to 0.
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1536);
}

#[test]
fn persist_probe_stores_zero_dimensions_when_disabled() {
    // A disabled probe carries no vectors, so dimensions = 0 is correct (the
    // recall gate then returns empty, which is the right behavior when the
    // provider has no embedding capability).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let status = Some("disabled".to_string());
    persist_embedding_probe_to_conn(&conn, &status, &None, 0).unwrap();

    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Disabled);
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 0);
}

#[test]
fn persist_probe_overwrites_stale_dimensions_on_re_probe() {
    // Re-probing after a provider switch must overwrite the previously-stored
    // dimensions, not be stuck at the old value (or at 0 from the bug).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // First probe: OpenAI 3-small (1536).
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-small".to_string()),
        1536,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1536);

    // Second probe: switch to a 3072-dim model (e.g. 3-large).
    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("text-embedding-3-large".to_string()),
        3072,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 3072);
    assert_eq!(
        app_settings_repo::get_embedding_model(&conn).unwrap().as_deref(),
        Some("text-embedding-3-large")
    );
}

#[test]
fn persist_probe_disabled_then_enabled_round_trip() {
    // Covers the realistic sequence: probe a provider that has no embedding
    // support (disabled, dims 0), then switch providers and re-probe enabled.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    persist_embedding_probe_to_conn(&conn, &Some("disabled".to_string()), &None, 0).unwrap();
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 0);

    persist_embedding_probe_to_conn(
        &conn,
        &Some("enabled".to_string()),
        &Some("mistral-embed".to_string()),
        1024,
    )
    .unwrap();
    assert_eq!(app_settings_repo::get_embedding_status(&conn).unwrap(), EmbeddingStatus::Enabled);
    assert_eq!(app_settings_repo::get_embedding_dimensions(&conn).unwrap(), 1024);
}
