use rusqlite::Connection;

use bango_lib::db::biblio_repo::{
    build_coauthor_edges, clear_all_biblio, clear_regeneratable_biblio, compute_author_metrics,
    compute_h_index, delete_network, get_all_authors, get_all_terms, get_authors_for_article,
    get_biblio_kpis, get_biblio_status, get_coauthor_network_json, get_terms_for_article,
    link_article_author, link_article_term, load_network, load_network_edges, load_network_nodes,
    save_article_terms, save_network, upsert_author, upsert_institution, upsert_term,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::models::biblio::{
    BiblioNetworkEdge, BiblioNetworkNode, NetworkType, TermSource, TermType, YearCount,
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
/// `year` is an Option<i32> matching the INTEGER publication_year column.
fn insert_kpi_article(
    conn: &Connection,
    id: &str,
    status: &str,
    pub_year: Option<i32>,
    num_cited: Option<i32>,
    authors: &str,
) {
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status, publication_year, num_cited)
         VALUES (?1, 'Test', 'Abstract', ?2, ?3, ?4, ?5)",
        rusqlite::params![id, authors, status, pub_year, num_cited],
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
    insert_kpi_article(&conn, "a1", "rejected", Some(2020), Some(5), "Smith J");
    insert_kpi_article(&conn, "a2", "duplicate", Some(2021), Some(10), "Doe A");
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
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(5), "Smith J; Doe A");
    insert_kpi_article(&conn, "a2", "included", Some(2021), Some(10), "Smith J");
    insert_kpi_article(&conn, "a3", "included", Some(2022), Some(15), "Lee K");

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
    insert_kpi_article(&conn, "a1", "included", None, Some(3), "Smith J");
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
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "A");
    insert_kpi_article(&conn, "a2", "included", None, Some(1), "B"); // NULL year
    insert_kpi_article(&conn, "a4", "included", Some(2022), Some(1), "D");

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
    insert_kpi_article(&conn, "a1", "included", Some(2018), Some(1), "A");
    insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "B");
    insert_kpi_article(&conn, "a3", "included", Some(2022), Some(1), "C");
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert!((kpis.pubs_per_year.unwrap() - 1.0).abs() < 0.01);

    // With 5 articles across 2 years → pubs_per_year = 2.5
    insert_kpi_article(&conn, "a4", "included", Some(2018), Some(1), "D");
    insert_kpi_article(&conn, "a5", "included", Some(2020), Some(1), "E");
    let kpis2 = get_biblio_kpis(&conn).unwrap();
    assert!((kpis2.pubs_per_year.unwrap() - (5.0 / 3.0)).abs() < 0.01);
}

#[test]
fn kpi_citations_with_nulls() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(10), "A");
    insert_kpi_article(&conn, "a2", "included", Some(2020), None, "B"); // NULL citations
    insert_kpi_article(&conn, "a3", "included", Some(2020), Some(5), "C");

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.total_citations, 15); // 10 + 5, NULL excluded
}

#[test]
fn kpi_avg_growth_rate_positive() {
    let conn = test_db();
    // 5 articles in 2021, 10 in 2022 → one pair: +100%
    for i in 0..5 {
        insert_kpi_article(&conn, &format!("old{i}"), "included", Some(2021), Some(1), "A");
    }
    for i in 0..10 {
        insert_kpi_article(&conn, &format!("new{i}"), "included", Some(2022), Some(1), "B");
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
        insert_kpi_article(&conn, &format!("old{i}"), "included", Some(2021), Some(1), "A");
    }
    for i in 0..5 {
        insert_kpi_article(&conn, &format!("new{i}"), "included", Some(2022), Some(1), "B");
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
        insert_kpi_article(&conn, &format!("a19_{i}"), "included", Some(2019), Some(1), "A");
    }
    for i in 0..8 {
        insert_kpi_article(&conn, &format!("a20_{i}"), "included", Some(2020), Some(1), "B");
    }
    for i in 0..4 {
        insert_kpi_article(&conn, &format!("a21_{i}"), "included", Some(2021), Some(1), "C");
    }
    for i in 0..12 {
        insert_kpi_article(&conn, &format!("a22_{i}"), "included", Some(2022), Some(1), "D");
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
    insert_kpi_article(&conn, "a1", "included", Some(2022), Some(1), "A");
    insert_kpi_article(&conn, "a2", "included", Some(2022), Some(1), "B");

    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.avg_growth_rate, None);
}

#[test]
fn kpi_unique_authors_zero_without_normalization() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J; Doe A");
    insert_kpi_article(&conn, "a2", "included", Some(2020), Some(1), "Smith J");

    // Without normalization, biblio_authors is empty → unique_authors = 0
    let kpis = get_biblio_kpis(&conn).unwrap();
    assert_eq!(kpis.unique_authors, 0);
}

#[test]
fn kpi_unique_authors_from_biblio_table() {
    let conn = test_db();
    insert_kpi_article(&conn, "a1", "included", Some(2020), Some(1), "Smith J");
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
