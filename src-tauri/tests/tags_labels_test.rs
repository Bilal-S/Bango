use bango_lib::db::app_settings_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::label_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::tag_repo;

#[test]
fn test_create_and_get_tags() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tag =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create_tag failed");
    assert_eq!(tag.name, "machine-learning");

    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "machine-learning");
}

#[test]
fn test_rename_tag() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tag = tag_repo::create_tag(&conn, "ml", "user_created").expect("create_tag failed");
    let renamed =
        tag_repo::rename_tag(&conn, &tag.id, "machine-learning").expect("rename_tag failed");
    assert_eq!(renamed.name, "machine-learning");
}

#[test]
fn test_delete_tag() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tag = tag_repo::create_tag(&conn, "test", "user_created").expect("create_tag failed");
    tag_repo::delete_tag(&conn, &tag.id).expect("delete_tag failed");
    assert!(tag_repo::get_all_tags(&conn).expect("get_all_tags failed").is_empty());
}

#[test]
fn test_create_tags_batch_skips_duplicates() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    tag_repo::create_tag(&conn, "ml", "user_created").expect("create_tag failed");
    let names = vec!["ml".to_string(), "dl".to_string()];
    let created = tag_repo::create_tags_batch(&conn, &names, "ai_suggested")
        .expect("create_tags_batch failed");
    assert_eq!(created.len(), 1); // Only "dl" created, "ml" skipped
    assert_eq!(created[0].name, "dl");
}

#[test]
fn test_create_and_get_labels() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let label = label_repo::create_label(&conn, "priority-read", "user_created")
        .expect("create_label failed");
    assert_eq!(label.name, "priority-read");

    let labels = label_repo::get_all_labels(&conn).expect("get_all_labels failed");
    assert_eq!(labels.len(), 1);
}

#[test]
fn test_rename_label() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let label =
        label_repo::create_label(&conn, "old-name", "user_created").expect("create_label failed");
    let renamed =
        label_repo::rename_label(&conn, &label.id, "new-name").expect("rename_label failed");
    assert_eq!(renamed.name, "new-name");
}

#[test]
fn test_delete_label() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let label =
        label_repo::create_label(&conn, "test", "user_created").expect("create_label failed");
    label_repo::delete_label(&conn, &label.id).expect("delete_label failed");
    assert!(label_repo::get_all_labels(&conn).expect("get_all_labels failed").is_empty());
}

#[test]
fn test_tag_label_isolation() {
    // Tags and labels live in separate tables -- names can overlap
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create_tag failed");
    label_repo::create_label(&conn, "machine-learning", "user_created")
        .expect("create_label failed");

    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    let labels = label_repo::get_all_labels(&conn).expect("get_all_labels failed");
    assert_eq!(tags.len(), 1);
    assert_eq!(labels.len(), 1);
    assert_eq!(tags[0].name, labels[0].name); // Same name, different entities
}

#[test]
fn test_get_article_count_for_tag() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tag = tag_repo::create_tag(&conn, "ml", "user_created").expect("create_tag failed");
    let count = tag_repo::get_article_count_for_tag(&conn, &tag.id).expect("count failed");
    assert_eq!(count, 0);
}

#[test]
fn test_get_article_count_for_label() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let label =
        label_repo::create_label(&conn, "priority", "user_created").expect("create_label failed");
    let count = label_repo::get_article_count_for_label(&conn, &label.id).expect("count failed");
    assert_eq!(count, 0);
}

#[test]
fn test_merge_tags() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let source = tag_repo::create_tag(&conn, "ml", "user_created").expect("create_tag failed");
    let target =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create_tag failed");

    let merged = tag_repo::merge_tags(&conn, &source.id, &target.id).expect("merge_tags failed");
    assert_eq!(merged.name, "machine-learning");

    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    assert_eq!(tags.len(), 1);
}

