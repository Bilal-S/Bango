use bango_lib::db::article_repo::{self, ArticleQuery};
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::prisma::data::compute_prisma_data;

#[test]
fn test_prisma_counts_from_empty_database() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let data = compute_prisma_data(&conn).unwrap();
    assert_eq!(data.records_identified, 0);
    assert_eq!(data.duplicates_removed, 0);
    assert_eq!(data.records_screened, 0);
    assert_eq!(data.records_excluded, 0);
    assert_eq!(data.studies_included, 0);
}

#[test]
fn test_prisma_counts_with_articles() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // Insert articles in various states
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a1', 'duplicate', 'T1', 'A1', '[]')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a2', 'working', 'T2', 'A2', '[]')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a3', 'included', 'T3', 'A3', '[]')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a4', 'rejected', 'T4', 'A4', '[]')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, duplicate_of) VALUES ('a5', 'duplicate', 'T5', 'A5', '[]', 'a1')",
        [],
    )
    .unwrap();

    let data = compute_prisma_data(&conn).unwrap();
    assert_eq!(data.records_identified, 5); // All articles
    assert_eq!(data.duplicates_removed, 1); // a5 has duplicate_of
    assert_eq!(data.studies_included, 1); // Only a3
    assert_eq!(data.records_excluded, 1); // Only a4
}

/// Parity guard: Rejected tab + "X. No Exclusion Criteria" must reproduce the
/// PRISMA `records_excluded_general` count exactly - including rejected rows
/// whose matched INCLUSION criteria are non-empty (the case the stricter
/// "Z. No Criteria" both-arrays filter would miss).
#[test]
fn test_rejected_plus_exclusion_criteria_empty_matches_prisma_general_excluded() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let seed = |id: &str, status: &str, inc: Option<&str>, exc: Option<&str>| {
        conn.execute(
            "INSERT INTO articles (id, status, title, abstract_text, authors, matched_inclusion_criteria, matched_exclusion_criteria) \
             VALUES (?1, ?2, ?3, 'A', '[]', ?4, ?5)",
            rusqlite::params![id, status, format!("Title {id}"), inc, exc],
        )
        .unwrap();
    };

    // General-excluded: rejected with empty exclusion arrays (NULL or '[]').
    // g1 has a matched INCLUSION criterion yet still counts as generally
    // excluded - the inclusion column is irrelevant to the PRISMA definition.
    seed("g1", "rejected", Some(r#"["live-uuid"]"#), Some("[]"));
    seed("g2", "rejected", None, None);
    // With reasons: rejected with a non-empty exclusion array.
    seed("r1", "rejected", None, Some(r#"["exc-uuid"]"#));
    // Empty exclusion arrays on non-rejected rows must NOT count.
    seed("w1", "working", None, None);
    seed("i1", "included", Some("[]"), Some("[]"));

    let data = compute_prisma_data(&conn).unwrap();
    assert_eq!(data.records_excluded_general, 2); // g1 + g2
    assert_eq!(data.records_excluded_with_reasons, 1); // r1

    let query = ArticleQuery {
        status: Some("rejected".into()),
        search: None,
        sort_by: Some("title".into()),
        sort_dir: Some("asc".into()),
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: true,
        limit: None,
        offset: None,
    };
    let results = article_repo::query_articles(&conn, &query).unwrap();
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles.len(), data.records_excluded_general);
    assert_eq!(titles, vec!["Title g1", "Title g2"]);
}
