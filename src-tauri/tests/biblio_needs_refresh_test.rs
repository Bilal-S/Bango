use bango_lib::db::app_settings_repo::{
    clear_biblio_needs_refresh, get_biblio_needs_refresh, mark_biblio_needs_refresh,
};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

#[test]
fn test_needs_refresh_defaults_to_false_when_absent() {
    let conn = test_db();
    // Fresh database has no app_settings row for the flag.
    assert!(!get_biblio_needs_refresh(&conn).unwrap());
}

#[test]
fn test_mark_and_clear_round_trip() {
    let conn = test_db();

    // Mark stale.
    mark_biblio_needs_refresh(&conn);
    assert!(get_biblio_needs_refresh(&conn).unwrap());

    // Clear it (as biblio_normalize does after commit).
    clear_biblio_needs_refresh(&conn);
    assert!(!get_biblio_needs_refresh(&conn).unwrap());

    // Marking again flips it back to true (idempotent upsert).
    mark_biblio_needs_refresh(&conn);
    mark_biblio_needs_refresh(&conn);
    assert!(get_biblio_needs_refresh(&conn).unwrap());
}

#[test]
fn test_persisted_value_in_app_settings() {
    let conn = test_db();

    mark_biblio_needs_refresh(&conn);
    let raw: Option<String> = conn
        .query_row("SELECT value FROM app_settings WHERE key = 'biblio_needs_refresh'", [], |r| {
            r.get(0)
        })
        .ok();
    assert_eq!(raw.as_deref(), Some("true"));

    clear_biblio_needs_refresh(&conn);
    let raw: Option<String> = conn
        .query_row("SELECT value FROM app_settings WHERE key = 'biblio_needs_refresh'", [], |r| {
            r.get(0)
        })
        .ok();
    assert_eq!(raw.as_deref(), Some("false"));
}
