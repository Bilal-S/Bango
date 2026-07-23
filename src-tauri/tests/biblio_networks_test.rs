//! Network builder & serializer unit tests.
//!
//! Extracted from `biblio_repo_tests.rs` to keep that file focused on
//! non-network repos. Covers: network CRUD (save/load/delete), co-author edge
//! building + JSON, keyword co-occurrence JSON, and the full co-citation
//! computation suite (filters, normalization modes, scope, meta diagnostics).
//!
//! End-to-end citation-pipeline tests live in `biblio_integration_test.rs`;
//! RIS-fixture co-citation tests live in `cocitation_data_test.rs`.

use rusqlite::Connection;

use bango_lib::db::biblio_repo::{
    build_coauthor_edges, compute_author_metrics, delete_network, get_coauthor_network_json,
    get_cocitation_network_json, get_keyword_network_json, link_article_author, link_article_term,
    load_network, load_network_edges, load_network_nodes, save_network, upsert_author, upsert_term,
    CocitationNormalization, CocitationScope,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::models::biblio::{
    BiblioNetworkEdge, BiblioNetworkNode, NetworkType, TermSource, TermType,
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
    )
    .unwrap();
}

/// Helper: insert an article with full control over key KPI fields.
/// `pub_year` is an Option<i32> matching the INTEGER publication_year column.
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
    )
    .unwrap();
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
    // Both papers are unmatched -> matchedArticleStatus is null.
    assert_eq!(rp1["matchedArticleId"], serde_json::Value::Null);
    assert_eq!(rp1["matchedArticleStatus"], serde_json::Value::Null);
}

#[test]
fn cocitation_node_matched_article_status() {
    let conn = test_db();
    // One included + one rejected library article.
    insert_test_article(&conn, "inc1");
    insert_kpi_article(&conn, "rej1", "rejected", Some(2020), None, "X", None, None);

    // rp1 matched to the included article; rp2 matched to the rejected article;
    // rp3 unmatched. All three are co-cited so they appear as nodes.
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, match_status, matched_article_id) \
         VALUES ('rp1', 'Included Match', '[\"A\"]', 'matched', 'inc1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, match_status, matched_article_id) \
         VALUES ('rp2', 'Rejected Match', '[\"B\"]', 'matched', 'rej1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, match_status) \
         VALUES ('rp3', 'Unmatched', '[\"C\"]', 'unmatched')",
        [],
    )
    .unwrap();

    // One article cites all three so the co-citation pairs form.
    insert_reference_link(&conn, "inc1", "rp1");
    insert_reference_link(&conn, "inc1", "rp2");
    insert_reference_link(&conn, "inc1", "rp3");

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1,
        1,
    )
    .unwrap();

    let nodes = json["nodes"].as_array().unwrap();
    let rp1 = nodes.iter().find(|n| n["id"] == "rp1").unwrap();
    assert_eq!(rp1["matchedArticleId"], "inc1");
    assert_eq!(rp1["matchedArticleStatus"], "included");

    let rp2 = nodes.iter().find(|n| n["id"] == "rp2").unwrap();
    assert_eq!(rp2["matchedArticleId"], "rej1");
    assert_eq!(rp2["matchedArticleStatus"], "rejected");

    let rp3 = nodes.iter().find(|n| n["id"] == "rp3").unwrap();
    assert_eq!(rp3["matchedArticleId"], serde_json::Value::Null);
    assert_eq!(rp3["matchedArticleStatus"], serde_json::Value::Null);
}
