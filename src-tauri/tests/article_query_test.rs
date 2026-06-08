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
        num_cited: None,
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
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

// ─── Search on "All" tab (status = None) ──────────────────────────

/// Regression test: search on the All tab (status = None) must not fail.
/// Before the WHERE 1=1 fix, appending "AND (LOWER(title) LIKE ...)" without
/// a preceding WHERE clause produced invalid SQL, silently returning stale results.
#[test]
fn test_all_view_search_filters_by_title() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Alpha Article", Some(2020)),
            ("Beta Article", Some(2021)),
            ("Gamma Paper", Some(2022)),
        ],
    );

    let query = ArticleQuery {
        status: None, // All view — no status filter
        search: Some("article".into()),
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
    assert_eq!(titles, vec!["Alpha Article", "Beta Article"]);
}

#[test]
fn test_all_view_search_no_results() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Alpha Article", Some(2020)),
            ("Beta Article", Some(2021)),
        ],
    );

    let query = ArticleQuery {
        status: None,
        search: Some("nonexistent_xyz".into()),
        sort_by: None,
        sort_dir: None,
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
    assert!(results.is_empty(), "Search with nonsense term should return zero results");
}

#[test]
fn test_all_view_search_case_insensitive() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Machine Learning in Healthcare", Some(2022))],
    );

    // Search with lowercase should match mixed-case title
    let query = ArticleQuery {
        status: None,
        search: Some("machine learning".into()),
        sort_by: None,
        sort_dir: None,
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
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Machine Learning in Healthcare");
}

#[test]
fn test_all_view_search_matches_abstract() {
    let conn = setup_db();

    // Insert one article with a known abstract
    let article = NewArticle {
        title: "Some Title".to_string(),
        abstract_text: "This paper discusses quantum computing.".to_string(),
        authors: vec![],
        publication_year: Some(2023),
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
        num_cited: None,
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
    };
    let inserted = article_repo::insert_article(&conn, &article).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

    let query = ArticleQuery {
        status: None,
        search: Some("quantum".into()),
        sort_by: None,
        sort_dir: None,
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
    assert_eq!(results.len(), 1, "Search should match abstract text");
}

#[test]
fn test_all_view_search_with_pagination() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Alpha One", Some(2020)),
            ("Alpha Two", Some(2021)),
            ("Alpha Three", Some(2022)),
            ("Beta One", Some(2023)),
        ],
    );

    // Page 1: limit 2, offset 0
    let query = ArticleQuery {
        status: None,
        search: Some("alpha".into()),
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
    assert_eq!(titles, vec!["Alpha One", "Alpha Three"]);

    // Page 2: limit 2, offset 2
    let query2 = ArticleQuery {
        status: None,
        search: Some("alpha".into()),
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
        offset: Some(2),
    };
    let results2 = article_repo::query_articles(&conn, &query2).expect("query failed");
    let titles2: Vec<&str> = results2.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles2, vec!["Alpha Two"]);
}

#[test]
fn test_all_view_year_filter() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[
            ("Old Article", Some(2018)),
            ("Mid Article", Some(2020)),
            ("New Article", Some(2023)),
        ],
    );

    let query = ArticleQuery {
        status: None,
        search: None,
        sort_by: Some("title".into()),
        sort_dir: Some("asc".into()),
        year_from: Some(2019),
        year_to: Some(2021),
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
    assert_eq!(titles, vec!["Mid Article"]);
}

#[test]
fn test_search_matches_user_notes() {
    let conn = setup_db();

    // Insert article with no keywords in title or abstract
    let article = NewArticle {
        title: "Generic Title".to_string(),
        abstract_text: "Generic abstract text.".to_string(),
        authors: vec![],
        publication_year: Some(2023),
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
        num_cited: None,
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
    };
    let inserted = article_repo::insert_article(&conn, &article).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

    // Add user notes after insert
    article_repo::update_user_notes(&conn, &inserted.id, "Important finding about XYZ compound").expect("notes failed");

    let query = ArticleQuery {
        status: Some("working".into()),
        search: Some("xyz compound".into()),
        sort_by: None,
        sort_dir: None,
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
    assert_eq!(results.len(), 1, "Search should match user_notes text");
    assert_eq!(results[0].title, "Generic Title");
}

#[test]
fn test_search_user_notes_null_no_crash() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("Article With No Notes", Some(2020))],
    );

    // user_notes is NULL for this article — search should not crash
    let query = ArticleQuery {
        status: Some("working".into()),
        search: Some("something".into()),
        sort_by: None,
        sort_dir: None,
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
    assert!(results.is_empty(), "NULL user_notes should not cause false positives");
}

#[test]
fn test_duplicate_view_search_filters_results() {
    let conn = setup_db();

    // Insert duplicates (status stays 'duplicate')
    let _d1 = article_repo::insert_article(&conn, &new_article("Dup Alpha", Some(2020))).expect("insert failed");
    let _d2 = article_repo::insert_article(&conn, &new_article("Dup Beta", Some(2021))).expect("insert failed");
    let _d3 = article_repo::insert_article(&conn, &new_article("Dup Gamma", Some(2022))).expect("insert failed");

    let query = ArticleQuery {
        status: Some("duplicate".into()),
        search: Some("alpha".into()),
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
    assert_eq!(results.len(), 1, "Duplicate view search should filter results");
    assert_eq!(results[0].title, "Dup Alpha");
}
