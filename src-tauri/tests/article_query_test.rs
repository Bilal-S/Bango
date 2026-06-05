use bango_lib::db::article_repo::{self, ArticleQuery};
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: build a minimal NewArticle with a given title and year.
fn new_article(title: &str, year: Option<i32>) -> NewArticle {
    NewArticle {
        title: title.to_string(),
        abstract_text: String::new(),
        authors: vec![],
        publication_year: year,
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec![],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        reference_type: None,
        date: None,
        author_address: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        ris_extras: None,
        import_source: None,
        data_length: None,
        token_estimate: None,
    }
}

/// Helper: insert articles and move them all to "working" status.
fn seed_working_articles(conn: &rusqlite::Connection, titles: &[(&str, Option<i32>)]) {
    for (title, year) in titles {
        let article = new_article(title, *year);
        let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
        article_repo::move_to_working(conn, &inserted.id).expect("move to working failed");
    }
}

// ─── Sort direction tests ─────────────────────────────────────────

#[test]
fn test_sort_direction_lowercase_asc() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Bravo", Some(2020)), ("Alpha", Some(2021)), ("Charlie", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Alpha", "Bravo", "Charlie"]);
}

#[test]
fn test_sort_direction_uppercase_asc() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Bravo", Some(2020)), ("Alpha", Some(2021)), ("Charlie", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("title".into()),
        sort_dir: Some("ASC".into()), // uppercase
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Alpha", "Bravo", "Charlie"]);
}

#[test]
fn test_sort_direction_desc() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Alpha", Some(2020)), ("Charlie", Some(2021)), ("Bravo", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("title".into()),
        sort_dir: Some("desc".into()),
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Charlie", "Bravo", "Alpha"]);
}

#[test]
fn test_sort_direction_mixed_case() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Bravo", Some(2020)), ("Alpha", Some(2021)), ("Charlie", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("title".into()),
        sort_dir: Some("Asc".into()), // mixed case
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Alpha", "Bravo", "Charlie"]);
}

// ─── Case-insensitive text sorting ────────────────────────────────

#[test]
fn test_sort_title_case_insensitive() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("banana", Some(2020)), ("Apple", Some(2021)), ("cherry", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // COLLATE NOCASE: Apple < banana < cherry
    assert_eq!(titles, vec!["Apple", "banana", "cherry"]);
}

// ─── Numeric sorting ──────────────────────────────────────────────

#[test]
fn test_sort_publication_year_numeric() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Article A", Some(2020)),
            ("Article B", Some(1999)),
            ("Article C", Some(2010)),
            ("Article D", Some(2005)),
        ],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("publicationYear".into()),
        sort_dir: Some("asc".into()),
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let years: Vec<Option<i32>> = results.iter().map(|a| a.publication_year).collect();
    assert_eq!(years, vec![Some(1999), Some(2005), Some(2010), Some(2020)]);
}

// ─── Pagination with sorting ──────────────────────────────────────

#[test]
fn test_pagination_page_1_of_sorted_results() {
    let conn = setup_db();
    // Insert 5 articles with known titles
    seed_working_articles(
        &conn,
        &[
            ("Echo", Some(2020)),
            ("Alpha", Some(2020)),
            ("Delta", Some(2020)),
            ("Bravo", Some(2020)),
            ("Charlie", Some(2020)),
        ],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: Some(2),
        offset: Some(0),
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Alpha", "Bravo"]);
}

#[test]
fn test_pagination_page_2_of_sorted_results() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Echo", Some(2020)),
            ("Alpha", Some(2020)),
            ("Delta", Some(2020)),
            ("Bravo", Some(2020)),
            ("Charlie", Some(2020)),
        ],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: Some(2),
        offset: Some(2), // page 2
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Charlie", "Delta"]);
}

#[test]
fn test_pagination_page_3_of_sorted_results() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Echo", Some(2020)),
            ("Alpha", Some(2020)),
            ("Delta", Some(2020)),
            ("Bravo", Some(2020)),
            ("Charlie", Some(2020)),
        ],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: Some(2),
        offset: Some(4), // page 3
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Echo"]);
}

// ─── "All" view includes merged duplicates ────────────────────────

