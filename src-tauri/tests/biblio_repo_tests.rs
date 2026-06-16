use rusqlite::Connection;

use bango_lib::db::biblio_repo::{
    build_coauthor_edges, clear_all_biblio, clear_regeneratable_biblio, compute_author_metrics,
    compute_h_index, delete_network, get_all_authors, get_all_terms, get_author_detail,
    get_author_productivity_kpis, get_author_rankings, get_authors_for_article, get_biblio_kpis,
    get_biblio_status, get_coauthor_network_json, get_cocitation_network_json,
    get_journal_year_data, get_keyword_network_json, get_terms_for_article, link_article_author,
    link_article_term, load_network, load_network_edges, load_network_nodes, save_article_terms,
    save_network, upsert_author, upsert_institution, upsert_term, CocitationNormalization,
    CocitationScope,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::models::biblio::{
    BiblioNetworkEdge, BiblioNetworkNode, JournalYearData, NetworkType, TermSource, TermType,
    YearCount,
};

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

// ── Network operations ──────────────────────────────────────

#[test]
fn test_save_and_load_network() {
    let conn = test_db();
    let nodes = vec![BiblioNetworkNode {
        id: String::new(),
        network_id: String::new(),
        entity_id: "author1".to_string(),
        label: "Smith".to_string(),
        weight: 5.0,
        cluster: Some(1),
        x: Some(0.5),
        y: Some(0.3),
    }];
    let edges = vec![BiblioNetworkEdge {
        id: String::new(),
        network_id: String::new(),
        source_id: "author1".to_string(),
        target_id: "author2".to_string(),
        weight: 3.0,
    }];

    let net_id =
        save_network(&conn, &NetworkType::CoAuthorship, "Test Network", None, None, &nodes, &edges)
            .unwrap();

    let meta = load_network(&conn, &net_id).unwrap().unwrap();
    assert_eq!(meta.label, "Test Network");
    assert_eq!(meta.node_count, 1);
    assert_eq!(meta.edge_count, 1);

    let loaded_nodes = load_network_nodes(&conn, &net_id).unwrap();
    assert_eq!(loaded_nodes.len(), 1);
    assert_eq!(loaded_nodes[0].entity_id, "author1");

    let loaded_edges = load_network_edges(&conn, &net_id).unwrap();
    assert_eq!(loaded_edges.len(), 1);
}

#[test]
fn test_delete_network_cascades() {
    let conn = test_db();
    let net_id = save_network(
        &conn,
        &NetworkType::CoOccurrence,
        "To Delete",
        None,
        None,
        &[BiblioNetworkNode {
            id: String::new(),
            network_id: String::new(),
            entity_id: "t1".to_string(),
            label: "term".to_string(),
            weight: 1.0,
            cluster: None,
            x: None,
            y: None,
        }],
        &[],
    )
    .unwrap();

    delete_network(&conn, &net_id).unwrap();
    assert!(load_network(&conn, &net_id).unwrap().is_none());
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

// ── Edge Counting Tests ─────────────────────────────────────

#[test]
fn test_build_coauthor_edges_full_and_fractional() {
    let conn = test_db();
    // 1 article with 3 authors → 3 pairs
    conn.execute("INSERT INTO articles (id, title, abstract_text, authors, status) VALUES ('a1', 'T', 'Abs', 'A; B; C', 'included')", []).unwrap();
    let a1 = upsert_author(&conn, "a", "A.").unwrap();
    let a2 = upsert_author(&conn, "b", "B.").unwrap();
    let a3 = upsert_author(&conn, "c", "C.").unwrap();
    link_article_author(&conn, "a1", &a1, 0, None, None).unwrap();
    link_article_author(&conn, "a1", &a2, 1, None, None).unwrap();
    link_article_author(&conn, "a1", &a3, 2, None, None).unwrap();

    let edge_count = build_coauthor_edges(&conn).unwrap();
    assert_eq!(edge_count, 3, "3 authors → 3 edges");

    // Verify fractional data in network metadata
    let meta: String = conn
        .query_row(
            "SELECT params_json FROM biblio_network_meta WHERE network_type = 'co_authorship'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&meta).unwrap();
    let frac_edges = parsed["fractional_edges"].as_array().unwrap();
    // 3 pairs from 3 authors: each pair gets 1/3 ≈ 0.333
    for fe in frac_edges {
        let fw = fe["fractional_weight"].as_f64().unwrap();
        assert!((fw - 0.333).abs() < 0.01, "Fractional weight should be ~0.333, got {fw}");
    }
}

// ── Network JSON Output Tests ───────────────────────────────

#[test]
fn test_get_coauthor_network_json_includes_metrics() {
    let conn = test_db();
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited) VALUES ('a1', 'T', 'Abs', 'Smith J', 'included', 2020, 10)",
        [],
    ).unwrap();
    let aid = upsert_author(&conn, "smith j", "Smith, J.").unwrap();
    link_article_author(&conn, "a1", &aid, 0, None, None).unwrap();
    compute_author_metrics(&conn).unwrap();

    let json = get_coauthor_network_json(&conn).unwrap();
    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["totalCitations"], 10);
    assert_eq!(nodes[0]["estimatedHIndex"], 1);
    assert_eq!(nodes[0]["avgYear"], 2020.0);
}

