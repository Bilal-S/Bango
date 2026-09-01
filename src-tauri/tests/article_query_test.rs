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
    NewArticle { title: title.to_string(), publication_year: year, ..Default::default() }
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        &[("Alpha Article", Some(2020)), ("Beta Article", Some(2021)), ("Gamma Paper", Some(2022))],
    );

    let query = ArticleQuery {
        status: None, // All view - no status filter
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
    seed_working_articles(&conn, &[("Alpha Article", Some(2020)), ("Beta Article", Some(2021))]);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert!(results.is_empty(), "Search with nonsense term should return zero results");
}

#[test]
fn test_all_view_search_case_insensitive() {
    let conn = setup_db();
    seed_working_articles(&conn, &[("Machine Learning in Healthcare", Some(2022))]);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        publication_year: Some(2023),
        ..Default::default()
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        &[("Old Article", Some(2018)), ("Mid Article", Some(2020)), ("New Article", Some(2023))],
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
        publication_year: Some(2023),
        ..Default::default()
    };
    let inserted = article_repo::insert_article(&conn, &article).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

    // Add user notes after insert
    article_repo::update_user_notes(&conn, &inserted.id, "Important finding about XYZ compound")
        .expect("notes failed");

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
    seed_working_articles(&conn, &[("Article With No Notes", Some(2020))]);

    // user_notes is NULL for this article - search should not crash
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
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
    let _d1 = article_repo::insert_article(&conn, &new_article("Dup Alpha", Some(2020)))
        .expect("insert failed");
    let _d2 = article_repo::insert_article(&conn, &new_article("Dup Beta", Some(2021)))
        .expect("insert failed");
    let _d3 = article_repo::insert_article(&conn, &new_article("Dup Gamma", Some(2022)))
        .expect("insert failed");

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 1, "Duplicate view search should filter results");
    assert_eq!(results[0].title, "Dup Alpha");
}

// ─── NOT-filters (excluded tags / labels) ──────────────────────────
//
// The Article list filter panel exposes a `NOT:` toggle on tag/label pills.
// Toggling moves the name from `tags`/`labels` (inclusion, `IN` clause) to
// `excluded_tags`/`excluded_labels` (exclusion, `NOT IN` clause). These tests
// pin the backend contract: an excluded name must filter OUT articles that have
// it, while leaving articles without it (or with no tags/labels at all).

/// Helper: insert an article, move to working, and attach a tag by name.
fn seed_article_with_tag(conn: &rusqlite::Connection, title: &str, tag_name: &str) {
    let article = new_article(title, Some(2020));
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    article_repo::move_to_working(conn, &inserted.id).expect("move failed");
    article_repo::update_article_tags(conn, &inserted.id, &[tag_name.to_string()])
        .expect("tag attach failed");
}

/// Helper: insert an article, move to working, and attach a label by name.
fn seed_article_with_label(conn: &rusqlite::Connection, title: &str, label_name: &str) {
    let article = new_article(title, Some(2020));
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    article_repo::move_to_working(conn, &inserted.id).expect("move failed");
    article_repo::update_article_labels(conn, &inserted.id, &[label_name.to_string()])
        .expect("label attach failed");
}

#[test]
fn test_excluded_tag_filters_out_tagged_articles() {
    let conn = setup_db();
    // Two articles: one tagged "ml", one untagged.
    seed_article_with_tag(&conn, "Tagged", "ml");
    let untagged = new_article("Untagged", Some(2020));
    let inserted = article_repo::insert_article(&conn, &untagged).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

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
        excluded_tags: vec!["ml".into()],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // The tagged article is filtered OUT; the untagged article passes.
    assert_eq!(titles, vec!["Untagged"]);
}

