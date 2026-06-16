use rusqlite::Connection;

use bango_lib::db::biblio_repo::{
    build_coauthor_edges, clear_all_biblio, clear_regeneratable_biblio, compute_author_metrics,
    compute_h_index, get_all_authors, get_all_terms, get_author_detail,
    get_author_productivity_kpis, get_author_rankings, get_authors_for_article, get_biblio_kpis,
    get_biblio_status, get_journal_year_data, get_terms_for_article, link_article_author,
    link_article_term, save_article_terms, upsert_author, upsert_institution, upsert_term,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::models::biblio::{JournalYearData, TermSource, TermType, YearCount};

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn insert_test_article(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) VALUES (?1, 'Test', 'Abstract', 'Smith J', 'included')",
        rusqlite::params![id],
    ).unwrap();
}

// ── Term operations ─────────────────────────────────────────

#[test]
fn test_upsert_term_creates_new() {
    let conn = test_db();
    let id = upsert_term(
        &conn,
        "Machine Learning",
        "machine learning",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();
    assert!(!id.is_empty());

    let count: i32 = conn
        .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_upsert_term_increments_count() {
    let conn = test_db();
    let id1 = upsert_term(
        &conn,
        "Machine Learning",
        "machine learning",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();
    let id2 = upsert_term(
        &conn,
        "machine learning",
        "machine learning",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();
    assert_eq!(id1, id2);

    let count: i32 = conn
        .query_row("SELECT article_count FROM biblio_terms WHERE id = ?1", [&id1], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_upsert_term_different_types() {
    let conn = test_db();
    let id_kw = upsert_term(&conn, "ML", "ml", &TermType::Keyword, &TermSource::Metadata).unwrap();
    let id_np =
        upsert_term(&conn, "ML", "ml", &TermType::NounPhrase, &TermSource::AiExtracted).unwrap();
    assert_ne!(id_kw, id_np);
}

#[test]
fn test_link_article_term_creates_link() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    let term_id =
        upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
    link_article_term(&conn, "art1", &term_id).unwrap();

    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM biblio_article_terms WHERE article_id = ?1",
            ["art1"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_link_article_term_increments_frequency() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    let term_id =
        upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
    link_article_term(&conn, "art1", &term_id).unwrap();
    link_article_term(&conn, "art1", &term_id).unwrap();

    let freq: i32 = conn
        .query_row(
            "SELECT frequency FROM biblio_article_terms WHERE article_id = ?1 AND term_id = ?2",
            rusqlite::params!["art1", term_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(freq, 2);
}

#[test]
fn test_get_terms_for_article() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    let t1 = upsert_term(
        &conn,
        "Machine Learning",
        "machine learning",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();
    let t2 = upsert_term(
        &conn,
        "neural network",
        "neural network",
        &TermType::NounPhrase,
        &TermSource::AiExtracted,
    )
    .unwrap();
    link_article_term(&conn, "art1", &t1).unwrap();
    link_article_term(&conn, "art1", &t2).unwrap();

    let terms = get_terms_for_article(&conn, "art1").unwrap();
    assert_eq!(terms.len(), 2);
}

#[test]
fn test_save_article_terms() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    save_article_terms(
        &conn,
        "art1",
        &[
            ("Machine Learning".to_string(), TermType::Keyword, TermSource::Metadata),
            ("deep learning".to_string(), TermType::NounPhrase, TermSource::AiExtracted),
            ("machine learning".to_string(), TermType::Keyword, TermSource::Metadata), // duplicate normalized
        ],
    )
    .unwrap();

    let terms = get_terms_for_article(&conn, "art1").unwrap();
    assert_eq!(terms.len(), 2); // "machine learning" deduplicated
}

// ── Author operations ───────────────────────────────────────

#[test]
fn test_upsert_author_creates_new() {
    let conn = test_db();
    let id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    assert!(!id.is_empty());
}

#[test]
fn test_upsert_author_increments_count() {
    let conn = test_db();
    let id1 = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    let id2 = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    assert_eq!(id1, id2);

    let count: i32 = conn
        .query_row("SELECT article_count FROM biblio_authors WHERE id = ?1", [&id1], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_link_article_author() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    let author_id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    link_article_author(&conn, "art1", &author_id, 0, Some("Smith J"), None).unwrap();

    let links = get_authors_for_article(&conn, "art1").unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].author_order, 0);
}

#[test]
fn test_first_author_count_updated() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    let author_id = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    link_article_author(&conn, "art1", &author_id, 0, None, None).unwrap();

    let count: i32 = conn
        .query_row(
            "SELECT first_author_count FROM biblio_authors WHERE id = ?1",
            [&author_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

// ── Institution operations ──────────────────────────────────

#[test]
fn test_upsert_institution_creates_new() {
    let conn = test_db();
    let (id, was_created) =
        upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
    assert!(!id.is_empty());
    assert!(was_created);
}

#[test]
fn test_upsert_institution_returns_same() {
    let conn = test_db();
    let (id1, was_created1) =
        upsert_institution(&conn, "mit", Some("USA"), Some("Cambridge")).unwrap();
    let (id2, was_created2) = upsert_institution(&conn, "mit", None, None).unwrap();
    assert_eq!(id1, id2);
    assert!(was_created1);
    assert!(!was_created2);
}

// ── Clear and Status ────────────────────────────────────────

#[test]
fn test_clear_all_biblio() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
    upsert_author(&conn, "smith j", "Smith, J.").unwrap();

    clear_all_biblio(&conn).unwrap();

    let status = get_biblio_status(&conn).unwrap();
    assert_eq!(status.author_count, 0);
    assert_eq!(status.term_count, 0);
}

#[test]
fn test_get_biblio_status() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    upsert_term(&conn, "AI", "ai", &TermType::Keyword, &TermSource::Metadata).unwrap();
    upsert_author(&conn, "smith j", "Smith, J.").unwrap();

    let status = get_biblio_status(&conn).unwrap();
    assert_eq!(status.author_count, 1);
    assert_eq!(status.term_count, 1);
}

#[test]
fn test_refresh_clears_and_repopulates() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");

    // Populate
    save_article_terms(
        &conn,
        "art1",
        &[("ML".to_string(), TermType::Keyword, TermSource::Metadata)],
    )
    .unwrap();
    save_article_terms(
        &conn,
        "art2",
        &[("AI".to_string(), TermType::Keyword, TermSource::Metadata)],
    )
    .unwrap();

    let status_before = get_biblio_status(&conn).unwrap();
    assert_eq!(status_before.term_count, 2);
    assert_eq!(status_before.article_term_links, 2);

    // Clear and repopulate
    clear_all_biblio(&conn).unwrap();
    let status_cleared = get_biblio_status(&conn).unwrap();
    assert_eq!(status_cleared.term_count, 0);

    // Repopulate
    save_article_terms(
        &conn,
        "art1",
        &[("ML".to_string(), TermType::Keyword, TermSource::Metadata)],
    )
    .unwrap();
    save_article_terms(
        &conn,
        "art2",
        &[("AI".to_string(), TermType::Keyword, TermSource::Metadata)],
    )
    .unwrap();

    let status_after = get_biblio_status(&conn).unwrap();
    assert_eq!(status_after.term_count, 2);
    assert_eq!(status_after.article_term_links, 2);
}

// ── KPI tests ──────────────────────────────────────────────────

/// Helper: insert an article with full control over key KPI fields.
/// `pub_year` is an Option<i32> matching the INTEGER publication_year column.
/// `journal` and `journal_index_id` support the timeline journal_distribution tests.
#[allow(clippy::too_many_arguments)]
fn insert_kpi_article(
    conn: &Connection,
    id: &str,
    status: &str,
    pub_year: Option<i32>,
    num_cited: Option<i32>,
    authors: &str,
    journal: Option<&str>,
    journal_index_id: Option<&str>,
) {
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited, journal, journal_index_id)
         VALUES (?1, 'Test', 'Abstract', ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, authors, status, pub_year, num_cited, journal, journal_index_id],
    )
    .unwrap();
}

#[test]
fn kpi_empty_db_returns_zeros() {
    let conn = test_db();
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.included_count, 0);
    assert_eq!(kpis.total_citations, 0);
    assert_eq!(kpis.unique_authors, 0);
    assert_eq!(kpis.year_from, None);
    assert_eq!(kpis.year_to, None);
    assert_eq!(kpis.pubs_per_year, None);
    assert!(kpis.pubs_by_year.is_empty());
    assert_eq!(kpis.avg_growth_rate, None);
    assert!(kpis.refs_by_year.is_empty());
}

#[test]
fn kpi_rejected_only_returns_zeros() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "rejected", Some(2020), Some(5), "Smith J", None, None);
    insert_kpi_article(&conn, "a2", "duplicate", Some(2021), Some(10), "Doe A", None, None);
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.included_count, 0);
    assert_eq!(kpis.total_citations, 0);
    assert_eq!(kpis.unique_authors, 0);
    assert_eq!(kpis.year_from, None);
    assert_eq!(kpis.year_to, None);
    assert_eq!(kpis.pubs_per_year, None);
    assert!(kpis.pubs_by_year.is_empty());
}