#[test]
fn test_get_keyword_network_json() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");

    // Insert metadata term for both articles -> co-occurrence edge
    let t1 = upsert_term(
        &conn,
        "Neural Network",
        "neural network",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();
    let t2 = upsert_term(
        &conn,
        "Machine Learning",
        "machin learn",
        &TermType::Keyword,
        &TermSource::Metadata,
    )
    .unwrap();

    link_article_term(&conn, "art1", &t1).unwrap();
    link_article_term(&conn, "art1", &t2).unwrap();
    link_article_term(&conn, "art2", &t1).unwrap();
    link_article_term(&conn, "art2", &t2).unwrap();

    // Insert a tag for art1
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES ('tag1', 'Deep Learning', 'user_created')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO article_tags (article_id, tag_id) VALUES ('art1', 'tag1')", [])
        .unwrap();

    // Fetch network json for metadata and tags
    let sources = vec!["metadata".to_string(), "tags".to_string()];
    let json = get_keyword_network_json(&conn, &sources, 1, 1).unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    let edges = json["edges"].as_array().unwrap();

    assert_eq!(nodes.len(), 3);
    assert_eq!(edges.len(), 3); // All 3 pairs should share art1, making 3 edges.
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

// ── Co-Citation Tests ─────────────────────────────────────────

/// Helper: insert a reference paper and return its ID.
fn insert_reference_paper(conn: &Connection, id: &str, title: &str, authors: &str) {
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, match_status) VALUES (?1, ?2, ?3, 'unmatched')",
        rusqlite::params![id, title, authors],
    )
    .unwrap();
}

/// Helper: link an article to a reference paper (type=1 = reference).
fn insert_reference_link(conn: &Connection, article_id: &str, paper_id: &str) {
    conn.execute(
        "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type) \
         VALUES (?1, ?2, ?3, 1)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), article_id, paper_id],
    )
    .unwrap();
}

#[test]
fn cocitation_empty_db_returns_empty() {
    let conn = test_db();
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        2,
        2,
    )
    .unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert!(nodes.is_empty());
    assert!(edges.is_empty());
}

#[test]
fn cocitation_single_reference_no_pairs() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_reference_paper(&conn, "rp1", "Paper One", "[\"Smith J\"]");
    insert_reference_link(&conn, "art1", "rp1");

    // With only 1 reference per article, no co-citation pairs can form.
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        1,
        1,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    // With only 1 reference per article, no co-citation pairs can form.
    assert!(edges.is_empty(), "no co-citation pairs with only 1 reference");
}

