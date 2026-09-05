use bango_lib::commands::labels::merge_label_inner;
use bango_lib::commands::tags::merge_tag_inner;
use bango_lib::db::app_settings_repo;
use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::label_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::tag_repo;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;

/// Helper: in-memory DB with migrations applied.
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

/// Helper: link a tag to an article by id (direct junction insert).
fn link_tag(conn: &rusqlite::Connection, article_id: &str, tag_id: &str) {
    conn.execute(
        "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, tag_id],
    )
    .expect("link tag failed");
}

/// Helper: link a label to an article by id (direct junction insert).
fn link_label(conn: &rusqlite::Connection, article_id: &str, label_id: &str) {
    conn.execute(
        "INSERT INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, label_id],
    )
    .expect("link label failed");
}

/// Helper: count `article_tags` rows for a tag.
fn count_tag_links(conn: &rusqlite::Connection, tag_id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM article_tags WHERE tag_id = ?1",
        rusqlite::params![tag_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Helper: count `tag_remove` audit rows for an article.
fn count_tag_remove_audit(conn: &rusqlite::Connection, article_id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM audit_entries WHERE article_id = ?1 AND action = 'tag_remove'",
        rusqlite::params![article_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

// ── merge_tag ──────────────────────────────────────────────────────────────

#[test]
fn merge_tag_reassigns_and_deletes() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 3);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");

    // Two articles get `from`, one gets `into`.
    link_tag(&conn, &articles[0], &from.id);
    link_tag(&conn, &articles[1], &from.id);
    link_tag(&conn, &articles[2], &into.id);

    let result = merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    assert_eq!(result.from_name, "ml");
    assert_eq!(result.into_name, "machine-learning");
    assert_eq!(result.reassigned_count, 2);
    assert_eq!(result.already_had_survivor_count, 0);

    // Source tag is gone, target survives.
    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "machine-learning");

    // Both from-tagged articles now carry the survivor.
    let a0 = article_repo::get_article_by_id(&conn, &articles[0]).expect("get a0");
    let a1 = article_repo::get_article_by_id(&conn, &articles[1]).expect("get a1");
    assert!(a0.tags.contains(&"machine-learning".to_string()));
    assert!(a1.tags.contains(&"machine-learning".to_string()));
}

#[test]
fn merge_tag_overcount_fix_overlap_subtracts_from_reassigned() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 2);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");

    // Article 0 has BOTH tags (overlap); article 1 has only `from`.
    link_tag(&conn, &articles[0], &from.id);
    link_tag(&conn, &articles[0], &into.id);
    link_tag(&conn, &articles[1], &from.id);

    let result = merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    // total_from = 2; overlap = 1 (article 0); reassigned = 2 - 1 = 1.
    assert_eq!(result.reassigned_count, 1);
    assert_eq!(result.already_had_survivor_count, 1);
}

#[test]
fn merge_tag_no_dangling_overlap_rows() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 1);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");

    // The single article has BOTH tags (pure overlap case).
    link_tag(&conn, &articles[0], &from.id);
    link_tag(&conn, &articles[0], &into.id);

    merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    // Regression guard: zero dangling rows referencing the deleted from-id.
    assert_eq!(count_tag_links(&conn, &from.id), 0);
    // And the article has exactly one link to the survivor.
    assert_eq!(count_tag_links(&conn, &into.id), 1);
}

#[test]
fn merge_tag_same_id_rejected() {
    let conn = setup_db();
    let tag = tag_repo::create_tag(&conn, "solo", "user_created").expect("create tag");

    let err = merge_tag_inner(&conn, &tag.id, &tag.id).unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "expected Validation error, got {err:?}");
}

#[test]
fn merge_tag_missing_from_rejected() {
    let conn = setup_db();
    let into = tag_repo::create_tag(&conn, "survivor", "user_created").expect("create into");

    let err = merge_tag_inner(&conn, "nonexistent-from-id", &into.id).unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)), "expected NotFound error, got {err:?}");
}

#[test]
fn merge_tag_missing_into_rejected() {
    let conn = setup_db();
    let from = tag_repo::create_tag(&conn, "source", "user_created").expect("create from");

    let err = merge_tag_inner(&conn, &from.id, "nonexistent-into-id").unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)), "expected NotFound error, got {err:?}");
}

#[test]
fn merge_tag_writes_per_article_audit() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 2);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");

    link_tag(&conn, &articles[0], &from.id);
    link_tag(&conn, &articles[1], &from.id);

    merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    // Each reassigned article carries a tag_remove audit row whose detail
    // mentions the merge.
    for id in &articles {
        let detail: String = conn
            .query_row(
                "SELECT details FROM audit_entries WHERE article_id = ?1 AND action = 'tag_remove'",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .expect("audit row missing");
        assert!(detail.contains("Replaced tag"), "unexpected detail: {detail}");
        assert!(detail.contains("merge"), "unexpected detail: {detail}");
    }
}

#[test]
fn merge_tag_bumps_changed_at() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 1);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");

    link_tag(&conn, &articles[0], &from.id);

    let before: String = conn
        .query_row(
            "SELECT changed_at FROM articles WHERE id = ?1",
            rusqlite::params![&articles[0]],
            |row| row.get(0),
        )
        .expect("get changed_at");

    // Sleep so the timestamp genuinely advances (SQLite `datetime('now')` is
    // second-resolution; without a gap the before/after can be equal).
    std::thread::sleep(std::time::Duration::from_secs(1));

    merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    let after: String = conn
        .query_row(
            "SELECT changed_at FROM articles WHERE id = ?1",
            rusqlite::params![&articles[0]],
            |row| row.get(0),
        )
        .expect("get changed_at");
    assert_ne!(before, after, "changed_at should advance after a merge");
}