#[test]
fn kpi_basic_happy_path() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(5), "Smith J; Doe A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2021), Some(10), "Smith J", None, None);
    insert_kpi_article(&conn, "a3", "included", Some(2022), Some(15), "Lee K", None, None);

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.included_count, 3);
    assert_eq!(kpis.total_citations, 30);
    assert_eq!(kpis.year_from, Some(2020));
    assert_eq!(kpis.year_to, Some(2022));

    // pubs_by_year: [{2020,1}, {2021,1}, {2022,1}]
    assert_eq!(kpis.pubs_by_year.len(), 3);
    assert_eq!(kpis.pubs_by_year[0], YearCount { year: 2020, count: 1 });
    assert_eq!(kpis.pubs_by_year[2], YearCount { year: 2022, count: 1 });

    // pubs_per_year = 3 articles / 3 years = 1.0
    assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);

    // avg_growth_rate: 0% (2020→2021) and 0% (2021→2022) = avg 0%
    assert!((kpis.avg_growth_rate.unwrap() - 0.0).abs() < 0.01);
}

#[test]
fn kpi_year_null_value() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", None, Some(3), "Smith J", None, None);
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.year_from, None);
    assert_eq!(kpis.year_to, None);
    assert_eq!(kpis.pubs_per_year, None);
    assert!(kpis.pubs_by_year.is_empty());
    assert_eq!(kpis.included_count, 1);
}