#[test]
fn test_merge_labels() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let source =
        label_repo::create_label(&conn, "read-first", "user_created").expect("create_label failed");
    let target =
        label_repo::create_label(&conn, "priority", "user_created").expect("create_label failed");

    let merged =
        label_repo::merge_labels(&conn, &source.id, &target.id).expect("merge_labels failed");
    assert_eq!(merged.name, "priority");

    let labels = label_repo::get_all_labels(&conn).expect("get_all_labels failed");
    assert_eq!(labels.len(), 1);
}

// ── Staleness-flag regression tests (PR 1 bugfix) ─────────────────────────
//
// `commands::tags::delete_tag` and `commands::labels::delete_label` previously
// omitted `mark_biblio_needs_refresh` + `mark_wiki_needs_refresh`, silently
// desyncing the keyword co-occurrence network and the wiki concept hubs after
// a delete. The command shims require `State<DbState>`, so these tests drive
// the repo delete + staleness-flag calls directly - the exact sequence the
// fixed command now performs.

#[test]
fn test_delete_tag_sets_staleness_flags() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tag = tag_repo::create_tag(&conn, "stale-tag", "user_created").expect("create_tag failed");

    // Reproduce the fixed command's sequence: repo delete + both staleness flags.
    tag_repo::delete_tag(&conn, &tag.id).expect("delete_tag failed");
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);

    assert!(
        app_settings_repo::get_biblio_needs_refresh(&conn).expect("biblio flag read failed"),
        "biblio_needs_refresh must be set after deleting a tag"
    );
    assert!(
        app_settings_repo::get_wiki_needs_refresh(&conn).expect("wiki flag read failed"),
        "wiki_needs_refresh must be set after deleting a tag"
    );
}

#[test]
fn test_delete_label_sets_staleness_flags() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let label = label_repo::create_label(&conn, "stale-label", "user_created")
        .expect("create_label failed");

    // Reproduce the fixed command's sequence: repo delete + both staleness flags.
    label_repo::delete_label(&conn, &label.id).expect("delete_label failed");
    app_settings_repo::mark_biblio_needs_refresh(&conn);
    app_settings_repo::mark_wiki_needs_refresh(&conn);

    assert!(
        app_settings_repo::get_biblio_needs_refresh(&conn).expect("biblio flag read failed"),
        "biblio_needs_refresh must be set after deleting a label"
    );
    assert!(
        app_settings_repo::get_wiki_needs_refresh(&conn).expect("wiki flag read failed"),
        "wiki_needs_refresh must be set after deleting a label"
    );
}

/// Refactor1 Tier 0: pin the
/// case-insensitive normalized-name dedupe contract of `create_tag` before the
/// tag/label repo consolidation (task T2.2). Creating a name that matches an
/// existing tag modulo case MUST return the existing row (original casing and
/// id preserved) instead of violating the UNIQUE constraint.
#[test]
fn create_tag_dedupes_normalized_name() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let first =
        tag_repo::create_tag(&conn, "Machine-Learning", "user_created").expect("create_tag failed");
    // Different casing + different source: still resolves to the same row.
    let second =
        tag_repo::create_tag(&conn, "machine-learning", "ai_suggested").expect("create_tag failed");

    assert_eq!(second.id, first.id, "must dedupe to the existing tag id");
    assert_eq!(second.name, "Machine-Learning", "original casing is preserved");
    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    assert_eq!(tags.len(), 1, "no second row may be inserted");
}

/// Refactor1 Tier 0: same dedupe contract for labels (`create_label`).
#[test]
fn create_label_dedupes_normalized_name() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let first = label_repo::create_label(&conn, "Priority-Read", "user_created")
        .expect("create_label failed");
    let second = label_repo::create_label(&conn, "priority-read", "ai_generated")
        .expect("create_label failed");

    assert_eq!(second.id, first.id, "must dedupe to the existing label id");
    assert_eq!(second.name, "Priority-Read", "original casing is preserved");
    let labels = label_repo::get_all_labels(&conn).expect("get_all_labels failed");
    assert_eq!(labels.len(), 1, "no second row may be inserted");
}