#[test]
fn test_all_view_includes_merged_duplicates() {
    let conn = setup_db();

    // Insert two articles, move to working
    let a1 = article_repo::insert_article(&conn, &new_article("Article 1", Some(2020)))
        .expect("insert failed");
    article_repo::move_to_working(&conn, &a1.id).expect("move failed");

    let a2 = article_repo::insert_article(&conn, &new_article("Article 2", Some(2021)))
        .expect("insert failed");
    article_repo::move_to_working(&conn, &a2.id).expect("move failed");

    // Insert a duplicate and mark it as merged-away (duplicate_of = a1.id)
    let dup = article_repo::insert_article(&conn, &new_article("Article 1 Dup", Some(2020)))
        .expect("insert failed");
    article_repo::mark_as_duplicate(&conn, &dup.id, &a1.id).expect("mark dup failed");

    // "All" view (status = None) should return all 3 articles including the merged one
    let query = ArticleQuery {
        status: None, // All view
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
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 3, "All view should include merged duplicates");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert!(titles.contains(&"Article 1 Dup"), "Should contain merged duplicate");
}

#[test]
fn test_working_view_excludes_merged_duplicates() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Article 1", Some(2020)))
        .expect("insert failed");
    article_repo::move_to_working(&conn, &a1.id).expect("move failed");

    let a2 = article_repo::insert_article(&conn, &new_article("Article 2", Some(2021)))
        .expect("insert failed");
    article_repo::move_to_working(&conn, &a2.id).expect("move failed");

    let dup = article_repo::insert_article(&conn, &new_article("Article 1 Dup", Some(2020)))
        .expect("insert failed");
    article_repo::move_to_working(&conn, &dup.id).expect("move failed");
    article_repo::mark_as_duplicate(&conn, &dup.id, &a1.id).expect("mark dup failed");

    // Working view should exclude the merged duplicate
    let query = ArticleQuery {
        status: Some("working".into()),
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
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 2, "Working view should exclude merged duplicates");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert!(!titles.contains(&"Article 1 Dup"), "Should NOT contain merged duplicate");
}

#[test]
fn test_duplicate_view_shows_all_duplicates() {
    let conn = setup_db();

    let a1 = article_repo::insert_article(&conn, &new_article("Original", Some(2020)))
        .expect("insert failed");

    let dup1 = article_repo::insert_article(&conn, &new_article("Dup 1", Some(2020)))
        .expect("insert failed");
    article_repo::mark_as_duplicate(&conn, &dup1.id, &a1.id).expect("mark dup failed");

    let _dup2 = article_repo::insert_article(&conn, &new_article("Dup 2", Some(2020)))
        .expect("insert failed");
    // dup2 stays as status=duplicate but is NOT merged (no duplicate_of)

    let query = ArticleQuery {
        status: Some("duplicate".into()),
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
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 3, "Duplicate view should show all duplicates (merged or not)");
}

// ─── Default sort direction (none specified) ──────────────────────

#[test]
fn test_default_sort_direction_is_desc() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Alpha", Some(2020)), ("Charlie", Some(2021)), ("Bravo", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("title".into()),
        sort_dir: None, // no direction specified
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // Default should be DESC
    assert_eq!(titles, vec!["Charlie", "Bravo", "Alpha"]);
}

// ─── Sequence ID (index) sorting ──────────────────────────────────

#[test]
fn test_sort_by_sequence_id_asc() {
    let conn = setup_db();
    // Articles are assigned sequential sequence_ids (1, 2, 3) in insertion order
    seed_working_articles(
        &conn,
        &[("Third", Some(2020)), ("First", Some(2021)), ("Second", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("index".into()),
        sort_dir: Some("asc".into()),
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // ASC order: insertion order = Third, First, Second
    assert_eq!(titles, vec!["Third", "First", "Second"]);
}

#[test]
fn test_sort_by_sequence_id_desc() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Third", Some(2020)), ("First", Some(2021)), ("Second", Some(2022))],
    );

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: Some("index".into()),
        sort_dir: Some("desc".into()),
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // DESC order: reverse insertion = Second, First, Third
    assert_eq!(titles, vec!["Second", "First", "Third"]);
}