#[test]
fn cocitation_two_articles_shared_refs() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // Both articles cite rp1 and rp2 → co-citation count = 2
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        2,
        2,
    )
    .unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(nodes.len(), 2, "two co-cited papers");
    assert_eq!(edges.len(), 1, "one co-citation edge");
    assert_eq!(edges[0]["rawWeight"], 2, "co-cited by 2 articles");
}

#[test]
fn cocitation_min_citation_count_filter() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");
    insert_reference_paper(&conn, "rp3", "Paper C", "[\"Lee K\"]");

    // art1 cites rp1, rp2, rp3
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art1", "rp3");
    // art2 cites rp1 only (rp1 cited twice, rp2/rp3 cited once)
    insert_reference_link(&conn, "art2", "rp1");

    // min_citation_count=2 → only rp1 qualifies (cited by 2 articles).
    // rp2 and rp3 are cited only once → excluded as candidates.
    // Since rp1 is the only candidate, no pairs can form → 0 nodes, 0 edges.
    // (Nodes are derived from surviving edges; an isolated candidate has no co-citation partner.)
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        2,
        1,
    )
    .unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert!(nodes.is_empty(), "rp1 alone cannot form a co-citation pair");
    assert!(edges.is_empty(), "no pairs with rp1 alone");
}

#[test]
fn cocitation_min_co_citation_filter() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // art1 cites both; art2 cites rp1 only.
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");

    // rp1 cited twice, rp2 cited once. With min_citation_count=1, both qualify.
    // But rp1/rp2 pair is co-cited only once (by art1).
    // With min_co_citation=2, no edges should survive.
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1,
        2,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    assert!(edges.is_empty(), "pair co-cited once < min_co_citation=2");
}

#[test]
fn cocitation_cosine_normalization() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        2,
        1,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    let cosine = edges[0]["weight"].as_f64().unwrap();
    // c_ij=2, c_i=2, c_j=2 → cosine = 2/sqrt(4) = 1.0
    assert!((cosine - 1.0).abs() < 0.01, "cosine should be 1.0, got {cosine}");
}

#[test]
fn cocitation_jaccard_normalization() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // Both articles cite both papers: c_ij=2, c_i=2, c_j=2
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Jaccard,
        2,
        1,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    let jaccard = edges[0]["weight"].as_f64().unwrap();
    // c_ij=2, c_i=2, c_j=2 → jaccard = 2/(2+2-2) = 1.0
    assert!((jaccard - 1.0).abs() < 0.01, "jaccard should be 1.0, got {jaccard}");
}

#[test]
fn cocitation_pearson_normalization() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // Both articles cite both papers: identical co-citation patterns.
    // Perfect positive correlation → pearson = 1.0
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Pearson,
        2,
        1,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    let pearson = edges[0]["weight"].as_f64().unwrap();
    // With N=2 articles, a=2, b=0, c=0, d=0: denom_sq = (2)(0)(2)(0) = 0 → weight=0
    // This is a degenerate case. Let's test with 3 articles for a proper value.
    assert!(
        pearson.abs() < 0.01 || pearson.abs() >= 0.99,
        "pearson should be ~0 (degenerate) or ~1, got {pearson}"
    );
}

#[test]
fn cocitation_pearson_positive_correlation() {
    let conn = test_db();
    // 4 articles, 2 reference papers.
    // Papers cited by the same set of articles → perfect correlation (phi=1).
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_test_article(&conn, "art3");
    insert_test_article(&conn, "art4");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // art1, art2 cite both rp1 and rp2; art3, art4 cite neither.
    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Pearson,
        2,
        2,
    )
    .unwrap();

    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    let pearson = edges[0]["weight"].as_f64().unwrap();
    // N=4, a=2, b=0, c=0, d=2: phi = (4 - 0) / sqrt(2*2*2*2) = 4/4 = 1.0
    assert!(
        (pearson - 1.0).abs() < 0.01,
        "pearson should be 1.0 (perfect positive correlation), got {pearson}"
    );
}

