//! Integration tests for audit entry coalescing.
//!
//! Verifies that [`audit_repo::create_or_update_entry`] coalesces rapid
//! same-type edits into a single audit row within the 300-second window,
//! while still creating separate rows for different actions or when the
//! window has expired.

use bango_lib::db::audit_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::audit::AuditAction;

fn setup() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) \
         VALUES ('art1', 'Test Article', 'Abstract', 'Author', 'working')",
        [],
    )
    .expect("insert article");
    conn
}

#[test]
fn coalesces_rapid_same_type_edits_into_one_row() {
    let conn = setup();

    // Simulate adding 3 labels one at a time (rapid succession).
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 1 label(s)"),
        "user",
    )
    .expect("first entry");
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 2 label(s)"),
        "user",
    )
    .expect("second entry");
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 3 label(s)"),
        "user",
    )
    .expect("third entry");

    let trail = audit_repo::get_audit_trail(&conn, "art1").expect("get audit trail");
    let label_entries: Vec<_> =
        trail.iter().filter(|e| e.action == AuditAction::LabelAdd).collect();

    // Should be exactly ONE entry, with the latest details.
    assert_eq!(label_entries.len(), 1, "3 rapid label_add calls should coalesce into 1 entry");
    assert_eq!(label_entries[0].details.as_deref(), Some("Labels updated: 3 label(s)"));
}

#[test]
fn does_not_coalesce_different_actions() {
    let conn = setup();

    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 1 label(s)"),
        "user",
    )
    .expect("label entry");
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "tag_add",
        None,
        None,
        Some("Tags updated: 2 tag(s)"),
        "user",
    )
    .expect("tag entry");

    let trail = audit_repo::get_audit_trail(&conn, "art1").expect("get audit trail");
    assert_eq!(trail.len(), 2, "different actions should not coalesce");
}

#[test]
fn does_not_coalesce_different_articles() {
    let conn = setup();
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) \
         VALUES ('art2', 'Second Article', 'Abstract', 'Author', 'working')",
        [],
    )
    .expect("insert article 2");

    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 1 label(s)"),
        "user",
    )
    .expect("art1 entry");
    audit_repo::create_or_update_entry(
        &conn,
        "art2",
        "label_add",
        None,
        None,
        Some("Labels updated: 2 label(s)"),
        "user",
    )
    .expect("art2 entry");

    let trail1 = audit_repo::get_audit_trail(&conn, "art1").expect("get audit trail 1");
    let trail2 = audit_repo::get_audit_trail(&conn, "art2").expect("get audit trail 2");
    assert_eq!(trail1.len(), 1, "art1 should have 1 entry");
    assert_eq!(trail2.len(), 1, "art2 should have 1 entry");
}

#[test]
fn does_not_coalesce_different_sources() {
    let conn = setup();

    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("User labels"),
        "user",
    )
    .expect("user entry");
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("AI labels"),
        "ai",
    )
    .expect("ai entry");

    let trail = audit_repo::get_audit_trail(&conn, "art1").expect("get audit trail");
    assert_eq!(trail.len(), 2, "different sources should not coalesce");
}

#[test]
fn coalesces_after_window_expired() {
    let conn = setup();

    // Insert an old entry with a timestamp 10 minutes ago (600 seconds > 300 window).
    let old_ts = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::seconds(600))
        .map(|t| t.to_rfc3339())
        .expect("subtract 600s");
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, source) \
         VALUES ('old-entry', 'art1', ?1, 'label_add', 'user')",
        rusqlite::params![old_ts],
    )
    .expect("insert old entry");

    // New entry should NOT coalesce with the old one (outside window).
    audit_repo::create_or_update_entry(
        &conn,
        "art1",
        "label_add",
        None,
        None,
        Some("Labels updated: 1 label(s)"),
        "user",
    )
    .expect("new entry");

    let trail = audit_repo::get_audit_trail(&conn, "art1").expect("get audit trail");
    let label_entries: Vec<_> =
        trail.iter().filter(|e| e.action == AuditAction::LabelAdd).collect();
    assert_eq!(label_entries.len(), 2, "entry outside window should not coalesce");
}