#[test]
fn test_excluded_label_filters_out_labeled_articles() {
    let conn = setup_db();
    // Two articles: one labeled "priority-read", one unlabeled.
    seed_article_with_label(&conn, "Labeled", "priority-read");
    let unlabeled = new_article("Unlabeled", Some(2020));
    let inserted = article_repo::insert_article(&conn, &unlabeled).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

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
        excluded_tags: vec![],
        excluded_labels: vec!["priority-read".into()],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // The labeled article is filtered OUT; the unlabeled article passes.
    assert_eq!(titles, vec!["Unlabeled"]);
}

#[test]
fn test_excluded_tag_case_insensitive() {
    let conn = setup_db();
    seed_article_with_tag(&conn, "Tagged", "MachineLearning");
    let untagged = new_article("Untagged", Some(2020));
    let inserted = article_repo::insert_article(&conn, &untagged).expect("insert failed");
    article_repo::move_to_working(&conn, &inserted.id).expect("move failed");

    // Filter value uses lowercase; the stored tag is mixed-case. The
    // LOWER()-based comparison in the NOT IN subquery must still match.
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
        excluded_tags: vec!["machinelearning".into()],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Untagged"]);
}

#[test]
fn test_inclusion_and_exclusion_combine() {
    let conn = setup_db();
    // Three articles:
    //  - "A" tagged "keep" + "drop"
    //  - "B" tagged "keep" only
    //  - "C" tagged "drop" only
    seed_article_with_tag(&conn, "A-keep-drop", "keep");
    {
        let a = article_repo::get_articles_by_status(&conn, "working")
            .expect("fetch")
            .into_iter()
            .find(|a| a.title == "A-keep-drop")
            .expect("find A");
        article_repo::update_article_tags(&conn, &a.id, &["keep".into(), "drop".into()])
            .expect("tag");
    }
    seed_article_with_tag(&conn, "B-keep", "keep");
    seed_article_with_tag(&conn, "C-drop", "drop");

    // Include "keep", exclude "drop": only "B" matches (has keep, does NOT have drop).
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
        tags: vec!["keep".into()],
        labels: vec![],
        excluded_tags: vec!["drop".into()],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["B-keep"]);
}

#[test]
fn test_empty_excluded_arrays_match_all() {
    // Regression guard: empty excluded_tags/excluded_labels must not filter
    // anything out (they default to empty via #[serde(default)]).
    let conn = setup_db();
    seed_working_articles(&conn, &[("One", Some(2020)), ("Two", Some(2021))]);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 2, "Empty excluded arrays must not filter anything");
}

// ─── get_next_unscreened_working_batch: after_sequence_id cursor ──────
//
// The cursor lets the screening engine advance past articles it already
// attempted in the current run (e.g. a transient LLM error left them
// unscreened). Without it, the engine would re-fetch the same batch forever.

#[test]
fn batch_fetch_no_offset_returns_all_unscreened() {
    let conn = setup_db();
    seed_working_articles(&conn, &[("A", None), ("B", None), ("C", None)]);
    let batch = article_repo::get_next_unscreened_working_batch(&conn, 10, None).expect("fetch");
    assert_eq!(batch.len(), 3, "all 3 unscreened articles returned with no offset");
}

#[test]
fn batch_fetch_offset_advances_past_cursor() {
    let conn = setup_db();
    seed_working_articles(&conn, &[("A", None), ("B", None), ("C", None), ("D", None)]);

    // First fetch: get all 4 to find the sequence_id of the first 2.
    let all = article_repo::get_next_unscreened_working_batch(&conn, 10, None).expect("fetch");
    assert_eq!(all.len(), 4);
    let second_seq = all[1].sequence_id;

    // Now fetch with offset = second_seq: should return only C and D (2 articles).
    let batch = article_repo::get_next_unscreened_working_batch(&conn, 10, Some(second_seq))
        .expect("fetch");
    assert_eq!(batch.len(), 2, "offset should advance past the first 2 articles");
    assert_eq!(batch[0].title, "C");
    assert_eq!(batch[1].title, "D");
}