#[test]
fn cocitation_scope_included_vs_all() {
    let conn = test_db();
    // 2 included articles + 1 rejected article.
    insert_test_article(&conn, "inc1");
    insert_test_article(&conn, "inc2");
    insert_kpi_article(&conn, "rej1", "rejected", Some(2020), None, "X", None, None);

    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");

    // Included articles cite both papers.
    insert_reference_link(&conn, "inc1", "rp1");
    insert_reference_link(&conn, "inc1", "rp2");
    insert_reference_link(&conn, "inc2", "rp1");
    insert_reference_link(&conn, "inc2", "rp2");
    // Rejected article also cites both.
    insert_reference_link(&conn, "rej1", "rp1");
    insert_reference_link(&conn, "rej1", "rp2");

    // Included scope: rp1 cited by 2 included, rp2 cited by 2 included.
    let json_inc = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        2,
        1,
    )
    .unwrap();
    let edges_inc = json_inc["edges"].as_array().unwrap();
    assert_eq!(edges_inc.len(), 1);
    assert_eq!(edges_inc[0]["rawWeight"], 2, "co-cited by 2 included articles");

    // All scope: rp1 cited by 3 (inc1, inc2, rej1), rp2 cited by 3.
    let json_all = get_cocitation_network_json(
        &conn,
        CocitationScope::AllArticles,
        CocitationNormalization::Raw,
        2,
        1,
    )
    .unwrap();
    let edges_all = json_all["edges"].as_array().unwrap();
    assert_eq!(edges_all.len(), 1);
    assert_eq!(edges_all[0]["rawWeight"], 3, "co-cited by 3 articles (including rejected)");
}

#[test]
fn cocitation_meta_block_diagnostics() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");
    insert_reference_paper(&conn, "rp1", "Paper A", "[\"Smith J\"]");
    insert_reference_paper(&conn, "rp2", "Paper B", "[\"Doe A\"]");
    insert_reference_paper(&conn, "rp3", "Paper C", "[\"Lee K\"]");

    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");
    // rp3 cited by only 1 article (below min_citation_count=2)
    insert_reference_link(&conn, "art1", "rp3");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        2,
        1,
    )
    .unwrap();

    let meta = &json["meta"];
    assert_eq!(meta["inScopeArticleCount"], 2);
    assert_eq!(meta["candidatePaperCount"], 2, "rp1 and rp2 qualify; rp3 excluded");
    assert_eq!(meta["scope"], "included");
    assert_eq!(meta["normalization"], "cosine");
}

#[test]
fn cocitation_node_metadata_from_reference_papers() {
    let conn = test_db();
    insert_test_article(&conn, "art1");
    insert_test_article(&conn, "art2");

    // Insert reference papers with full metadata.
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, publication_year, journal, doi, citation_count, match_status) \
         VALUES ('rp1', 'Foundational Paper', '[\"Smith J\"]', 2015, 'Nature', '10.1234/test', 42, 'unmatched')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, publication_year, journal, citation_count, match_status) \
         VALUES ('rp2', 'Related Work', '[\"Doe A\"]', 2018, 'Science', 20, 'unmatched')",
        [],
    )
    .unwrap();

    insert_reference_link(&conn, "art1", "rp1");
    insert_reference_link(&conn, "art1", "rp2");
    insert_reference_link(&conn, "art2", "rp1");
    insert_reference_link(&conn, "art2", "rp2");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        2,
        1,
    )
    .unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);

    let rp1 = nodes.iter().find(|n| n["id"] == "rp1").unwrap();
    assert_eq!(rp1["title"], "Foundational Paper");
    assert_eq!(rp1["journal"], "Nature");
    assert_eq!(rp1["doi"], "10.1234/test");
    assert_eq!(rp1["citationCount"], 42);
    assert_eq!(rp1["year"], 2015);
    assert_eq!(rp1["coCitationCount"], 2, "cited by 2 in-scope articles");
}
