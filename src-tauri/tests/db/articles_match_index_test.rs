//! v010 match-acceleration indexes on `articles`.
//!
//! `reference_repo::auto_match_paper_to_article` probes `articles` with
//! `LOWER(doi)` / `LOWER(title)` predicates. Before v010 neither column had
//! any index, so every probe was a full scan of the widest table in the DB -
//! multiplied by every unmatched reference paper, this dominated
//! `biblio_normalize` runtime on libraries with harvested references. These
//! tests pin index presence, planner usage, and re-run idempotency.

use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::migrations::v010_articles_match_indexes;
use rusqlite::Connection;

fn index_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .expect("index existence query runs");
    count > 0
}

fn query_plan_detail(conn: &Connection, sql: &str) -> String {
    // EXPLAIN QUERY PLAN column layouts differ across SQLite builds (3 vs 4
    // columns), so concatenate every cell of every row instead of pinning an
    // index; the index-name text lives in the detail column.
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain).expect("EXPLAIN QUERY PLAN prepares");
    let columns = stmt.column_count();
    let mut out = String::new();
    let mut rows = stmt.query([]).expect("EXPLAIN QUERY PLAN runs");
    while let Ok(Some(row)) = rows.next() {
        for i in 0..columns {
            match row.get::<_, rusqlite::types::Value>(i) {
                Ok(rusqlite::types::Value::Text(text)) => out.push_str(&text),
                Ok(other) => out.push_str(&format!("{other:?}")),
                Err(_) => {}
            }
            out.push(' ');
        }
    }
    out
}

#[test]
fn migrated_database_has_articles_match_indexes() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations run");
    assert!(
        index_exists(&conn, "idx_articles_doi_lower"),
        "idx_articles_doi_lower must exist after the full migration chain"
    );
    assert!(
        index_exists(&conn, "idx_articles_title_lower"),
        "idx_articles_title_lower must exist after the full migration chain"
    );
}

#[test]
fn doi_and_title_probes_use_the_expression_indexes() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations run");
    // At least one row so the planner has representative statistics.
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, doi, journal, publication_year) \
         VALUES ('a1', 'included', 'Title A', 'x', '[\"A, B\"]', '10.0000/a', 'Nature', 2020)",
        [],
    )
    .expect("seed article");

    let doi_plan =
        query_plan_detail(&conn, "SELECT id FROM articles WHERE LOWER(doi) = LOWER('10.0000/a')");
    assert!(
        doi_plan.contains("idx_articles_doi_lower"),
        "LOWER(doi) probe must use the expression index, got: {doi_plan}"
    );

    let title_plan = query_plan_detail(
        &conn,
        "SELECT id FROM articles WHERE LOWER(title) = LOWER('Title A') AND publication_year = 2020",
    );
    assert!(
        title_plan.contains("idx_articles_title_lower"),
        "LOWER(title) probe must use the expression index, got: {title_plan}"
    );
}

#[test]
fn v010_up_sql_is_idempotent() {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations run");
    // Re-executing the batch (e.g. after a partial-heal scenario) must not fail.
    conn.execute_batch(v010_articles_match_indexes::UP_SQL).expect("re-run v010 UP_SQL");
    assert!(index_exists(&conn, "idx_articles_doi_lower"));
    assert!(index_exists(&conn, "idx_articles_title_lower"));
}
