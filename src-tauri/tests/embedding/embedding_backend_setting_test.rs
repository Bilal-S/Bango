//! DB round-trip and portability tests for the `embedding_backend` setting.
//!
//! The setting selects which backend generates embeddings (Configured
//! Provider vs Bango Local). It is machine-local: it is tied to this
//! machine's installed local-embedding components and must never travel with
//! a project backup (spec §8.1 `embedding_*` rule).

use bango_lib::db::app_settings_repo::{
    get_embedding_backend, is_project_portable, set_embedding_backend, set_setting,
    EMBEDDING_BACKEND_KEY,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::backend::EmbeddingBackend;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn backend_defaults_to_configured_provider() {
    let conn = test_db();
    assert_eq!(
        get_embedding_backend(&conn).unwrap(),
        EmbeddingBackend::ConfiguredProvider,
        "fresh DB must default to the cloud backend"
    );
}

#[test]
fn backend_round_trips_bango_local() {
    let conn = test_db();
    set_embedding_backend(&conn, EmbeddingBackend::BangoLocal).unwrap();
    assert_eq!(get_embedding_backend(&conn).unwrap(), EmbeddingBackend::BangoLocal);
    // And back to the cloud backend.
    set_embedding_backend(&conn, EmbeddingBackend::ConfiguredProvider).unwrap();
    assert_eq!(get_embedding_backend(&conn).unwrap(), EmbeddingBackend::ConfiguredProvider);
}

#[test]
fn backend_unknown_value_falls_back_to_default() {
    let conn = test_db();
    set_setting(&conn, EMBEDDING_BACKEND_KEY, Some("garbage-value")).unwrap();
    assert_eq!(
        get_embedding_backend(&conn).unwrap(),
        EmbeddingBackend::ConfiguredProvider,
        "corrupted row must never silently select the local backend"
    );
}

#[test]
fn backend_key_travels_with_project_backup() {
    assert!(
        is_project_portable(EMBEDDING_BACKEND_KEY),
        "embedding_backend is a project-level preference and must travel with a project backup"
    );
}

#[test]
fn backend_parse_exact_is_strict_for_command_arguments() {
    // The forgiving DB read (garbage -> default) vs the strict command
    // boundary (garbage -> None -> validation error).
    assert_eq!(
        EmbeddingBackend::parse_exact("configured_provider"),
        Some(EmbeddingBackend::ConfiguredProvider)
    );
    assert_eq!(EmbeddingBackend::parse_exact("bango_local"), Some(EmbeddingBackend::BangoLocal));
    assert_eq!(
        EmbeddingBackend::parse_exact("garbage-value"),
        None,
        "invalid selections must error, not fall back"
    );
    assert_eq!(EmbeddingBackend::parse_exact(""), None);
    // Round-trip with the canonical serialized form.
    for value in [EmbeddingBackend::ConfiguredProvider, EmbeddingBackend::BangoLocal] {
        assert_eq!(EmbeddingBackend::parse_exact(value.as_str()), Some(value));
    }
}
