//! Integration tests for the screening-flags reset on status transitions.
//!
//! Regression coverage for a bug where articles moved back to `working` from
//! `included` or `rejected` (via `update_article_status` or
//! `bulk_update_article_status`) retained their stale `screened_at` timestamp
//! and `screening_error` flag. The stale metadata made the articles invisible
//! to the screening engine (`get_next_unscreened_working_batch` requires
//! `screened_at IS NULL`) and caused them to surface in the Error tab even
//! though `screening_error` was `0`.
//!
//! The fix resets both flags whenever an article is moved TO `working`, so the
//! state-machine transition `Working ↔ Included ↔ Rejected` (see
//! `docs/bango-v5-spec.md` §4.2) always leaves the article eligible for
//! re-screening.

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

fn new_article(title: &str) -> NewArticle {
    NewArticle { title: title.to_string(), ..Default::default() }
}

/// Helper: simulate a successful screening pass by calling the same UPDATE the
/// engine's `update_article_after_screening` helper runs.
fn simulate_screening_success(conn: &rusqlite::Connection, article_id: &str, decision: &str) {
    conn.execute(
        "UPDATE articles SET status = ?1, \
         screened_at = datetime('now'), \
         screening_error = 0, \
         changed_at = datetime('now') \
         WHERE id = ?2",
        rusqlite::params![decision, article_id],
    )
    .expect("simulate_screening_success UPDATE failed");
}

/// Helper: simulate a screening error by calling the same UPDATE the engine's
/// `set_screening_error` helper runs.
fn simulate_screening_error(conn: &rusqlite::Connection, article_id: &str) {
    conn.execute(
        "UPDATE articles SET screening_error = 1, \
         screened_at = datetime('now'), \
         changed_at = datetime('now') \
         WHERE id = ?1",
        rusqlite::params![article_id],
    )
    .expect("simulate_screening_error UPDATE failed");
}

fn read_screening_flags(conn: &rusqlite::Connection, article_id: &str) -> (Option<String>, bool) {
    conn.query_row(
        "SELECT screened_at, screening_error FROM articles WHERE id = ?1",
        rusqlite::params![article_id],
        |row| {
            let screened_at: Option<String> = row.get(0)?;
            let screening_error_int: i64 = row.get(1)?;
            Ok((screened_at, screening_error_int != 0))
        },
    )
    .expect("read_screening_flags SELECT failed")
}

// ─── update_article_status (single article) ──────────────────────────────

#[test]
fn test_update_article_status_to_working_resets_screening_flags() {
    // Seed article -> move to working -> screen (included) -> move back to working.
    let conn = setup_db();
    let inserted =
        article_repo::insert_article(&conn, &new_article("Article A")).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move to working failed");

    // Simulate AI screening marking the article as included with screened_at set.
    simulate_screening_success(&conn, &inserted.id, "included");

    // Sanity: before the fix the flags were set.
    let (screened_at, screening_error) = read_screening_flags(&conn, &inserted.id);
    assert!(screened_at.is_some(), "screened_at should be set after screening");
    assert!(!screening_error, "screening_error should be 0 for a successful screen");

    // Move back to working via update_article_status.
    article_repo::update_article_status(&conn, &inserted.id, "working")
        .expect("move back to working failed");

    // The fix: screened_at should be NULL and screening_error should be 0.
    let (screened_at, screening_error) = read_screening_flags(&conn, &inserted.id);
    assert!(
        screened_at.is_none(),
        "screened_at should be NULL after moving back to working so the article is eligible for re-screening"
    );
    assert!(!screening_error, "screening_error should be cleared after moving back to working");

    // The article should now be counted as unscreened and thus eligible for screening.
    let unscreened = article_repo::count_unscreened_working(&conn).expect("count failed");
    assert_eq!(
        unscreened, 1,
        "Article should be counted as unscreened after moving back to working"
    );
}