#[test]
fn batch_fetch_offset_with_limit_respects_both() {
    let conn = setup_db();
    seed_working_articles(
        &conn,
        &[("A", None), ("B", None), ("C", None), ("D", None), ("E", None)],
    );

    let all = article_repo::get_next_unscreened_working_batch(&conn, 10, None).expect("fetch");
    let second_seq = all[1].sequence_id;

    // Offset past first 2, limit to 2 → should return C and D only.
    let batch =
        article_repo::get_next_unscreened_working_batch(&conn, 2, Some(second_seq)).expect("fetch");
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].title, "C");
    assert_eq!(batch[1].title, "D");
}

#[test]
fn batch_fetch_offset_beyond_last_returns_empty() {
    let conn = setup_db();
    seed_working_articles(&conn, &[("A", None), ("B", None)]);

    let all = article_repo::get_next_unscreened_working_batch(&conn, 10, None).expect("fetch");
    let last_seq = all[1].sequence_id;

    // Offset past the last article → empty.
    let batch =
        article_repo::get_next_unscreened_working_batch(&conn, 10, Some(last_seq)).expect("fetch");
    assert!(batch.is_empty(), "offset past last article should return empty");
}

#[test]
fn batch_fetch_offset_zero_is_equivalent_to_no_offset() {
    // The implementation uses `unwrap_or(0)` for the cursor, so sequence_id > 0
    // matches all (sequence_id starts at 1).
    let conn = setup_db();
    seed_working_articles(&conn, &[("A", None), ("B", None)]);

    let batch_none =
        article_repo::get_next_unscreened_working_batch(&conn, 10, None).expect("fetch");
    let batch_zero =
        article_repo::get_next_unscreened_working_batch(&conn, 10, Some(0)).expect("fetch");
    assert_eq!(batch_none.len(), batch_zero.len());
    assert_eq!(batch_none.len(), 2);
}

// ─── DOI filters (partial match + empty-DOI) ──────────────────────────
//
// The Article list filter panel exposes a free-text DOI input plus an
// "Only no DOI" checkbox. The text input emits a case-insensitive substring
// match (`LOWER(doi) LIKE '%...%'`); the checkbox emits
// `doi IS NULL OR doi = ''`. The two are mutually exclusive at the UI layer
// (the checkbox disables the input), and the backend `doi_empty` branch wins
// if both are somehow set so contradictory SQL is never emitted.

/// Helper: insert an article, move to working, with a given DOI (or None).
fn seed_article_with_doi(conn: &rusqlite::Connection, title: &str, doi: Option<&str>) {
    let mut article = new_article(title, Some(2020));
    article.doi = doi.map(|d| d.to_string());
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    article_repo::move_to_working(conn, &inserted.id).expect("move failed");
}

#[test]
fn test_doi_partial_match_finds_substring() {
    let conn = setup_db();
    seed_article_with_doi(&conn, "Elsevier One", Some("10.1016/j.iref.2025.104618"));
    seed_article_with_doi(&conn, "Springer", Some("10.1007/s10479-022-04868-0"));
    seed_article_with_doi(&conn, "No DOI", None);

    // "10.1016" matches only the Elsevier article.
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: Some("10.1016".into()),
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Elsevier One"]);
}

#[test]
fn test_doi_empty_only_returns_articles_without_doi() {
    let conn = setup_db();
    seed_article_with_doi(&conn, "Has DOI", Some("10.1007/s10479-022-04868-0"));
    seed_article_with_doi(&conn, "No DOI", None);
    seed_article_with_doi(&conn, "Empty String DOI", Some(""));

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: true,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // Both the NULL-DOI and the empty-string-DOI article pass.
    assert_eq!(titles, vec!["Empty String DOI", "No DOI"]);
}

#[test]
fn test_doi_empty_wins_over_text_when_both_set() {
    // Defense-in-depth: if the UI ever sends both `doi` (text) and
    // `doi_empty = true`, the empty-DOI branch wins so we never emit the
    // contradictory `doi LIKE '%x%' AND doi IS NULL` (which returns zero rows).
    let conn = setup_db();
    seed_article_with_doi(&conn, "Has DOI", Some("10.1007/s10479-022-04868-0"));
    seed_article_with_doi(&conn, "No DOI", None);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: Some("10.1007".into()), // would match "Has DOI" if respected
        doi_empty: true,             // but this wins -> empty-DOI filter
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // The empty-DOI branch won, so only "No DOI" is returned.
    assert_eq!(titles, vec!["No DOI"]);
}

