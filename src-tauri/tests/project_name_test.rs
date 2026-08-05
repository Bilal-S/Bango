//! Coverage for the user-editable project name (`app_settings` key).
//!
//! Mirrors the `auto_translate` round-trip test shape. The name is absent by
//! default (dashboard renders the "Project Dashboard" fallback), round-trips
//! through `set_project_name` / `get_project_name`, trims surrounding
//! whitespace, treats empty/whitespace as cleared (NULL storage), and is
//! hard-capped to `PROJECT_NAME_MAX_LEN` (60) chars as defense-in-depth.
//! Portability: the key is in `PROJECT_PORTABLE_SETTINGS` so it travels with
//! a project backup.
use bango_lib::db::app_settings_repo::{
    get_project_name, is_project_portable, set_project_name, PROJECT_NAME_KEY,
    PROJECT_NAME_MAX_LEN, PROJECT_PORTABLE_SETTINGS,
};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn project_name_defaults_to_none_when_absent() {
    let conn = test_db();
    // Fresh database has no app_settings row for the name; the dashboard
    // renders the "Project Dashboard" fallback in this state.
    assert_eq!(get_project_name(&conn).unwrap(), None);
}

#[test]
fn project_name_round_trips() {
    let conn = test_db();
    set_project_name(&conn, "UK Sugar Levy Review").unwrap();
    assert_eq!(get_project_name(&conn).unwrap().as_deref(), Some("UK Sugar Levy Review"));
}

#[test]
fn project_name_update_overwrites_previous_value() {
    let conn = test_db();
    set_project_name(&conn, "First Title").unwrap();
    set_project_name(&conn, "Second Title").unwrap();
    assert_eq!(get_project_name(&conn).unwrap().as_deref(), Some("Second Title"));
}

#[test]
fn project_name_trims_surrounding_whitespace_on_read() {
    let conn = test_db();
    set_project_name(&conn, "  padded name  ").unwrap();
    assert_eq!(get_project_name(&conn).unwrap().as_deref(), Some("padded name"));
}

#[test]
fn project_name_empty_string_clears_to_none() {
    let conn = test_db();
    set_project_name(&conn, "Was set").unwrap();
    set_project_name(&conn, "").unwrap();
    assert_eq!(get_project_name(&conn).unwrap(), None);
    // The stored row must be NULL (not the empty string), so the table stays
    // clean and `get_project_name`'s None-on-empty contract is upheld at the
    // storage layer.
    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM app_settings WHERE key = '{PROJECT_NAME_KEY}'"),
            [],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    assert_eq!(raw, None);
}

#[test]
fn project_name_whitespace_only_clears_to_none() {
    let conn = test_db();
    set_project_name(&conn, "   ").unwrap();
    assert_eq!(get_project_name(&conn).unwrap(), None);
}

#[test]
fn set_project_name_hard_caps_to_max_len() {
    let conn = test_db();
    // Build a string well over the cap.
    let overlong = "x".repeat(PROJECT_NAME_MAX_LEN + 25);
    set_project_name(&conn, &overlong).unwrap();
    let stored = get_project_name(&conn).unwrap().unwrap();
    assert_eq!(stored.chars().count(), PROJECT_NAME_MAX_LEN);
    assert!(stored.chars().all(|c| c == 'x'));
}

#[test]
fn set_project_name_hard_caps_multibyte_by_char_count() {
    let conn = test_db();
    // Each emoji is one char (one code point) but 4 bytes in UTF-8. The cap
    // must count chars, not bytes, so a browser's `maxlength` and the backend
    // agree on the limit.
    let emoji = "🎉";
    assert_eq!(emoji.len(), 4); // 4 bytes per emoji
    let overlong = emoji.repeat(PROJECT_NAME_MAX_LEN + 10);
    set_project_name(&conn, &overlong).unwrap();
    let stored = get_project_name(&conn).unwrap().unwrap();
    assert_eq!(stored.chars().count(), PROJECT_NAME_MAX_LEN);
}

#[test]
fn project_name_is_project_portable() {
    // The project name travels with a project backup so a restore on a new
    // machine keeps the user's title. Defense-in-depth: assert membership in
    // the allowlist directly.
    assert!(PROJECT_PORTABLE_SETTINGS.contains(&PROJECT_NAME_KEY));
    assert!(is_project_portable(PROJECT_NAME_KEY));
}