#[test]
fn kpi_year_null_filtered() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "A", None, None);
    insert_kpi_article(&conn, "a2", "included", None, Some(1), "B", None, None); // NULL year
    insert_kpi_article(&conn, "a4", "included", Some(2022), Some(1), "D", None, None);

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.included_count, 3);
    assert_eq!(kpis.year_from, Some(2020));
    assert_eq!(kpis.year_to, Some(2022));

    // pubs_by_year only has 2020 and 2022 (NULL excluded)
    assert_eq!(kpis.pubs_by_year.len(), 2);
    assert_eq!(kpis.pubs_by_year[0], YearCount { year: 2020, count: 1 });
    assert_eq!(kpis.pubs_by_year[1], YearCount { year: 2022, count: 1 });

    // pubs_per_year = 2 articles with year / 2 distinct years = 1.0
    assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);
}

#[test]
fn kpi_pubs_per_year_precision() {
    let conn = test_db();
    // 3 articles across 3 years → pubs_per_year = 1.0
    insert_kpi_article(&conn, "a1", "included", Some(2018), Some(1), "A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "B", None, None);
    insert_kpi_article(&conn, "a3", "included", Some(2022), Some(1), "C", None, None);
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);

    // With 5 articles across 2 years → pubs_per_year = 2.5
    insert_kpi_article(&conn, "a4", "included", Some(2018), Some(1), "D", None, None);
    insert_kpi_article(&conn, "a5", "included", Some(2020), Some(1), "E", None, None);
    let kpis2 = get_biblio_kpis(&conn).unwrap();
    assert!((kpis2.pubs_per_year.unwrap() - (5.0 / 3.0)).abs() < 0.01);
}

#[test]
fn kpi_citations_with_nulls() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(10), "A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B", None, None); // NULL citations
    insert_kpi_article(&conn, "a3", "included", Some(2020), Some(5), "C", None, None);

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.total_citations, 15); // 10 + 5, NULL excluded
}

#[test]
fn kpi_avg_growth_rate_positive() {
    let conn = test_db();
    // 5 articles in 2021, 10 in 2022 → one pair: +100%
    for i in 0..5 {
        insert_kpi_article(
            &conn,
            &format!("old{i}"),
            "included",
            Some(2021),
            Some(1),
            "A",
            None,
            None,
        );
    }
    for i in 0..10 {
        insert_kpi_article(
            &conn,
            &format!("new{i}"),
            "included",
            Some(2022),
            Some(1),
            "B",
            None,
            None,
        );
    }

    let kpis = get_biblio_kpis(&conn).unwrap();
    let rate = kpis.avg_growth_rate.unwrap();
    assert!((rate - 100.0).abs() < 0.1, "expected +100%, got {rate}");
}