#[test]
fn test_doi_partial_match_is_case_insensitive() {
    let conn = setup_db();
    seed_article_with_doi(&conn, "Upper", Some("10.1016/SCM-05-2021-0227"));

    // Lowercase query must match the mixed-case DOI.
    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: Some("scm-05".into()),
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Upper");
}

#[test]
fn test_doi_filter_combines_with_status_filter() {
    // DOI filter should compose with the status base filter (e.g. an article
    // with a matching DOI that is in `duplicate` status must NOT appear in the
    // `working` view even when its DOI matches the substring).
    let conn = setup_db();
    seed_article_with_doi(&conn, "Working", Some("10.1016/j.iref.2025.104618"));

    // A second article with the same DOI prefix but left in `duplicate` status.
    let mut dup_article = new_article("Dup Status", Some(2020));
    dup_article.doi = Some("10.1016/j.techfore.2024.123574".into());
    let _ = article_repo::insert_article(&conn, &dup_article).expect("insert failed");
    // Status stays 'duplicate' (the default insert status).

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: Some("10.1016".into()),
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // Only the working article with the matching DOI prefix is returned.
    assert_eq!(titles, vec!["Working"]);
}

// ─── Matched-criteria filters (specific + unknown + empty) ────────────
//
// The Article list filter panel exposes a "Match Criteria" picker (pills +
// combobox, mirroring Tags/Labels). Four backend dimensions:
// - `matched_criteria`: criterion UUIDs the article must have matched
//   (AND-combined; a UUID counts if present in EITHER array).
// - `criteria_unknown`: "Y. Unknown Criteria" - articles referencing >= 1
//   UUID no longer in `criteria` (deleted-criterion ghosts).
// - `criteria_empty`: "Z. No Criteria" - both matched arrays NULL/empty.
// - `exclusion_criteria_empty`: "X. No Exclusion Criteria" - exclusion array
//   NULL/empty, inclusion irrelevant (PRISMA "records generally excluded"
//   when combined with status = "rejected"; parity-tested in prisma_test.rs).

/// Helper: insert a working article with explicit matched-criteria JSON columns.
fn seed_article_with_criteria(
    conn: &rusqlite::Connection,
    title: &str,
    inclusion_json: Option<&str>,
    exclusion_json: Option<&str>,
) {
    let article = new_article(title, Some(2020));
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    article_repo::move_to_working(conn, &inserted.id).expect("move failed");
    conn.execute(
        "UPDATE articles SET matched_inclusion_criteria = ?1, matched_exclusion_criteria = ?2 WHERE id = ?3",
        rusqlite::params![inclusion_json, exclusion_json, inserted.id],
    )
    .expect("criteria update failed");
}

