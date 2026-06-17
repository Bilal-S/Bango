//! Coverage for db::summary_repo (save/get/clear upsert logic).
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::summary_repo;

#[test]
fn get_summary_returns_none_when_empty() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    assert!(summary_repo::get_summary(&conn).expect("get").is_none());
}

#[test]
fn save_then_get_returns_values() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    summary_repo::save_summary(&conn, "review text", "apa", "2026-01-01T00:00:00Z").expect("save");

    let s = summary_repo::get_summary(&conn).expect("get").expect("some");
    assert_eq!(s.summary_text, "review text");
    assert_eq!(s.citation_style, "apa");
    assert_eq!(s.generated_at, "2026-01-01T00:00:00Z");
}

#[test]
fn save_is_upsert_overwriting_existing() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    summary_repo::save_summary(&conn, "first", "apa", "2026-01-01").expect("save1");
    summary_repo::save_summary(&conn, "second", "vancouver", "2026-02-01").expect("save2");

    let s = summary_repo::get_summary(&conn).expect("get").expect("some");
    assert_eq!(s.summary_text, "second");
    assert_eq!(s.citation_style, "vancouver");
    assert_eq!(s.generated_at, "2026-02-01");
}

#[test]
fn clear_summary_removes_row() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    summary_repo::save_summary(&conn, "x", "apa", "2026-01-01").expect("save");
    assert!(summary_repo::get_summary(&conn).expect("get").is_some());

    summary_repo::clear_summary(&conn).expect("clear");
    assert!(summary_repo::get_summary(&conn).expect("get").is_none());
}

#[test]
fn clear_summary_when_empty_is_noop() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    summary_repo::clear_summary(&conn).expect("clear empty");
    assert!(summary_repo::get_summary(&conn).expect("get").is_none());
}
