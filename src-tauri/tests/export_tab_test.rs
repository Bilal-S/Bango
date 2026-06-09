use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use rusqlite::params;

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

/// Helper: set article status directly.
fn set_status(conn: &rusqlite::Connection, id: &str, status: &str) {
    conn.execute(
        "UPDATE articles SET status = ?1, changed_at = datetime('now') WHERE id = ?2",
        params![status, id],
    ).expect("set status");
}

// ─── get_articles_for_export: "all" tab ────────────────────────────

#[test]
fn test_export_all_returns_every_article() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Article 1")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");

    let a2 = article_repo::insert_article(&conn, &new_article("Article 2")).expect("insert");
    article_repo::move_to_working(&conn, &a2.id).expect("move");
    set_status(&conn, &a2.id, "included");

    let a3 = article_repo::insert_article(&conn, &new_article("Article 3")).expect("insert");
    article_repo::move_to_working(&conn, &a3.id).expect("move");
    set_status(&conn, &a3.id, "rejected");

    let results =
        article_repo::get_articles_for_export(&conn, "all", false).expect("export all");
    assert_eq!(results.len(), 3, "All tab should return every article regardless of status");
}

// ─── get_articles_for_export: specific status tab ──────────────────

#[test]
fn test_export_included_returns_only_included() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Included 1")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");
    set_status(&conn, &a1.id, "included");

    let a2 = article_repo::insert_article(&conn, &new_article("Working 1")).expect("insert");
    article_repo::move_to_working(&conn, &a2.id).expect("move");

    let results =
        article_repo::get_articles_for_export(&conn, "included", false).expect("export included");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Included 1");
}

#[test]
fn test_export_rejected_returns_only_rejected() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Rejected 1")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");
    set_status(&conn, &a1.id, "rejected");

    let a2 = article_repo::insert_article(&conn, &new_article("Included 1")).expect("insert");
    article_repo::move_to_working(&conn, &a2.id).expect("move");
    set_status(&conn, &a2.id, "included");

    let results =
        article_repo::get_articles_for_export(&conn, "rejected", false).expect("export rejected");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Rejected 1");
}

#[test]
fn test_export_working_returns_only_working() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Working 1")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");

    let a2 = article_repo::insert_article(&conn, &new_article("Included 1")).expect("insert");
    article_repo::move_to_working(&conn, &a2.id).expect("move");
    set_status(&conn, &a2.id, "included");

    let results =
        article_repo::get_articles_for_export(&conn, "working", false).expect("export working");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Working 1");
}

// ─── get_articles_for_export: error tab (screening_errors_only) ───

#[test]
fn test_export_errors_returns_only_screened_working_articles() {
    let conn = setup_db();

    // Working article that has been screened (has screened_at set) — this is an "error" article
    let a1 = article_repo::insert_article(&conn, &new_article("Screened With Error")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");
    // Set screened_at to simulate a screening error
    conn.execute(
        "UPDATE articles SET screened_at = datetime('now') WHERE id = ?1",
        params![a1.id],
    ).expect("update screened_at");

    // Working article that has NOT been screened — should NOT appear in errors
    let a2 = article_repo::insert_article(&conn, &new_article("Unscreened Working")).expect("insert");
    article_repo::move_to_working(&conn, &a2.id).expect("move");

    // Included article — should NOT appear in errors
    let a3 = article_repo::insert_article(&conn, &new_article("Included Article")).expect("insert");
    article_repo::move_to_working(&conn, &a3.id).expect("move");
    set_status(&conn, &a3.id, "included");

    let results = article_repo::get_articles_for_export(&conn, "error", true)
        .expect("export errors");

    assert_eq!(results.len(), 1, "Only working articles with screening errors should be returned");
    assert_eq!(results[0].title, "Screened With Error");
}

#[test]
fn test_export_errors_empty_when_no_screened_articles() {
    let conn = setup_db();

    // Only unscreened working articles — no errors
    let a1 = article_repo::insert_article(&conn, &new_article("Unscreened")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");

    let results = article_repo::get_articles_for_export(&conn, "error", true)
        .expect("export errors");

    assert!(results.is_empty(), "No screening errors should return empty list");
}

#[test]
fn test_export_duplicate_returns_only_duplicates() {
    let conn = setup_db();

    let orig = article_repo::insert_article(&conn, &new_article("Original")).expect("insert");
    article_repo::move_to_working(&conn, &orig.id).expect("move");

    let dup = article_repo::insert_article(&conn, &new_article("Duplicate")).expect("insert");
    article_repo::mark_as_duplicate(&conn, &dup.id, &orig.id).expect("mark dup");

    let results = article_repo::get_articles_for_export(&conn, "duplicate", false)
        .expect("export duplicate");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Duplicate");
}

// ─── get_articles_for_export: empty tab ────────────────────────────

#[test]
fn test_export_empty_tab_returns_empty_list() {
    let conn = setup_db();

    // Only working articles, no included ones
    let a1 = article_repo::insert_article(&conn, &new_article("Working 1")).expect("insert");
    article_repo::move_to_working(&conn, &a1.id).expect("move");

    let results =
        article_repo::get_articles_for_export(&conn, "included", false).expect("export included");
    assert!(results.is_empty(), "No included articles should return empty list");
}