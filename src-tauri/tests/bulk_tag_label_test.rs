use bango_lib::db::article_repo;
use bango_lib::db::audit_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::models::audit::AuditAction;

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: insert N articles and return their ids.
fn seed_articles(conn: &rusqlite::Connection, n: usize) -> Vec<String> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let article = NewArticle { title: format!("Test Article {i}"), ..Default::default() };
        let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
        ids.push(inserted.id);
    }
    ids
}

/// Helper: count audit entries of a given action for a specific article.
fn count_audit_entries(
    conn: &rusqlite::Connection,
    article_id: &str,
    action: AuditAction,
) -> usize {
    let action_str = action.as_str();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE article_id = ?1 AND action = ?2",
            rusqlite::params![article_id, action_str],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count as usize
}

// ─── Tag add ───────────────────────────────────────────────────────

#[test]
fn bulk_add_tag_to_all_articles_returns_affected_ids() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 3);

    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");

    assert_eq!(affected.len(), 3);
    // All three articles now have the tag.
    for id in &ids {
        let article = article_repo::get_article_by_id(&conn, id).expect("get failed");
        assert!(article.tags.contains(&"ml".to_string()));
    }
}

#[test]
fn bulk_add_tag_skips_articles_that_already_have_it() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 3);

    // First call: all 3 get the tag.
    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");
    assert_eq!(affected.len(), 3);

    // Second call: none get it again (INSERT OR IGNORE).
    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");
    assert_eq!(affected.len(), 0);
}

#[test]
fn bulk_add_tag_creates_tag_if_missing() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 1);

    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &ids, "brand-new").expect("add tag failed");
    assert_eq!(affected.len(), 1);

    // The tag row was created.
    let tag_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags WHERE name = 'brand-new'", [], |row| row.get(0))
        .unwrap_or(0);
    assert_eq!(tag_count, 1);
}

#[test]
fn bulk_add_tag_with_empty_ids_returns_empty() {
    let conn = setup_db();
    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &[], "ml").expect("add tag failed");
    assert!(affected.is_empty());
}

// ─── Tag remove ────────────────────────────────────────────────────

#[test]
fn bulk_remove_tag_from_all_articles_returns_affected_ids() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 3);

    // Add the tag first.
    article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");

    // Remove it.
    let affected =
        article_repo::bulk_remove_tag_from_articles(&conn, &ids, "ml").expect("remove tag failed");
    assert_eq!(affected.len(), 3);

    for id in &ids {
        let article = article_repo::get_article_by_id(&conn, id).expect("get failed");
        assert!(!article.tags.contains(&"ml".to_string()));
    }
}

#[test]
fn bulk_remove_tag_returns_empty_when_tag_not_present_on_any_article() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 3);

    // No article has the tag; removing it affects nothing.
    let affected = article_repo::bulk_remove_tag_from_articles(&conn, &ids, "absent")
        .expect("remove tag failed");
    assert!(affected.is_empty());
}

#[test]
fn bulk_remove_tag_returns_empty_when_tag_does_not_exist() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    // The tag row doesn't exist at all; remove is a clean no-op.
    let affected = article_repo::bulk_remove_tag_from_articles(&conn, &ids, "ghost")
        .expect("remove tag failed");
    assert!(affected.is_empty());
}

#[test]
fn bulk_remove_tag_partial_only_affects_articles_that_have_it() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 3);

    // Add the tag to only the first article.
    article_repo::bulk_add_tag_to_articles(&conn, &ids[0..1], "ml").expect("add tag failed");

    // Remove from all three: only one had it.
    let affected =
        article_repo::bulk_remove_tag_from_articles(&conn, &ids, "ml").expect("remove tag failed");
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0], ids[0]);
}

// ─── Label add / remove (mirror) ───────────────────────────────────

#[test]
fn bulk_add_label_to_all_articles_returns_affected_ids() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    let affected = article_repo::bulk_add_label_to_articles(&conn, &ids, "priority")
        .expect("add label failed");
    assert_eq!(affected.len(), 2);

    for id in &ids {
        let article = article_repo::get_article_by_id(&conn, id).expect("get failed");
        assert!(article.labels.contains(&"priority".to_string()));
    }
}