#[test]
fn kpi_avg_growth_rate_negative() {
    let conn = test_db();
    // 10 articles in 2021, 5 in 2022 → one pair: -50%
    for i in 0..10 {
        insert_kpi_article(
            &conn,
            &format!("old{i}"),
            "included",
            Some(2021),
            Some(1),
            "A",
            None,
            None,
        );
    }
    for i in 0..5 {
        insert_kpi_article(
            &conn,
            &format!("new{i}"),
            "included",
            Some(2022),
            Some(1),
            "B",
            None,
            None,
        );
    }

    let kpis = get_biblio_kpis(&conn).unwrap();
    let rate = kpis.avg_growth_rate.unwrap();
    assert!((rate - (-50.0)).abs() < 0.1, "expected -50%, got {rate}");
}

#[test]
fn kpi_avg_growth_rate_multi_year() {
    let conn = test_db();
    // 4 in 2019, 8 in 2020, 4 in 2021, 12 in 2022
    for i in 0..4 {
        insert_kpi_article(
            &conn,
            &format!("a19_{i}"),
            "included",
            Some(2019),
            Some(1),
            "A",
            None,
            None,
        );
    }
    for i in 0..8 {
        insert_kpi_article(
            &conn,
            &format!("a20_{i}"),
            "included",
            Some(2020),
            Some(1),
            "B",
            None,
            None,
        );
    }
    for i in 0..4 {
        insert_kpi_article(
            &conn,
            &format!("a21_{i}"),
            "included",
            Some(2021),
            Some(1),
            "C",
            None,
            None,
        );
    }
    for i in 0..12 {
        insert_kpi_article(
            &conn,
            &format!("a22_{i}"),
            "included",
            Some(2022),
            Some(1),
            "D",
            None,
            None,
        );
    }

    let kpis = get_biblio_kpis(&conn).unwrap();
    // Growth rates: 2019→2020 = +100%, 2020→2021 = -50%, 2021→2022 = +200%
    // Avg = (100 + (-50) + 200) / 3 = 250 / 3 ≈ 83.33
    let rate = kpis.avg_growth_rate.unwrap();
    let expected = (100.0 + (-50.0) + 200.0) / 3.0;
    assert!((rate - expected).abs() < 0.1, "expected {expected}%, got {rate}");

    // pubs_per_year = 28 / 4 = 7.0
    assert!((kpis.pubs_per_year.unwrap() - 7.0).abs() < 0.01);
}

#[test]
fn kpi_avg_growth_rate_single_year_is_none() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2022), Some(1), "A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2022), Some(1), "B", None, None);

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.avg_growth_rate, None);
}

#[test]
fn kpi_unique_authors_zero_without_normalization() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J; Doe A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "Smith J", None, None);

    // Without normalization, biblio_authors is empty → unique_authors = 0
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.unique_authors, 0);
}

#[test]
fn kpi_unique_authors_from_biblio_table() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J", None, None);
    upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    upsert_author(&conn, "doe a", "Doe, A.").unwrap();

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.unique_authors, 2);
}

// ── Selective Clear Tests ───────────────────────────────────

#[test]
fn test_clear_regeneratable_preserves_ai_terms() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    // Insert metadata term and AI term
    save_article_terms(
        &conn,
        "art1",
        &[
            ("keyword".to_string(), TermType::Keyword, TermSource::Metadata),
            ("ai concept".to_string(), TermType::NounPhrase, TermSource::AiExtracted),
        ],
    )
    .unwrap();

    let before = get_biblio_status(&conn).unwrap();
    assert_eq!(before.term_count, 2);

    clear_regeneratable_biblio(&conn).unwrap();

    let after = get_biblio_status(&conn).unwrap();
    assert_eq!(after.term_count, 1, "AI term should be preserved");
    assert_eq!(after.author_count, 0, "Authors should be cleared");
}

#[test]
fn test_clear_regeneratable_preserves_user_terms() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    save_article_terms(
        &conn,
        "art1",
        &[
            ("user tag".to_string(), TermType::Keyword, TermSource::UserAdded),
            ("metadata kw".to_string(), TermType::Keyword, TermSource::Metadata),
        ],
    )
    .unwrap();

    clear_regeneratable_biblio(&conn).unwrap();

    let terms = get_all_terms(&conn).unwrap();
    assert_eq!(terms.len(), 1);
    assert_eq!(terms[0].source, TermSource::UserAdded);
}

// ── Author Metrics Tests ────────────────────────────────────

#[test]
fn test_compute_author_metrics() {
    let conn = test_db();
    // Insert articles with citation counts
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a1', 'T1', 'Abs', 'Smith J', 'included', 2020, 10)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a2', 'T2', 'Abs', 'Smith J', 'included', 2022, 5)",
        [],
    ).unwrap();

    // Create author and links
    let aid = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    link_article_author(&conn, "a1", &aid, 0, Some("Smith J"), None).unwrap();
    link_article_author(&conn, "a2", &aid, 0, Some("Smith J"), None).unwrap();

    compute_author_metrics(&conn).unwrap();

    let author = get_all_authors(&conn).unwrap().into_iter().next().unwrap();
    assert_eq!(author.total_citations, 15, "Total citations should be 10+5=15");
    assert!(author.avg_year.unwrap() > 2020.0, "Avg year should be ~2021");
    assert_eq!(author.estimated_h_index, Some(2), "h-index: 2 papers with >=2 citations");
}

