use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

#[test]
fn test_database_initializes_with_all_tables() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("Failed to prepare query")
        .query_map([], |row| row.get(0))
        .expect("Failed to query")
        .filter_map(|r| r.ok())
        .collect();

    assert!(tables.contains(&"articles".to_string()), "Missing articles table");
    assert!(tables.contains(&"criteria".to_string()), "Missing criteria table");
    assert!(tables.contains(&"research_aims".to_string()), "Missing research_aims table");
    assert!(tables.contains(&"tags".to_string()), "Missing tags table");
    assert!(tables.contains(&"labels".to_string()), "Missing labels table");
    assert!(tables.contains(&"audit_entries".to_string()), "Missing audit_entries table");
    assert!(tables.contains(&"llm_config".to_string()), "Missing llm_config table");
    assert!(tables.contains(&"article_tags".to_string()), "Missing article_tags table");
    assert!(tables.contains(&"article_labels".to_string()), "Missing article_labels table");
}

#[test]
fn test_migrations_are_idempotent() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("First migration run failed");
    run_migrations(&conn).expect("Second migration run should succeed");
}

#[test]
fn test_database_stores_all_article_fields() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, doi, publication_year, journal, keywords) VALUES ('test-1', 'duplicate', 'Test Title', 'Test Abstract', '[\"Author, A\"]', '10.1234/test', 2024, 'Test Journal', '[\"keyword1\"]')",
        [],
    ).expect("Insert failed");

    let title: String = conn
        .query_row("SELECT title FROM articles WHERE id = 'test-1'", [], |row| row.get(0))
        .expect("Query failed");
    assert_eq!(title, "Test Title");

    let doi: Option<String> = conn
        .query_row("SELECT doi FROM articles WHERE id = 'test-1'", [], |row| row.get(0))
        .expect("Query failed");
    assert_eq!(doi, Some("10.1234/test".to_string()));
}

/// Test E: migration v003 must create the `article_chunks` table and set
/// `user_version = 3`. This catches the migration not being registered in
/// `migrations/mod.rs::get_migrations()`.
#[test]
fn test_migration_v003_creates_article_chunks_table_and_sets_user_version() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // user_version must be 3 (v001 + v002 + v003).
    let version: i64 =
        conn.query_row("PRAGMA user_version", [], |row| row.get(0)).expect("PRAGMA failed");
    assert_eq!(version, 3, "user_version must be 3 after migrations v001-v003");

    // article_chunks table must exist.
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='article_chunks'",
            [],
            |row| row.get(0),
        )
        .expect("Query failed");
    assert_eq!(exists, 1, "article_chunks table must exist after migration v003");

    // Its schema must include the expected columns.
    let has_chunk_index: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('article_chunks') WHERE name='chunk_index'",
            [],
            |row| row.get(0),
        )
        .expect("pragma_table_info failed");
    assert_eq!(has_chunk_index, 1, "article_chunks.chunk_index column must exist");
}