#[test]
fn bulk_add_label_skips_articles_that_already_have_it() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    article_repo::bulk_add_label_to_articles(&conn, &ids, "priority").expect("add label failed");
    let affected = article_repo::bulk_add_label_to_articles(&conn, &ids, "priority")
        .expect("add label failed");
    assert!(affected.is_empty());
}

#[test]
fn bulk_remove_label_from_all_articles_returns_affected_ids() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    article_repo::bulk_add_label_to_articles(&conn, &ids, "priority").expect("add label failed");

    let affected = article_repo::bulk_remove_label_from_articles(&conn, &ids, "priority")
        .expect("remove label failed");
    assert_eq!(affected.len(), 2);

    for id in &ids {
        let article = article_repo::get_article_by_id(&conn, id).expect("get failed");
        assert!(!article.labels.contains(&"priority".to_string()));
    }
}

#[test]
fn bulk_remove_label_returns_empty_when_label_not_present() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    let affected = article_repo::bulk_remove_label_from_articles(&conn, &ids, "absent")
        .expect("remove label failed");
    assert!(affected.is_empty());
}

// ─── Audit entries (command-layer contract) ────────────────────────
//
// The repo functions do NOT write audit entries (that is the command layer's
// job via `audit_repo::create_or_update_entry`). This test exercises the full
// command-layer path by replicating the `write_bulk_tag_label_audit` helper so
// the per-article audit contract is locked in.

fn write_audit_entries(
    conn: &rusqlite::Connection,
    affected_ids: &[String],
    action: &str,
    name: &str,
) {
    let detail = format!("Bulk {action}: \"{name}\"");
    for id in affected_ids {
        audit_repo::create_or_update_entry(conn, id, action, None, None, Some(&detail), "user")
            .expect("audit entry failed");
    }
}

#[test]
fn bulk_add_tag_then_audit_per_article() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    let affected =
        article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");
    write_audit_entries(&conn, &affected, "tag_add", "ml");

    for id in &ids {
        assert_eq!(count_audit_entries(&conn, id, AuditAction::TagAdd), 1);
    }
}

#[test]
fn bulk_remove_tag_then_audit_per_article() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    article_repo::bulk_add_tag_to_articles(&conn, &ids, "ml").expect("add tag failed");

    let affected =
        article_repo::bulk_remove_tag_from_articles(&conn, &ids, "ml").expect("remove tag failed");
    write_audit_entries(&conn, &affected, "tag_remove", "ml");

    for id in &ids {
        assert_eq!(count_audit_entries(&conn, id, AuditAction::TagRemove), 1);
    }
}

#[test]
fn bulk_add_label_then_audit_per_article() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    let affected = article_repo::bulk_add_label_to_articles(&conn, &ids, "priority")
        .expect("add label failed");
    write_audit_entries(&conn, &affected, "label_add", "priority");

    for id in &ids {
        assert_eq!(count_audit_entries(&conn, id, AuditAction::LabelAdd), 1);
    }
}

#[test]
fn bulk_remove_label_then_audit_per_article() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    article_repo::bulk_add_label_to_articles(&conn, &ids, "priority").expect("add label failed");

    let affected = article_repo::bulk_remove_label_from_articles(&conn, &ids, "priority")
        .expect("remove label failed");
    write_audit_entries(&conn, &affected, "label_remove", "priority");

    for id in &ids {
        assert_eq!(count_audit_entries(&conn, id, AuditAction::LabelRemove), 1);
    }
}

#[test]
fn bulk_remove_no_audit_when_nothing_affected() {
    let conn = setup_db();
    let ids = seed_articles(&conn, 2);

    // The tag doesn't exist, so `affected` is empty; no audit entries should
    // be written.
    let affected = article_repo::bulk_remove_tag_from_articles(&conn, &ids, "ghost")
        .expect("remove tag failed");
    write_audit_entries(&conn, &affected, "tag_remove", "ghost");

    for id in &ids {
        assert_eq!(count_audit_entries(&conn, id, AuditAction::TagRemove), 0);
    }
}
