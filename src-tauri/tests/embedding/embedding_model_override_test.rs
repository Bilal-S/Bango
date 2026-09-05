//! Coverage for the embedding-model override `app_settings` key (premium).
//!
//! Mirrors the `auto_translate_test.rs` round-trip shape. The override defaults
//! to `None` when the key is absent, round-trips a non-empty model name, trims
//! surrounding whitespace, and clears to `None` when set to an empty/whitespace
//! value.
use bango_lib::db::app_settings_repo::{
    get_embedding_model_override, set_embedding_model_override, EMBEDDING_MODEL_OVERRIDE_KEY,
};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn override_defaults_to_none_when_absent() {
    let conn = test_db();
    // Fresh database has no app_settings row for the override; default is None
    // (auto-detection active).
    assert_eq!(get_embedding_model_override(&conn).unwrap(), None);
}

#[test]
fn override_round_trips_a_model_name() {
    let conn = test_db();
    set_embedding_model_override(&conn, Some("text-embedding-3-large")).unwrap();
    assert_eq!(
        get_embedding_model_override(&conn).unwrap().as_deref(),
        Some("text-embedding-3-large")
    );
}

#[test]
fn override_trims_surrounding_whitespace_on_write() {
    let conn = test_db();
    set_embedding_model_override(&conn, Some("  nomic-embed-text  ")).unwrap();
    // The stored value is trimmed so downstream consumers never see a padded
    // model name (which would fail the provider's model lookup).
    assert_eq!(get_embedding_model_override(&conn).unwrap().as_deref(), Some("nomic-embed-text"));
}

#[test]
fn override_clears_to_none_when_set_to_empty() {
    let conn = test_db();
    set_embedding_model_override(&conn, Some("text-embedding-3-large")).unwrap();
    set_embedding_model_override(&conn, None).unwrap();
    assert_eq!(get_embedding_model_override(&conn).unwrap(), None);

    // The key should also be absent from the raw table (cleared, not stored
    // as an empty string) so the read helper's `None` is unambiguous.
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [EMBEDDING_MODEL_OVERRIDE_KEY],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    assert_eq!(raw, None, "cleared override must not leave an empty-string row");
}

#[test]
fn override_clears_to_none_when_set_to_whitespace() {
    let conn = test_db();
    set_embedding_model_override(&conn, Some("text-embedding-3-large")).unwrap();
    set_embedding_model_override(&conn, Some("   ")).unwrap();
    assert_eq!(get_embedding_model_override(&conn).unwrap(), None);
}

#[test]
fn override_is_not_in_project_portable_settings() {
    // The override is machine-local: it is tied to the provider configuration
    // on this machine and must NOT travel with a project backup (per the DOX
    // rule in AGENTS.md: any new app_settings key MUST trigger a review of
    // PROJECT_PORTABLE_SETTINGS).
    assert!(
        !bango_lib::db::app_settings_repo::is_project_portable(EMBEDDING_MODEL_OVERRIDE_KEY),
        "embedding_model_override must stay machine-local"
    );
}