#[test]
fn test_compute_h_index() {
    let conn = test_db();
    // 5 papers with citations [10, 8, 5, 4, 3] → h-index = 4 (4 papers with >=4 citations)
    for i in 0..5 {
        let cites = [10, 8, 5, 4, 3][i];
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, num_cited) VALUES (?1, 'T', 'Abs', 'A', 'included', ?2)",
            rusqlite::params![format!("a{i}"), cites],
        ).unwrap();
    }
    let aid = upsert_author(&conn, "a", "A.").unwrap();
    for i in 0..5 {
        link_article_author(&conn, &format!("a{i}"), &aid, i as i32, None, None).unwrap();
    }

    let h = compute_h_index(&conn, &aid).unwrap();
    assert_eq!(h, 4, "h-index should be 4 for [10,8,5,4,3]");
}

// ── Journal Distribution Tests (timeline) ────────────────────

/// Seed a journal_index row and return its id.
fn insert_journal_index_row(conn: &Connection, id: &str, title: &str) {
    conn.execute(
        "INSERT INTO journal_index (id, journal_title) VALUES (?1, ?2)",
        rusqlite::params![id, title],
    )
    .unwrap();
}

#[test]
fn journal_distribution_uses_canonical_title() {
    let conn = test_db();
    insert_journal_index_row(&conn, "j1", "Nature");

    // Three articles with different raw journal casing, all linked to the same journal_index row.
    insert_kpi_article(&conn, "a1", "included", Some(2020), None, "A", Some("Nature"), Some("j1"));
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B", Some("nature"), Some("j1"));
    insert_kpi_article(&conn, "a3", "included", Some(2020), None, "C", Some("NATURE "), Some("j1"));

    let kpis = get_biblio_kpis(&conn).unwrap();
    let dist = &kpis.journal_distribution;

    // All three collapse to a single canonical "Nature" bucket for 2020.
    let nature_rows: Vec<&JournalYearData> =
        dist.iter().filter(|d| d.journal == "Nature" && d.year == 2020).collect();
    assert_eq!(nature_rows.len(), 1, "raw variants should collapse to one canonical bucket");
    assert_eq!(nature_rows[0].count, 3);
    assert_eq!(nature_rows[0].journal_index_id.as_deref(), Some("j1"));
}

#[test]
fn journal_distribution_falls_back_to_normalized_raw() {
    let conn = test_db();
    // Articles with NO journal_index_id — varied casing should normalize via UPPER(TRIM).
    insert_kpi_article(&conn, "a1", "included", Some(2020), None, "A", Some("Science"), None);
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B", Some("science "), None);
    insert_kpi_article(&conn, "a3", "included", Some(2020), None, "C", Some("SCIENCE"), None);

    let dist = get_journal_year_data(&conn).unwrap();
    // Normalized key is UPPER(TRIM) → "SCIENCE"
    let science_rows: Vec<&JournalYearData> =
        dist.iter().filter(|d| d.journal == "SCIENCE" && d.year == 2020).collect();
    assert_eq!(science_rows.len(), 1, "raw casing variants should normalize to one bucket");
    assert_eq!(science_rows[0].count, 3);
    assert!(science_rows[0].journal_index_id.is_none());
}

#[test]
fn journal_distribution_journal_index_id_flag() {
    let conn = test_db();
    insert_journal_index_row(&conn, "j1", "Nature");
    insert_kpi_article(&conn, "a1", "included", Some(2020), None, "A", Some("Nature"), Some("j1"));
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B", Some("Cell"), None);

    let dist = get_journal_year_data(&conn).unwrap();
    for row in &dist {
        if row.journal == "Nature" {
            assert_eq!(row.journal_index_id.as_deref(), Some("j1"));
        } else if row.journal == "CELL" {
            assert!(row.journal_index_id.is_none());
        } else {
            panic!("unexpected journal key: {}", row.journal);
        }
    }
}