#[test]
fn test_update_article_status_to_working_clears_screening_error_flag() {
    // Regression: an article that had a screening error (`screening_error = 1`)
    // and was then moved to `included`/`rejected` should also have its error flag
    // cleared when moved back to `working`.
    let conn = setup_db();
    let inserted =
        article_repo::insert_article(&conn, &new_article("Article B")).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move to working failed");

    // Simulate a screening error: screened_at set + screening_error = 1.
    simulate_screening_error(&conn, &inserted.id);

    // Move to included manually (via update_article_status) so the error flag
    // survives the status change.
    article_repo::update_article_status(&conn, &inserted.id, "included")
        .expect("move to included failed");
    let (screened_at, screening_error) = read_screening_flags(&conn, &inserted.id);
    assert!(
        screened_at.is_some(),
        "Sanity: screened_at should still be set after moving to included (the old behavior)"
    );
    assert!(
        screening_error,
        "Sanity: screening_error should still be 1 after moving to included (the old behavior)"
    );

    // Move back to working - the fix should reset both flags.
    article_repo::update_article_status(&conn, &inserted.id, "working")
        .expect("move back to working failed");

    let (screened_at, screening_error) = read_screening_flags(&conn, &inserted.id);
    assert!(screened_at.is_none(), "screened_at should be NULL after moving back to working");
    assert!(
        !screening_error,
        "screening_error should be cleared after moving back to working so the article is eligible for re-screening"
    );

    let unscreened = article_repo::count_unscreened_working(&conn).expect("count failed");
    assert_eq!(unscreened, 1, "Article should be counted as unscreened");
}

#[test]
fn test_update_article_status_to_included_does_not_reset_screening_flags() {
    // Sanity: moving away from `working` should NOT reset the screening flags.
    // The reset is only for the TO-working direction.
    let conn = setup_db();
    let inserted =
        article_repo::insert_article(&conn, &new_article("Article C")).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move to working failed");

    simulate_screening_success(&conn, &inserted.id, "included");
    let (screened_at_before, _) = read_screening_flags(&conn, &inserted.id);
    assert!(screened_at_before.is_some());

    // Move to rejected - flags should be preserved.
    article_repo::update_article_status(&conn, &inserted.id, "rejected")
        .expect("move to rejected failed");

    let (screened_at_after, _) = read_screening_flags(&conn, &inserted.id);
    assert_eq!(
        screened_at_after, screened_at_before,
        "Moving away from working should not change screened_at"
    );
}

// ─── bulk_update_article_status (batch) ──────────────────────────────────

#[test]
fn test_bulk_update_article_status_to_working_resets_screening_flags() {
    let conn = setup_db();

    let ids: Vec<String> = (0..3)
        .map(|i| {
            let inserted =
                article_repo::insert_article(&conn, &new_article(&format!("Article {}", i)))
                    .expect("insert failed");
            article_repo::move_to_working(&conn, &inserted.id).expect("move to working failed");
            simulate_screening_success(&conn, &inserted.id, "included");
            inserted.id
        })
        .collect();

    // All three should have screened_at set.
    for id in &ids {
        let (screened_at, _) = read_screening_flags(&conn, id);
        assert!(screened_at.is_some(), "Sanity: screened_at should be set after screening");
    }

    // Bulk move back to working.
    article_repo::bulk_update_article_status(&conn, &ids, "working")
        .expect("bulk_update_article_status failed");

    // All three should now have screened_at = NULL and screening_error = 0.
    for id in &ids {
        let (screened_at, screening_error) = read_screening_flags(&conn, id);
        assert!(screened_at.is_none(), "screened_at should be NULL after bulk move to working");
        assert!(!screening_error, "screening_error should be cleared after bulk move to working");
    }

    let unscreened = article_repo::count_unscreened_working(&conn).expect("count failed");
    assert_eq!(
        unscreened,
        ids.len(),
        "All {} articles should be counted as unscreened after bulk move to working",
        ids.len()
    );
}

#[test]
fn test_bulk_update_article_status_to_included_does_not_reset_screening_flags() {
    // Sanity: bulk moving away from `working` should NOT reset the screening flags.
    let conn = setup_db();

    let ids: Vec<String> = (0..2)
        .map(|i| {
            let inserted =
                article_repo::insert_article(&conn, &new_article(&format!("Article {}", i)))
                    .expect("insert failed");
            article_repo::move_to_working(&conn, &inserted.id).expect("move to working failed");
            simulate_screening_success(&conn, &inserted.id, "included");
            inserted.id
        })
        .collect();

    // Bulk move to rejected - flags should be preserved.
    article_repo::bulk_update_article_status(&conn, &ids, "rejected")
        .expect("bulk_update_article_status failed");

    for id in &ids {
        let (screened_at, _) = read_screening_flags(&conn, id);
        assert!(screened_at.is_some(), "Moving away from working should not clear screened_at");
    }
}
