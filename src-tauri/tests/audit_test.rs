use bango_lib::db::audit_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

fn insert_test_article(conn: &rusqlite::Connection, id: &str, status: &str) {
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES (?1, ?2, 'Test Article', 'Test abstract', '[\"Author, A\"]')",
        rusqlite::params![id, status],
    ).expect("Failed to insert test article");
}

#[test]
fn test_create_and_retrieve_audit_entry() {
    let conn = setup_db();
    insert_test_article(&conn, "art-1", "duplicate");

    audit_repo::create_entry(
        &conn,
        "art-1",
        "import",
        None,
        None,
        Some("Imported from test.ris"),
        "system",
    )
    .expect("Failed to create audit entry");

    let entries = audit_repo::get_audit_trail(&conn, "art-1").expect("Failed to get audit trail");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action.as_str(), "import");
    assert_eq!(entries[0].source.as_str(), "system");
    assert_eq!(entries[0].details.as_deref(), Some("Imported from test.ris"));
}

#[test]
fn test_audit_tracks_status_changes() {
    let conn = setup_db();
    insert_test_article(&conn, "art-2", "working");

    audit_repo::create_entry(
        &conn,
        "art-2",
        "status_change",
        Some("duplicate"),
        Some("working"),
        None,
        "system",
    )
    .expect("Failed to create status change entry");

    audit_repo::create_entry(
        &conn,
        "art-2",
        "ai_screen",
        Some("working"),
        Some("included"),
        Some("AI screened: include"),
        "ai",
    )
    .expect("Failed to create ai_screen entry");

    let entries = audit_repo::get_audit_trail(&conn, "art-2").expect("Failed to get audit trail");
    assert_eq!(entries.len(), 2);
    // Most recent first (DESC order)
    assert_eq!(entries[0].action.as_str(), "ai_screen");
    assert_eq!(entries[0].source.as_str(), "ai");
    assert_eq!(entries[1].from_status.as_deref(), Some("duplicate"));
}

#[test]
fn test_audit_trail_empty_for_nonexistent_article() {
    let conn = setup_db();
    let entries = audit_repo::get_audit_trail(&conn, "nonexistent")
        .expect("Should not error on empty result");
    assert!(entries.is_empty());
}

#[test]
fn test_multiple_audit_actions() {
    let conn = setup_db();
    insert_test_article(&conn, "art-3", "included");

    audit_repo::create_entry(&conn, "art-3", "import", None, None, None, "system")
        .expect("Failed to create import entry");
    audit_repo::create_entry(&conn, "art-3", "tag_add", None, None, Some("Added tag: AI"), "user")
        .expect("Failed to create tag_add entry");
    audit_repo::create_entry(
        &conn,
        "art-3",
        "manual_override",
        Some("rejected"),
        Some("included"),
        Some("User override"),
        "user",
    )
    .expect("Failed to create manual_override entry");

    let entries = audit_repo::get_audit_trail(&conn, "art-3").expect("Failed to get audit trail");
    assert_eq!(entries.len(), 3);
    // DESC order: manual_override first
    assert_eq!(entries[0].action.as_str(), "manual_override");
    assert_eq!(entries[1].action.as_str(), "tag_add");
    assert_eq!(entries[2].action.as_str(), "import");
}

#[test]
fn test_generic_system_errors() {
    let conn = setup_db();

    // Log system errors (which shouldn't require a valid article ID)
    audit_repo::log_error(&conn, "Failed to connect to LLM").expect("Failed to log error");
    audit_repo::log_error(&conn, "Malformed JSON response").expect("Failed to log error");

    let entries = audit_repo::get_generic_audit_entries(&conn, 10).expect("Failed to get generic audit entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].details.as_deref(), Some("Malformed JSON response"));
    assert_eq!(entries[0].article_id, ""); // Should map to empty string for Rust struct compatibility
    assert_eq!(entries[0].action.as_str(), "error");

    let cleared = audit_repo::clear_generic_entries(&conn).expect("Failed to clear entries");
    assert_eq!(cleared, 2);

    let entries_after = audit_repo::get_generic_audit_entries(&conn, 10).expect("Failed to get entries");
    assert!(entries_after.is_empty());
}
