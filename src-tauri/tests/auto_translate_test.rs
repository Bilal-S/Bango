//! Coverage for the experimental auto-translate `app_settings` toggle.
//!
//! Mirrors the `biblio_needs_refresh` round-trip test shape. The toggle
//! defaults to `true` (enabled) when the key is absent, and round-trips both
//! `true` and `false` through `set_auto_translate` / `get_auto_translate`.
//! Garbage values fall back to the default so a corrupted row never silently
//! disables the feature.
use bango_lib::db::app_settings_repo::{
    get_auto_translate, set_auto_translate, AUTO_TRANSLATE_KEY,
};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn auto_translate_defaults_to_true_when_absent() {
    let conn = test_db();
    // Fresh database has no app_settings row for the toggle; default is enabled.
    assert!(get_auto_translate(&conn).unwrap());
}

#[test]
fn auto_translate_round_trips_false() {
    let conn = test_db();
    set_auto_translate(&conn, false).unwrap();
    assert!(!get_auto_translate(&conn).unwrap());

    // The persisted value must be the literal "false".
    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM app_settings WHERE key = '{AUTO_TRANSLATE_KEY}'"),
            [],
            |r| r.get(0),
        )
        .ok();
    assert_eq!(raw.as_deref(), Some("false"));
}

#[test]
fn auto_translate_round_trips_true() {
    let conn = test_db();
    // Disable first, then re-enable to prove the upsert path updates the row.
    set_auto_translate(&conn, false).unwrap();
    set_auto_translate(&conn, true).unwrap();
    assert!(get_auto_translate(&conn).unwrap());

    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM app_settings WHERE key = '{AUTO_TRANSLATE_KEY}'"),
            [],
            |r| r.get(0),
        )
        .ok();
    assert_eq!(raw.as_deref(), Some("true"));
}

#[test]
fn auto_translate_garbage_value_falls_back_to_default() {
    let conn = test_db();
    // Write a garbage value directly so we can confirm the read helper treats
    // anything other than "true"/"false" as the default (enabled).
    conn.execute(
        &format!("INSERT INTO app_settings (key, value) VALUES ('{AUTO_TRANSLATE_KEY}', 'yes')"),
        [],
    )
    .unwrap();
    assert!(get_auto_translate(&conn).unwrap());
}