#[test]
fn journal_distribution_null_and_empty_coalesce() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), None, "A", None, None);
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B", Some(""), None);

    let dist = get_journal_year_data(&conn).unwrap();
    let blank_rows: Vec<&JournalYearData> =
        dist.iter().filter(|d| d.journal.is_empty() && d.year == 2020).collect();
    assert_eq!(blank_rows.len(), 1, "NULL and empty journal should coalesce into one '' bucket");
    assert_eq!(blank_rows[0].count, 2);
}

#[test]
fn journal_distribution_ordering() {
    let conn = test_db();
    insert_journal_index_row(&conn, "j1", "Nature");
    insert_kpi_article(&conn, "a1", "included", Some(2018), None, "A", Some("Nature"), Some("j1"));
    insert_kpi_article(&conn, "a2", "included", Some(2018), None, "B", Some("Cell"), None);
    insert_kpi_article(&conn, "a3", "included", Some(2020), None, "C", Some("Nature"), Some("j1"));

    let dist = get_journal_year_data(&conn).unwrap();
    // Rows ordered by publication_year ASC.
    assert_eq!(dist[0].year, 2018);
    assert!(dist[dist.len() - 1].year >= 2018);
}

#[test]
fn journal_distribution_regression_other_kpis_unchanged() {
    let conn = test_db();
    insert_journal_index_row(&conn, "j1", "Nature");
    insert_kpi_article(
        &conn,
        "a1",
        "included",
        Some(2020),
        Some(5),
        "A",
        Some("Nature"),
        Some("j1"),
    );
    insert_kpi_article(&conn, "a2", "included", Some(2021), Some(10), "B", Some("Cell"), None);

    let kpis = get_biblio_kpis(&conn).unwrap();
    // pubs_by_year / refs_by_year / citations_by_year remain unaffected by the new field.
    assert_eq!(kpis.pubs_by_year.len(), 2);
    assert_eq!(kpis.pubs_by_year[0], YearCount { year: 2020, count: 1 });
    assert_eq!(kpis.included_count, 2);
    assert_eq!(kpis.total_citations, 15);
    // journal_distribution populated.
    assert_eq!(kpis.journal_distribution.len(), 2);
}

// ── Journal Info Tests (timeline info card) ──────────────────

#[test]
fn get_journal_info_unknown_returns_none() {
    let conn = test_db();
    let result = bango_lib::db::journal_repo::get_journal_info(&conn, "does-not-exist").unwrap();
    assert!(result.is_none(), "unknown journal id should return Ok(None)");
}

#[test]
fn get_journal_info_returns_metadata_and_aggregates() {
    let conn = test_db();
    // Seed a journal_index row with rich metadata.
    conn.execute(
        "INSERT INTO journal_index (id, journal_title, issn, eissn, publisher_name, publisher_address, languages, web_of_science_categories)
         VALUES ('j1', 'Nature', '0028-0836', '1476-4687', 'Springer Nature', 'London', 'English', 'Multidisciplinary')",
        [],
    )
    .unwrap();

    // Two included articles linked to the journal (different years, different citation counts).
    insert_kpi_article(
        &conn,
        "a1",
        "included",
        Some(2019),
        Some(40),
        "A",
        Some("Nature"),
        Some("j1"),
    );
    insert_kpi_article(
        &conn,
        "a2",
        "included",
        Some(2021),
        Some(60),
        "B",
        Some("Nature"),
        Some("j1"),
    );
    // A rejected article linked to the same journal — must NOT be counted.
    insert_kpi_article(
        &conn,
        "a3",
        "rejected",
        Some(2020),
        Some(100),
        "C",
        Some("Nature"),
        Some("j1"),
    );

    let info = bango_lib::db::journal_repo::get_journal_info(&conn, "j1")
        .unwrap()
        .expect("known journal id should return Some");

    assert_eq!(info.id, "j1");
    assert_eq!(info.journal_title, "Nature");
    assert_eq!(info.issn.as_deref(), Some("0028-0836"));
    assert_eq!(info.eissn.as_deref(), Some("1476-4687"));
    assert_eq!(info.publisher_name.as_deref(), Some("Springer Nature"));
    assert_eq!(info.languages.as_deref(), Some("English"));
    assert_eq!(info.web_of_science_categories.as_deref(), Some("Multidisciplinary"));
    // Aggregates over included articles only.
    assert_eq!(info.article_count, 2);
    assert_eq!(info.first_year, Some(2019));
    assert_eq!(info.last_year, Some(2021));
    assert_eq!(info.citations_total, 100); // 40 + 60 (rejected excluded)
    assert_eq!(info.pubs_by_year.len(), 2);
    assert_eq!(info.pubs_by_year[0], YearCount { year: 2019, count: 1 });
    assert_eq!(info.pubs_by_year[1], YearCount { year: 2021, count: 1 });
}

