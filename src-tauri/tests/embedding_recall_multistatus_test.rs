//! Integration tests for the multi-status extension to `list_for_recall` /
//! `recall` (cf2.md §7).
//!
//! The Citation Finder needs `working + included` while excluding
//! `duplicate`/`rejected`. The previous `Option<&str>` single-status signature
//! could not express that, so the API was extended to `&[String]` with
//! `status IN (?, ?, ?)` SQL. These tests pin the new contract.

use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow, TITLE_ABSTRACT_CHUNK_INDEX};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn seed_article(conn: &Connection, id: &str, status: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, ?2, 'Author', 'Abstract', ?3, 'test')",
        rusqlite::params![id, format!("Title {id}"), status],
    )
    .expect("seed article");
}

fn insert_vec(conn: &Connection, article_id: &str, chunk_index: i32, vec: &[f32]) {
    embedding_repo::insert_embedding(
        conn,
        &NewEmbeddingRow {
            article_id,
            chunk_index,
            embedding: vec,
            dimensions: vec.len() as i32,
            input_hash: "test-hash",
            model_name: "test-model",
            provider: "test",
            generated_at: 1,
        },
    )
    .expect("insert embedding");
}

/// Empty filter slice = no WHERE clause on status → returns rows for ALL
/// articles regardless of status. This is the historical `None` behavior.
#[test]
fn empty_filter_returns_all_statuses() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "wk", "working");
    seed_article(&conn, "rej", "rejected");
    seed_article(&conn, "dup", "duplicate");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "wk", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "rej", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "dup", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let all = embedding_repo::list_for_recall(&conn, 4, &[]).unwrap();
    assert_eq!(all.len(), 4, "empty filter = all 4 articles");
}

/// Single-element slice reproduces the historical `Some("included")` behavior.
#[test]
fn single_status_filter_matches_historical_behavior() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "wk", "working");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "wk", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let incl = embedding_repo::list_for_recall(&conn, 4, &["included".to_string()]).unwrap();
    assert_eq!(incl.len(), 1);
    assert_eq!(incl[0].article_id, "inc");
}

/// The Citation Finder's primary use case: `working + included` while excluding
/// `rejected` and `duplicate`.
#[test]
fn multi_status_filter_working_plus_included() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "wk", "working");
    seed_article(&conn, "rej", "rejected");
    seed_article(&conn, "dup", "duplicate");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "wk", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "rej", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "dup", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let hits =
        embedding_repo::list_for_recall(&conn, 4, &["working".to_string(), "included".to_string()])
            .unwrap();
    let mut ids: Vec<String> = hits.into_iter().map(|r| r.article_id).collect();
    ids.sort();
    assert_eq!(ids, vec!["inc", "wk"], "working + included only; rejected + duplicate excluded");
}

/// Three statuses in the filter (e.g. if the user also checks Rejected in the
/// Citation Finder's domain filter).
#[test]
fn three_status_filter() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "wk", "working");
    seed_article(&conn, "rej", "rejected");
    seed_article(&conn, "dup", "duplicate");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "wk", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "rej", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "dup", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let hits = embedding_repo::list_for_recall(
        &conn,
        4,
        &["working".to_string(), "included".to_string(), "rejected".to_string()],
    )
    .unwrap();
    assert_eq!(hits.len(), 3, "working + included + rejected; duplicate excluded");
}

/// A status in the filter that no article has → returns zero rows for that
/// status, but other matching statuses still return their rows.
#[test]
fn status_filter_with_unmatched_status_still_returns_matched() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let hits = embedding_repo::list_for_recall(
        &conn,
        4,
        &["included".to_string(), "rejected".to_string()],
    )
    .unwrap();
    assert_eq!(hits.len(), 1, "the unmatched 'rejected' adds zero rows; 'included' still matches");
    assert_eq!(hits[0].article_id, "inc");
}

/// A filter where NO article has any of the requested statuses → empty.
#[test]
fn status_filter_all_unmatched_returns_empty() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let hits =
        embedding_repo::list_for_recall(&conn, 4, &["working".to_string(), "rejected".to_string()])
            .unwrap();
    assert!(hits.is_empty(), "no article has working or rejected status");
}

/// The dimension filter still applies on top of the multi-status filter.
#[test]
fn multi_status_filter_combined_with_dimension_filter() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc4", "included");
    seed_article(&conn, "wk8", "working");

    insert_vec(&conn, "inc4", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]); // 4-dim, included
    insert_vec(&conn, "wk8", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 8]); // 8-dim, working

    // Query dim=4: only inc4 matches (dimension + status).
    let hits4 =
        embedding_repo::list_for_recall(&conn, 4, &["working".to_string(), "included".to_string()])
            .unwrap();
    assert_eq!(hits4.len(), 1);
    assert_eq!(hits4[0].article_id, "inc4");

    // Query dim=8: only wk8 matches.
    let hits8 =
        embedding_repo::list_for_recall(&conn, 8, &["working".to_string(), "included".to_string()])
            .unwrap();
    assert_eq!(hits8.len(), 1);
    assert_eq!(hits8[0].article_id, "wk8");
}

/// Per-article rows in multiple statuses are all counted (max-pool happens in
/// `recall`, not `list_for_recall` - this test confirms the row count).
#[test]
fn multi_status_filter_returns_all_chunks_for_matched_articles() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");

    // 3 rows for the included article.
    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "inc", 0, &[0.2; 4]);
    insert_vec(&conn, "inc", 1, &[0.3; 4]);

    let hits = embedding_repo::list_for_recall(&conn, 4, &["included".to_string()]).unwrap();
    assert_eq!(hits.len(), 3, "all 3 chunk rows returned for the matched article");
}
