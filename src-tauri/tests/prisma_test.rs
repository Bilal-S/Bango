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
