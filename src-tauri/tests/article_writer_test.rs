use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::screening::article_writer::{
    mark_batch_screening_error, write_article_screening_result,
};
use bango_lib::screening::decision::ArticleDecision;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = create_connection().expect("db");
    run_migrations(&conn).expect("migrations");
    conn
}

fn insert_article(conn: &Connection) -> String {
    let article = NewArticle {
        title: "Test Article".to_string(),
        abstract_text: "Abstract about sugar taxes.".to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        import_source: Some("test".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    let id = inserted[0].id.clone();
    article_repo::move_articles_to_working_batch(conn, std::slice::from_ref(&id)).expect("working");
    id
}

// -- mark_batch_screening_error tests --

#[test]
fn mark_batch_marks_all_articles() {
    let conn = setup_db();
    let id1 = insert_article(&conn);
    let id2 = insert_article(&conn);
    let working = article_repo::get_articles_by_status(&conn, "working").expect("get working");
    assert_eq!(working.len(), 2, "should have 2 working articles");

    mark_batch_screening_error(&conn, &working, "LLM error", None).expect("mark batch");

    for id in [&id1, &id2] {
        let error_flag: i64 = conn
            .query_row(
                "SELECT screening_error FROM articles WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(error_flag, 1, "article {id} should have screening_error=1");
    }
}

#[test]
fn mark_batch_with_raw_response() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let working = article_repo::get_articles_by_status(&conn, "working").expect("get working");

    mark_batch_screening_error(&conn, &working, "Parse error", Some("[{\"bad\":true}"))
        .expect("mark batch");

    let audit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE article_id = ?1 AND details LIKE '%Parse error%'",
            rusqlite::params![&id],
            |row| row.get(0),
        )
        .expect("query");
    assert!(audit_count > 0, "audit entry should exist with the error reason");
}

#[test]
fn mark_batch_empty_is_noop() {
    let conn = setup_db();
    mark_batch_screening_error(&conn, &[], "error", None).expect("mark batch");
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM audit_entries", [], |row| row.get(0)).expect("query");
    assert_eq!(count, 0, "no audit entries for empty batch");
}

// -- write_article_screening_result tests --

fn make_decision(decision: &str) -> ArticleDecision {
    ArticleDecision {
        final_decision: decision.to_string(),
        reasoning: "Matches criteria.".to_string(),
        augmented_inc: vec!["inc-1".to_string()],
        augmented_exc: vec![],
        auto_label_criteria: vec![("Inclusion".to_string(), "sugar tax".to_string())],
        evidence_sections: None,
    }
}

#[test]
fn write_result_updates_article_status() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let decision = make_decision("include");

    write_article_screening_result(
        &conn,
        &id,
        &decision,
        0.9,
        Some(100),
        &["ml".to_string()],
        true,
        &[],
    )
    .expect("write");

    let status: String = conn
        .query_row("SELECT status FROM articles WHERE id = ?1", rusqlite::params![&id], |row| {
            row.get(0)
        })
        .expect("query");
    assert_eq!(status, "included", "article should be included after write");
}

#[test]
fn write_result_creates_tags() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let decision = make_decision("include");

    write_article_screening_result(
        &conn,
        &id,
        &decision,
        0.9,
        Some(100),
        &["machine-learning".to_string(), "healthcare".to_string()],
        false,
        &[],
    )
    .expect("write");

    let tag_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM article_tags WHERE article_id = ?1",
            rusqlite::params![&id],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(tag_count, 2, "should have 2 tags linked");
}

#[test]
fn write_result_creates_auto_labels() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let decision = make_decision("include");

    write_article_screening_result(&conn, &id, &decision, 0.9, None, &[], false, &[])
        .expect("write");

    let label_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM article_labels WHERE article_id = ?1",
            rusqlite::params![&id],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(label_count, 1, "should have 1 auto-label from Inclusion: sugar tax");
}

#[test]
fn write_result_saves_terms_when_flag_set() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let decision = make_decision("include");

    write_article_screening_result(
        &conn,
        &id,
        &decision,
        0.9,
        None,
        &[],
        true,
        &["sugar-tax".to_string(), "children".to_string()],
    )
    .expect("write");

    let term_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_article_terms WHERE article_id = ?1",
            rusqlite::params![&id],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(term_count, 2, "should have 2 extracted terms saved");
}

#[test]
fn write_result_skips_terms_when_flag_unset() {
    let conn = setup_db();
    let id = insert_article(&conn);
    let decision = make_decision("include");

    write_article_screening_result(
        &conn,
        &id,
        &decision,
        0.9,
        None,
        &[],
        false, // save_terms = false
        &["sugar-tax".to_string()],
    )
    .expect("write");

    let term_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_article_terms WHERE article_id = ?1",
            rusqlite::params![&id],
            |row| row.get(0),
        )
        .expect("query");
    assert_eq!(term_count, 0, "should have 0 terms saved when flag is false");
}
