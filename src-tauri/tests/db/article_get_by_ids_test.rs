use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: build a minimal NewArticle with a given title.
fn new_article(title: &str) -> NewArticle {
    NewArticle { title: title.to_string(), ..Default::default() }
}

/// Insert three articles and return their UUIDs.
fn seed_three(conn: &rusqlite::Connection) -> Vec<String> {
    ["Alpha", "Bravo", "Charlie"]
        .iter()
        .map(|t| article_repo::insert_article(conn, &new_article(t)).expect("insert failed").id)
        .collect()
}

#[test]
fn get_articles_by_ids_returns_exactly_the_requested_set() {
    let conn = setup_db();
    let ids = seed_three(&conn);

    // Request only the first two; the third must be absent.
    let requested = vec![ids[0].clone(), ids[1].clone()];
    let results = article_repo::get_articles_by_ids(&conn, &requested).expect("query failed");
    assert_eq!(results.len(), 2, "should return exactly the requested ids");
    let returned_ids: Vec<&str> = results.iter().map(|a| a.id.as_str()).collect();
    assert!(returned_ids.contains(&ids[0].as_str()), "should include first id");
    assert!(returned_ids.contains(&ids[1].as_str()), "should include second id");
    assert!(!returned_ids.contains(&ids[2].as_str()), "should NOT include third id");
}

#[test]
fn get_articles_by_ids_empty_input_returns_empty_vec_without_error() {
    // Empty input would produce invalid `IN ()` SQL without the early return.
    // This test pins that guard so a regression surfaces as a failed test
    // rather than a runtime SQL error.
    let conn = setup_db();
    seed_three(&conn);

    let results = article_repo::get_articles_by_ids(&conn, &[]).expect("empty query failed");
    assert!(results.is_empty(), "empty ids should yield empty result");
}

#[test]
fn get_articles_by_ids_unknown_ids_silently_absent() {
    // Unknown ids do not error and do not produce phantom rows; the
    // `filter_map` drops row-decode errors, matching the other read fns.
    let conn = setup_db();
    let real_ids = seed_three(&conn);

    let mixed = vec![real_ids[0].clone(), "nonexistent-uuid-xyz".to_string()];
    let results = article_repo::get_articles_by_ids(&conn, &mixed).expect("query failed");
    assert_eq!(results.len(), 1, "unknown id should be absent, real id present");
    assert_eq!(results[0].id, real_ids[0]);
}

#[test]
fn get_articles_by_ids_single_id() {
    // A single-id `IN (?)` query exercises the placeholder-join path with no
    // comma (edge case for the `(0..len).map("?")` join).
    let conn = setup_db();
    let ids = seed_three(&conn);

    let results =
        article_repo::get_articles_by_ids(&conn, &[ids[2].clone()]).expect("single query failed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Charlie");
}