// ── Author Productivity tests ───────────────────────────────

/// Helper: seed an article with authors + citation count, then
/// link the authors to it. Returns the created author IDs in order.
fn seed_productivity_article(
    conn: &Connection,
    article_id: &str,
    pub_year: Option<i32>,
    num_cited: Option<i32>,
    authors: &[(&str, &str)], // (normalized, display)
) -> Vec<String> {
    seed_productivity_article_with_status(
        conn, article_id, "included", pub_year, num_cited, authors,
    )
}

/// Helper: seed an article with a custom status (working/included/rejected/duplicate).
fn seed_productivity_article_with_status(
    conn: &Connection,
    article_id: &str,
    status: &str,
    pub_year: Option<i32>,
    num_cited: Option<i32>,
    authors: &[(&str, &str)], // (normalized, display)
) -> Vec<String> {
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) \
         VALUES (?1, 'T', 'Abs', ?2, ?3, ?4, ?5)",
        rusqlite::params![
            article_id,
            authors.iter().map(|(_, d)| *d).collect::<Vec<_>>().join("; "),
            status,
            pub_year,
            num_cited
        ],
    )
    .unwrap();

    let mut ids = Vec::new();
    for (order, (norm, display)) in authors.iter().enumerate() {
        let id = upsert_author(conn, norm, display).unwrap();
        link_article_author(conn, article_id, &id, order as i32, Some(display), None).unwrap();
        ids.push(id);
    }
    ids
}

#[test]
fn productivity_rankings_empty_db() {
    let conn = test_db();
    let rankings = get_author_rankings(&conn).unwrap();
    assert!(rankings.is_empty(), "no articles → no rankings");
}

#[test]
fn productivity_rankings_basic_metrics() {
    let conn = test_db();
    // 3 articles, 2 authors each
    seed_productivity_article(
        &conn,
        "a1",
        Some(2020),
        Some(10),
        &[("smith j", "Smith, J."), ("doe a", "Doe, A.")],
    );
    seed_productivity_article(
        &conn,
        "a2",
        Some(2021),
        Some(20),
        &[("smith j", "Smith, J."), ("doe a", "Doe, A.")],
    );
    seed_productivity_article(
        &conn,
        "a3",
        Some(2022),
        Some(30),
        &[("smith j", "Smith, J."), ("lee k", "Lee, K.")],
    );

    compute_author_metrics(&conn).unwrap();
    let rankings = get_author_rankings(&conn).unwrap();
    assert_eq!(rankings.len(), 3, "3 distinct authors");

    // Smith has 3 articles (first author each), 60 total citations
    let smith = rankings.iter().find(|r| r.display_name == "Smith, J.").unwrap();
    assert_eq!(smith.article_count, 3);
    assert_eq!(smith.first_author_count, 3);
    assert_eq!(smith.total_citations, 60);
    assert_eq!(smith.last_author_count, 0, "Smith is always order 0, never last");

    // Lee has 1 article, 30 citations
    let lee = rankings.iter().find(|r| r.display_name == "Lee, K.").unwrap();
    assert_eq!(lee.article_count, 1);
    assert_eq!(lee.total_citations, 30);
    assert_eq!(lee.last_author_count, 1, "Lee is order 1 (last) in a3");
}