#[test]
fn matched_criteria_filter_matches_inclusion_or_exclusion() {
    let conn = setup_db();
    let inc = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Human studies",
        "critical",
    )
    .expect("criterion failed");
    let inc_json = format!(r#"["{}"]"#, inc.id);
    let exc_json = format!(r#"["{}"]"#, inc.id);
    seed_article_with_criteria(&conn, "Inclusion Match", Some(&inc_json), None);
    seed_article_with_criteria(&conn, "Exclusion Match", None, Some(&exc_json));
    seed_article_with_criteria(&conn, "Other Criterion", Some(r#"["other-id"]"#), None);
    seed_article_with_criteria(&conn, "No Match", Some("[]"), Some("[]"));

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![inc.id.clone()],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // The UUID counts whether it sits in the inclusion OR the exclusion array.
    assert_eq!(titles, vec!["Exclusion Match", "Inclusion Match"]);
}

#[test]
fn matched_criteria_filter_ands_multiple_criteria() {
    let conn = setup_db();
    let c1 = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "First criterion",
        "critical",
    )
    .expect("criterion failed");
    let c2 = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "exclusion",
        "Second criterion",
        "standard",
    )
    .expect("criterion failed");
    let both = format!(r#"["{}"]"#, c1.id);
    let only_c2 = format!(r#"["{}"]"#, c2.id);
    seed_article_with_criteria(&conn, "Has Both", Some(&both), Some(&only_c2));
    seed_article_with_criteria(&conn, "Has Only C2", None, Some(&only_c2));

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![c1.id.clone(), c2.id.clone()],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // Mirrors tags/labels: each selected UUID AND-combines.
    assert_eq!(titles, vec!["Has Both"]);
}

#[test]
fn criteria_unknown_filter_finds_deleted_criterion_ghosts() {
    let conn = setup_db();
    let live = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Live criterion",
        "critical",
    )
    .expect("criterion failed");
    let live_json = format!(r#"["{}"]"#, live.id);
    let ghost_json = r#"["ghost-uuid"]"#.to_string();
    seed_article_with_criteria(&conn, "Ghost In Inclusion", Some(&ghost_json), None);
    seed_article_with_criteria(&conn, "Ghost In Exclusion", None, Some(&ghost_json));
    seed_article_with_criteria(&conn, "Only Live", Some(&live_json), None);
    seed_article_with_criteria(&conn, "Mixed Live And Ghost", Some(&live_json), Some(&ghost_json));
    seed_article_with_criteria(&conn, "No Criteria", None, None);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: true,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // Only rows referencing a UUID missing from `criteria` pass; a live UUID
    // alongside a ghost still qualifies (per-array check, not per-row purity).
    assert_eq!(titles, vec!["Ghost In Exclusion", "Ghost In Inclusion", "Mixed Live And Ghost"]);
}

#[test]
fn criteria_empty_filter_finds_only_unassigned_articles() {
    let conn = setup_db();
    seed_article_with_criteria(&conn, "Null Arrays", None, None);
    seed_article_with_criteria(&conn, "Empty Arrays", Some("[]"), Some("[]"));
    seed_article_with_criteria(&conn, "Assigned", Some(r#"["c1"]"#), None);
    seed_article_with_criteria(&conn, "Assigned Exclusion", None, Some(r#"["c2"]"#));
    // Both columns must be empty: an inclusion match with an empty exclusion
    // array still counts as "has criteria" and must NOT match.
    seed_article_with_criteria(
        &conn,
        "Inclusion Occupied Empty Exclusion",
        Some(r#"["c9"]"#),
        Some("[]"),
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: true,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // NULL and '[]' arrays both count as "no criteria assigned" - on BOTH
    // columns (doi_empty-style literal comparison).
    assert_eq!(titles, vec!["Empty Arrays", "Null Arrays"]);
}

#[test]
fn criteria_unknown_combines_with_specific_criterion() {
    let conn = setup_db();
    let live = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Live criterion",
        "critical",
    )
    .expect("criterion failed");
    let live_json = format!(r#"["{}"]"#, live.id);
    let ghost_json = r#"["ghost-uuid"]"#.to_string();
    // "Has Both": matches the live UUID AND carries a ghost -> passes the AND.
    seed_article_with_criteria(&conn, "Has Both", Some(&live_json), Some(&ghost_json));
    // "Only Ghost": carries a ghost but never matched the live UUID -> filtered out.
    seed_article_with_criteria(&conn, "Only Ghost", None, Some(&ghost_json));
    // "Only Live": matched the live UUID but has no ghost -> filtered out.
    seed_article_with_criteria(&conn, "Only Live", Some(&live_json), None);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![live.id.clone()],
        criteria_unknown: true,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Has Both"]);
}

#[test]
fn matched_criteria_filters_tolerate_malformed_json() {
    // Legacy/hand-edited rows can hold non-JSON text in the matched columns.
    // `row_to_article` decodes those to empty arrays; the json_each branches
    // (UUID + unknown) must never error on them (json_valid CASE guards), and
    // `criteria_empty`'s exact-string comparison (`IS NULL OR = '[]'`) is
    // crash-proof by construction - though malformed values do NOT count as
    // "no criteria assigned" (the app only writes canonical JSON).
    let conn = setup_db();
    let live = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Live criterion",
        "critical",
    )
    .expect("criterion failed");
    let live_json = format!(r#"["{}"]"#, live.id);
    seed_article_with_criteria(&conn, "Malformed", Some("not json"), Some("{broken"));
    seed_article_with_criteria(&conn, "Assigned", Some(&live_json), None);
    seed_article_with_criteria(&conn, "Unassigned", None, None);

    let empty_query = ArticleQuery {
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: true,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };
    let empty_results =
        article_repo::query_articles(&conn, &empty_query).expect("criteria_empty crashed");
    let titles: Vec<&str> = empty_results.iter().map(|a| a.title.as_str()).collect();
    // Only the true NULL/'[]' row passes; malformed text is neither NULL nor
    // '[]', so it is excluded (but the query never errors on it).
    assert_eq!(titles, vec!["Unassigned"]);

    let mut specific_query = empty_query;
    specific_query.criteria_empty = false;
    specific_query.matched_criteria = vec![live.id.clone()];
    let specific_results =
        article_repo::query_articles(&conn, &specific_query).expect("matched_criteria crashed");
    assert_eq!(specific_results.len(), 1);
    assert_eq!(specific_results[0].title, "Assigned");

    let mut unknown_query = specific_query;
    unknown_query.matched_criteria = vec![];
    unknown_query.criteria_unknown = true;
    let unknown_results =
        article_repo::query_articles(&conn, &unknown_query).expect("criteria_unknown crashed");
    assert!(
        unknown_results.is_empty(),
        "malformed JSON must not surface as unknown-criteria ghosts"
    );

    let mut exclusion_empty_query = unknown_query;
    exclusion_empty_query.criteria_unknown = false;
    exclusion_empty_query.exclusion_criteria_empty = true;
    let exclusion_empty_results =
        article_repo::query_articles(&conn, &exclusion_empty_query).expect("x sentinel crashed");
    let titles: Vec<&str> = exclusion_empty_results.iter().map(|a| a.title.as_str()).collect();
    // "{broken" is neither NULL nor '[]' so it must not match - mirroring the
    // PRISMA literal comparison; the NULL-exclusion rows pass regardless of
    // their inclusion column ("Assigned" holds a live inclusion UUID).
    assert_eq!(titles, vec!["Assigned", "Unassigned"]);
}

#[test]
fn matched_criteria_defaults_filter_nothing() {
    // Regression guard: default (empty) matched-criteria fields must not
    // filter anything out (they default via #[serde(default)]).
    let conn = setup_db();
    seed_working_articles(&conn, &[("One", Some(2020)), ("Two", Some(2021))]);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(results.len(), 2, "empty matched-criteria fields must not filter");
}

#[test]
fn exclusion_criteria_empty_matches_only_empty_exclusion_array() {
    // "X. No Exclusion Criteria": the exclusion column alone decides. This is
    // deliberately WIDER than criteria_empty ("Z"): an article with matched
    // inclusion criteria but no matched exclusion criteria still matches -
    // exactly the PRISMA "records generally excluded" set on the Rejected tab.
    let conn = setup_db();
    let inc = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Human studies",
        "critical",
    )
    .expect("criterion failed");
    let inc_json = format!(r#"["{}"]"#, inc.id);
    seed_article_with_criteria(&conn, "Exclusion Assigned", None, Some(&inc_json));
    seed_article_with_criteria(&conn, "Inclusion Only", Some(&inc_json), None);
    seed_article_with_criteria(&conn, "Both Empty", None, None);
    seed_article_with_criteria(&conn, "Empty Exclusion Array", Some("[]"), Some("[]"));

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

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    // "Exclusion Assigned" is the only row with a non-empty exclusion array.
    assert_eq!(titles, vec!["Both Empty", "Empty Exclusion Array", "Inclusion Only"]);
}

#[test]
fn exclusion_criteria_empty_combines_with_specific_criterion() {
    // AND semantics: a specific UUID (counts in EITHER array) plus an empty
    // exclusion array. A UUID in the exclusion array means that array is not
    // empty, so only an inclusion-array match can satisfy both conditions.
    let conn = setup_db();
    let c1 = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Human studies",
        "critical",
    )
    .expect("criterion failed");
    let c1_json = format!(r#"["{}"]"#, c1.id);
    seed_article_with_criteria(&conn, "Exclusion Match", None, Some(&c1_json));
    seed_article_with_criteria(&conn, "Inclusion Match", Some(&c1_json), None);
    seed_article_with_criteria(&conn, "No Match", None, None);

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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![c1.id.clone()],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: true,
        limit: None,
        offset: None,
    };

    let results = article_repo::query_articles(&conn, &query).expect("query failed");
    let titles: Vec<&str> = results.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, vec!["Inclusion Match"]);
}

// `count_query_articles` powers the filtered list's true result count: the
// SAME filters as `query_articles`, with sort/limit/offset ignored.

#[test]
fn count_query_articles_matches_unpaged_query_and_ignores_limit_offset() {
    let conn = setup_db();
    let live = bango_lib::db::criteria_repo::create_criterion(
        &conn,
        "inclusion",
        "Live criterion",
        "critical",
    )
    .expect("criterion failed");
    let live_json = format!(r#"["{}"]"#, live.id);
    let ghost_json = r#"["ghost-uuid"]"#.to_string();
    seed_article_with_criteria(&conn, "Live", Some(&live_json), None);
    seed_article_with_criteria(&conn, "Ghost", None, Some(&ghost_json));
    seed_article_with_criteria(&conn, "Empty", None, None);

    let base = ArticleQuery {
        status: Some("working".into()),
        search: None,
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
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: true,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };

    // Count equals the unpaged list length for the same filters.
    let count = article_repo::count_query_articles(&conn, &base).expect("count failed");
    let unpaged = article_repo::query_articles(&conn, &base).expect("query failed");
    assert_eq!(count as usize, unpaged.len());
    assert_eq!(count, 1); // Only "Ghost" references a UUID missing from `criteria`.

    // limit/offset/sort never change the count.
    let mut paged = base;
    paged.limit = Some(1);
    paged.offset = Some(1);
    paged.sort_by = Some("title".into());
    paged.sort_dir = Some("asc".into());
    let paged_count = article_repo::count_query_articles(&conn, &paged).expect("count failed");
    assert_eq!(paged_count, count);
}

#[test]
fn count_query_articles_respects_status_and_criteria_filters() {
    let conn = setup_db();
    // Rejected set matching PRISMA "records generally excluded": G1 (inclusion
    // present, exclusion empty) + G2 (both NULL). R1 has a matched exclusion
    // criterion (not generally excluded). W1 keeps working status with the
    // same empty arrays (status must exclude it).
    seed_article_with_criteria(&conn, "G1", Some(r#"["live-uuid"]"#), Some("[]"));
    seed_article_with_criteria(&conn, "G2", None, None);
    seed_article_with_criteria(&conn, "R1", None, Some(r#"["exc-uuid"]"#));
    seed_article_with_criteria(&conn, "W1", None, None);

    // Flip G1/G2/R1 to rejected (the seed helper inserts working rows).
    conn.execute("UPDATE articles SET status = 'rejected' WHERE title IN ('G1', 'G2', 'R1')", [])
        .expect("status update");

    let query = ArticleQuery {
        status: Some("rejected".into()),
        search: None,
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

    let count = article_repo::count_query_articles(&conn, &query).expect("count failed");
    assert_eq!(count, 2); // G1 + G2; R1 (exclusion assigned) and W1 (working) excluded.
    let list = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(list.len(), 2);
}