#[test]
fn merge_tag_sets_staleness_flags() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 1);
    let from = tag_repo::create_tag(&conn, "ml", "user_created").expect("create from");
    let into =
        tag_repo::create_tag(&conn, "machine-learning", "user_created").expect("create into");
    link_tag(&conn, &articles[0], &from.id);

    merge_tag_inner(&conn, &from.id, &into.id).expect("merge failed");

    assert!(app_settings_repo::get_biblio_needs_refresh(&conn).expect("biblio flag"));
    assert!(app_settings_repo::get_wiki_needs_refresh(&conn).expect("wiki flag"));
}

#[test]
fn merge_tag_chain_safe() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 2);
    let a = tag_repo::create_tag(&conn, "alpha", "user_created").expect("create a");
    let b = tag_repo::create_tag(&conn, "beta", "user_created").expect("create b");
    let c = tag_repo::create_tag(&conn, "gamma", "user_created").expect("create c");

    link_tag(&conn, &articles[0], &a.id);
    link_tag(&conn, &articles[1], &b.id);

    // First merge: A -> B. Article 0 (had A) is reassigned to B; article 1
    // already had B and is unaffected. No article had BOTH A and B, so the
    // overlap is 0.
    let r1 = merge_tag_inner(&conn, &a.id, &b.id).expect("merge A->B failed");
    assert_eq!(r1.reassigned_count, 1);
    assert_eq!(r1.already_had_survivor_count, 0);

    // Second merge: B -> C. Both articles now carry C.
    let r2 = merge_tag_inner(&conn, &b.id, &c.id).expect("merge B->C failed");
    assert_eq!(r2.reassigned_count, 2);
    assert_eq!(r2.already_had_survivor_count, 0);

    // No dangling rows for A or B; only C survives.
    assert_eq!(count_tag_links(&conn, &a.id), 0);
    assert_eq!(count_tag_links(&conn, &b.id), 0);
    assert_eq!(count_tag_links(&conn, &c.id), 2);

    let tags = tag_repo::get_all_tags(&conn).expect("get_all_tags failed");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "gamma");
}

// ── merge_label (mirror coverage) ──────────────────────────────────────────

#[test]
fn merge_label_reassigns_and_deletes() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 2);
    let from = label_repo::create_label(&conn, "read-first", "user_created").expect("create from");
    let into = label_repo::create_label(&conn, "priority", "user_created").expect("create into");

    link_label(&conn, &articles[0], &from.id);
    link_label(&conn, &articles[1], &from.id);

    let result = merge_label_inner(&conn, &from.id, &into.id).expect("merge failed");

    assert_eq!(result.from_name, "read-first");
    assert_eq!(result.into_name, "priority");
    assert_eq!(result.reassigned_count, 2);
    assert_eq!(result.already_had_survivor_count, 0);

    let labels = label_repo::get_all_labels(&conn).expect("get_all_labels failed");
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].name, "priority");
}

#[test]
fn merge_label_overcount_fix() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 2);
    let from = label_repo::create_label(&conn, "read-first", "user_created").expect("create from");
    let into = label_repo::create_label(&conn, "priority", "user_created").expect("create into");

    // Article 0 has BOTH labels (overlap).
    link_label(&conn, &articles[0], &from.id);
    link_label(&conn, &articles[0], &into.id);
    link_label(&conn, &articles[1], &from.id);

    let result = merge_label_inner(&conn, &from.id, &into.id).expect("merge failed");
    assert_eq!(result.reassigned_count, 1);
    assert_eq!(result.already_had_survivor_count, 1);
}

#[test]
fn merge_label_same_id_rejected() {
    let conn = setup_db();
    let label = label_repo::create_label(&conn, "solo", "user_created").expect("create label");

    let err = merge_label_inner(&conn, &label.id, &label.id).unwrap_err();
    assert!(matches!(err, AppError::Validation(_)), "expected Validation error, got {err:?}");
}

#[test]
fn merge_label_sets_staleness_flags() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 1);
    let from = label_repo::create_label(&conn, "read-first", "user_created").expect("create from");
    let into = label_repo::create_label(&conn, "priority", "user_created").expect("create into");
    link_label(&conn, &articles[0], &from.id);

    merge_label_inner(&conn, &from.id, &into.id).expect("merge failed");

    assert!(app_settings_repo::get_biblio_needs_refresh(&conn).expect("biblio flag"));
    assert!(app_settings_repo::get_wiki_needs_refresh(&conn).expect("wiki flag"));
}

#[test]
fn merge_label_no_dangling_overlap_rows() {
    let conn = setup_db();
    let articles = seed_articles(&conn, 1);
    let from = label_repo::create_label(&conn, "read-first", "user_created").expect("create from");
    let into = label_repo::create_label(&conn, "priority", "user_created").expect("create into");

    link_label(&conn, &articles[0], &from.id);
    link_label(&conn, &articles[0], &into.id);

    merge_label_inner(&conn, &from.id, &into.id).expect("merge failed");

    let dangling: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM article_labels WHERE label_id = ?1",
            rusqlite::params![&from.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(dangling, 0);

    let survivor_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM article_labels WHERE label_id = ?1",
            rusqlite::params![&into.id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(survivor_links, 1);
}

// Silence the unused helper warning for `count_tag_remove_audit` (kept for
// future audit-detail assertions; the per-article audit test above queries the
// detail inline instead).
#[allow(dead_code)]
fn _silence_unused() {
    let conn = setup_db();
    let _ = count_tag_remove_audit(&conn, "x");
}