#[test]
fn productivity_rankings_i10_index() {
    let conn = test_db();
    // Papers cited [15, 12, 8, 5] → i10 = 2 (only 15 and 12 ≥ 10)
    seed_productivity_article(&conn, "a1", Some(2020), Some(15), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a2", Some(2020), Some(12), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a3", Some(2020), Some(8), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a4", Some(2020), Some(5), &[("smith j", "Smith")]);

    compute_author_metrics(&conn).unwrap();
    let rankings = get_author_rankings(&conn).unwrap();
    let smith = &rankings[0];
    assert_eq!(smith.i10_index, 2, "2 papers have ≥ 10 citations");
}

#[test]
fn productivity_rankings_g_index() {
    let conn = test_db();
    // Papers cited [10, 9, 8, 7] → sorted desc: [10, 9, 8, 7], cumulative: [10, 19, 27, 34]
    // n=1: 10 ≥ 1 ✓; n=2: 19 ≥ 4 ✓; n=3: 27 ≥ 9 ✓; n=4: 34 ≥ 16 ✓ → g=4
    seed_productivity_article(&conn, "a1", Some(2020), Some(7), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a2", Some(2020), Some(8), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a3", Some(2020), Some(9), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a4", Some(2020), Some(10), &[("smith j", "Smith")]);

    compute_author_metrics(&conn).unwrap();
    let rankings = get_author_rankings(&conn).unwrap();
    let smith = &rankings[0];
    assert_eq!(smith.g_index, 4, "g=4 for [10,9,8,7] (34 cumulative ≥ 16)");
}

#[test]
fn productivity_rankings_g_index_caps_at_citation_deficit() {
    let conn = test_db();
    // Papers cited [3, 2, 1] → sorted desc: [3, 2, 1], cumulative: [3, 5, 6]
    // n=1: 3 ≥ 1 ✓; n=2: 5 ≥ 4 ✓; n=3: 6 < 9 ✗ → g=2
    seed_productivity_article(&conn, "a1", Some(2020), Some(1), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a2", Some(2020), Some(2), &[("smith j", "Smith")]);
    seed_productivity_article(&conn, "a3", Some(2020), Some(3), &[("smith j", "Smith")]);

    compute_author_metrics(&conn).unwrap();
    let rankings = get_author_rankings(&conn).unwrap();
    let smith = &rankings[0];
    assert_eq!(smith.g_index, 2, "g=2 for [3,2,1] (6 cumulative < 9)");
}

#[test]
fn productivity_rankings_scope_excludes_duplicates_only() {
    let conn = test_db();
    // One included, one working, one rejected, one duplicate.
    // Rankings should include working + included + rejected authors,
    // but NOT the duplicate's author.
    seed_productivity_article(&conn, "inc1", Some(2020), Some(5), &[("a", "A")]);
    seed_productivity_article(&conn, "wk1", Some(2021), Some(3), &[("b", "B")]);
    seed_productivity_article(&conn, "rej1", Some(2022), Some(1), &[("c", "C")]);
    // Duplicate article — its author must NOT appear
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) VALUES ('dup1', 'T', 'Abs', 'D', 'duplicate')",
        [],
    )
    .unwrap();
    let author_d = upsert_author(&conn, "d", "D").unwrap();
    link_article_author(&conn, "dup1", &author_d, 0, Some("D"), None).unwrap();

    compute_author_metrics(&conn).unwrap();
    let rankings = get_author_rankings(&conn).unwrap();
    assert_eq!(
        rankings.len(),
        3,
        "working + included + rejected authors appear, duplicate excluded"
    );
    let names: Vec<&str> = rankings.iter().map(|r| r.display_name.as_str()).collect();
    assert!(names.contains(&"A"), "included author present");
    assert!(names.contains(&"B"), "working author present");
    assert!(names.contains(&"C"), "rejected author present");
    assert!(!names.contains(&"D"), "duplicate author excluded");
}

#[test]
fn productivity_kpis_basic() {
    let conn = test_db();
    seed_productivity_article(&conn, "a1", Some(2020), Some(10), &[("a", "A")]);
    seed_productivity_article(&conn, "a2", Some(2022), Some(20), &[("a", "A"), ("b", "B")]);
    compute_author_metrics(&conn).unwrap();
    build_coauthor_edges(&conn).unwrap();

    let kpis = get_author_productivity_kpis(&conn).unwrap();
    assert_eq!(kpis.total_authors, 2);
    assert!(kpis.max_h_index >= 1);
    assert_eq!(kpis.year_from, Some(2020));
    assert_eq!(kpis.year_to, Some(2022));
    assert!(kpis.total_collaborations >= 1, "A and B co-authored a2");
}

#[test]
fn productivity_detail_lazy_load() {
    let conn = test_db();
    let author_ids = seed_productivity_article(
        &conn,
        "a1",
        Some(2020),
        Some(10),
        &[("smith j", "Smith, J."), ("doe a", "Doe, A.")],
    );
    compute_author_metrics(&conn).unwrap();
    build_coauthor_edges(&conn).unwrap();

    let smith_id = &author_ids[0];
    let detail = get_author_detail(&conn, smith_id).unwrap();

    assert_eq!(detail.rank.display_name, "Smith, J.");
    assert_eq!(detail.pubs_by_year.len(), 1);
    assert!(!detail.recent_papers.is_empty(), "should have ≥ 1 recent paper");
    assert_eq!(detail.recent_papers[0].title, "T");
    // Collaborators: Doe A with 1 shared paper
    assert_eq!(detail.top_collaborators.len(), 1);
    assert_eq!(detail.top_collaborators[0].collaborator_name, "Doe, A.");
}

// Network builder & serializer unit tests (including all Co-Citation tests)
// now live in `biblio_networks_test.rs`. End-to-end citation-pipeline tests
// live in `biblio_integration_test.rs`; RIS-fixture co-citation tests live in
// `cocitation_data_test.rs`.
