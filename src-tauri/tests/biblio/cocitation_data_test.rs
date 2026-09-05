//! Integration test validating co-citation analysis against RIS fixture data.
//!
//! This test loads the dedicated co-citation dataset from `tests/test-citations/`:
//!   - `co-citation.ris` - 5 main (citing) articles
//!   - `10.2001_cocite{1..5}_references.ris` - shared reference papers R1–R6
//!   - `10.2001_cocite{1..5}_citations.ris` - dummy citing papers (not used in co-citation)
//!
//! ## Dataset Topology
//!
//! | Article   | Cites (references) | Cocitation partners formed |
//! |-----------|--------------------|-----------------------------|
//! | cocite1   | R1, R2, R3         | R1–R2, R1–R3, R2–R3          |
//! | cocite2   | R1, R2, R4         | R1–R2, R1–R4, R2–R4          |
//! | cocite3   | R1, R2, R5         | R1–R2, R1–R5, R2–R5          |
//! | cocite4   | R3, R4, R6         | R3–R4, R3–R6, R4–R6          |
//! | cocite5   | R5, R6             | R5–R6                        |
//!
//! ## Expected Co-Citation Counts (c_ij)
//!
//! - **R1–R2**: 3 (cocite1, cocite2, cocite3) - dominant pair
//! - **R1–R3**: 1 (cocite1)
//! - **R1–R4**: 1 (cocite2)
//! - **R1–R5**: 1 (cocite3)
//! - **R2–R3**: 1 (cocite1)
//! - **R2–R4**: 1 (cocite2)
//! - **R2–R5**: 1 (cocite3)
//! - **R3–R4**: 1 (cocite4)
//! - **R3–R6**: 1 (cocite4)
//! - **R4–R6**: 1 (cocite4)
//! - **R5–R6**: 1 (cocite5)
//!
//! **Total: 11 co-citation edges** (with min_citation_count=1, min_co_citation=1).
//!
//! ## Per-Paper Citation Totals (c_i)
//!
//! - **R1**: 3 (cocite1, cocite2, cocite3)
//! - **R2**: 3 (cocite1, cocite2, cocite3)
//! - **R3**: 2 (cocite1, cocite4)
//! - **R4**: 2 (cocite2, cocite4)
//! - **R5**: 2 (cocite3, cocite5)
//! - **R6**: 2 (cocite4, cocite5)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use bango_lib::commands::import::ris_record_to_new_article;
use bango_lib::commands::references::ris_record_to_reference_paper;
use bango_lib::db::article_repo;
use bango_lib::db::biblio_repo::{
    get_cocitation_network_json, CocitationNormalization, CocitationScope,
};
use bango_lib::db::migration::run_migrations;
use bango_lib::db::reference_repo::{create_link, insert_or_find_paper};
use bango_lib::models::reference::ReferenceType;
use bango_lib::ris::parser::parse_ris;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Path to the `tests/test-citations/` directory from the cargo workspace root.
fn citations_dir() -> PathBuf {
    Path::new("../tests/test-citations").to_path_buf()
}

/// Read a file from `tests/test-citations/` by name.
fn read_fixture(name: &str) -> String {
    fs::read_to_string(citations_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to read fixture '{name}': {e}"))
}

/// Create an in-memory database with all migrations applied.
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Import the 5 main articles from `co-citation.ris` and move them to 'included'.
///
/// Returns a map of article DOI → article DB ID.
fn import_main_articles(conn: &Connection) -> HashMap<String, String> {
    let content = read_fixture("co-citation.ris");
    let parse_result = parse_ris(&content).expect("Parse co-citation.ris failed");
    assert_eq!(parse_result.records.len(), 5, "co-citation.ris should contain exactly 5 articles");

    let new_articles: Vec<_> = parse_result.records.iter().map(ris_record_to_new_article).collect();
    let imported = article_repo::insert_articles_batch(conn, &new_articles, "co-citation.ris")
        .expect("Insert main articles failed");
    assert_eq!(imported.len(), 5, "Should import all 5 articles");

    // Move each article from 'duplicate' → 'working' → 'included'.
    let mut doi_map = HashMap::new();
    for article in &imported {
        article_repo::move_to_working(conn, &article.id).expect("move_to_working failed");
        article_repo::update_article_status(conn, &article.id, "included")
            .expect("update_article_status failed");
        if let Some(doi) = &article.doi {
            doi_map.insert(doi.clone(), article.id.clone());
        }
    }

    // Sanity: all 5 articles must be 'included' for co-citation (default scope).
    let included_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(included_count, 5, "All 5 articles should be 'included'");

    doi_map
}

/// Import the reference papers for a single article from its `_references.ris` file.
///
/// Uses `insert_or_find_paper` so that shared reference papers (same DOI) resolve
/// to the same DB row across multiple articles.
fn import_references_for_article(conn: &Connection, article_id: &str, fixture_name: &str) {
    let content = read_fixture(fixture_name);
    let parse_result =
        parse_ris(&content).unwrap_or_else(|e| panic!("Parse {fixture_name} failed: {e}"));

    for record in &parse_result.records {
        let new_paper = ris_record_to_reference_paper(record);
        let (paper, _is_new) = insert_or_find_paper(conn, &new_paper)
            .unwrap_or_else(|e| panic!("insert_or_find_paper failed for {fixture_name}: {e}"));
        create_link(conn, article_id, &paper.id, &ReferenceType::Reference)
            .unwrap_or_else(|e| panic!("create_link failed for {fixture_name}: {e}"));
    }
}

/// Import all 5 articles and their references. Returns a DOI→ID map for main articles.
fn setup_full_dataset() -> (Connection, HashMap<String, String>) {
    let conn = test_db();
    let doi_map = import_main_articles(&conn);

    // Each cocite article → its references file.
    let fixture_map = [
        ("10.2001/cocite1", "10.2001_cocite1_references.ris"),
        ("10.2001/cocite2", "10.2001_cocite2_references.ris"),
        ("10.2001/cocite3", "10.2001_cocite3_references.ris"),
        ("10.2001/cocite4", "10.2001_cocite4_references.ris"),
        ("10.2001/cocite5", "10.2001_cocite5_references.ris"),
    ];

    for (doi, fixture) in fixture_map {
        let article_id =
            doi_map.get(doi).unwrap_or_else(|| panic!("missing article for DOI {doi}"));
        import_references_for_article(&conn, article_id, fixture);
    }

    (conn, doi_map)
}

/// Build a set of reference-paper DOIs from the nodes array (for lookups).
fn node_dois(json: &serde_json::Value) -> HashMap<String, String> {
    // Map: DOI → node ID (reference_paper ID)
    json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| {
            let doi = n["doi"].as_str()?.to_string();
            let id = n["id"].as_str()?.to_string();
            Some((doi, id))
        })
        .collect()
}

/// Find an edge between two node IDs (undirected - checks both orderings).
fn find_edge<'a>(
    edges: &'a [serde_json::Value],
    id_a: &str,
    id_b: &str,
) -> Option<&'a serde_json::Value> {
    edges.iter().find(|e| {
        let s = e["source"].as_str().unwrap_or("");
        let t = e["target"].as_str().unwrap_or("");
        (s == id_a && t == id_b) || (s == id_b && t == id_a)
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn cocitation_data_raw_edge_count_and_dominant_pair() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1, // min_citation_count
        1, // min_co_citation
    )
    .expect("get_cocitation_network_json failed");

    let edges = json["edges"].as_array().unwrap();
    let nodes = json["nodes"].as_array().unwrap();

    // 6 unique reference papers should appear as nodes.
    assert_eq!(nodes.len(), 6, "Expected 6 co-citation nodes (R1–R6)");

    // 11 co-citation pairs total.
    assert_eq!(edges.len(), 11, "Expected 11 co-citation edges");

    // Find the dominant R1–R2 edge (co-cited by 3 articles).
    let doi_to_id = node_dois(&json);
    let r1_id = doi_to_id.get("10.3001/ref1").expect("R1 node missing");
    let r2_id = doi_to_id.get("10.3001/ref2").expect("R2 node missing");

    let dominant_edge = find_edge(edges, r1_id, r2_id).expect("R1–R2 edge should exist");
    assert_eq!(
        dominant_edge["rawWeight"].as_i64().unwrap(),
        3,
        "R1–R2 should be co-cited by 3 articles"
    );

    // The dominant edge should be first (sorted by raw count descending).
    let first_raw = edges[0]["rawWeight"].as_i64().unwrap();
    assert_eq!(first_raw, 3, "Strongest edge should have rawWeight=3");
}

#[test]
fn cocitation_data_per_paper_citation_totals() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let nodes = json["nodes"].as_array().unwrap();

    // coCitationCount = c_i (how many articles cite this paper).
    let c_i: HashMap<String, i64> = nodes
        .iter()
        .filter_map(|n| {
            let doi = n["doi"].as_str()?.to_string();
            let count = n["coCitationCount"].as_i64()?;
            Some((doi, count))
        })
        .collect();

    assert_eq!(c_i.get("10.3001/ref1"), Some(&3), "R1 cited by 3 articles");
    assert_eq!(c_i.get("10.3001/ref2"), Some(&3), "R2 cited by 3 articles");
    assert_eq!(c_i.get("10.3001/ref3"), Some(&2), "R3 cited by 2 articles");
    assert_eq!(c_i.get("10.3001/ref4"), Some(&2), "R4 cited by 2 articles");
    assert_eq!(c_i.get("10.3001/ref5"), Some(&2), "R5 cited by 2 articles");
    assert_eq!(c_i.get("10.3001/ref6"), Some(&2), "R6 cited by 2 articles");
}

#[test]
fn cocitation_data_doi_dedup_produces_6_unique_papers() {
    let (conn, _doi_map) = setup_full_dataset();

    // Verify in the DB directly: 6 distinct reference papers should exist.
    let paper_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT doi) FROM reference_papers WHERE doi LIKE '10.3001/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(paper_count, 6, "Should have exactly 6 unique reference papers (R1–R6)");

    // Each reference paper should have exactly the right number of article links.
    let r1_links: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT l.parent_article_id) \
             FROM article_reference_links l \
             JOIN reference_papers rp ON rp.id = l.reference_paper_id \
             WHERE rp.doi = '10.3001/ref1' AND l.type = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(r1_links, 3, "R1 should be linked to 3 articles");
}

#[test]
fn cocitation_data_cosine_normalization_r1_r2() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Cosine,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let edges = json["edges"].as_array().unwrap();
    let doi_to_id = node_dois(&json);
    let r1_id = doi_to_id.get("10.3001/ref1").unwrap();
    let r2_id = doi_to_id.get("10.3001/ref2").unwrap();

    let edge = find_edge(edges, r1_id, r2_id).expect("R1–R2 edge missing");

    // c_ij=3, c_i=3, c_j=3 → cosine = 3/√(9) = 1.0
    let cosine = edge["weight"].as_f64().unwrap();
    assert!(
        (cosine - 1.0).abs() < 0.001,
        "R1–R2 cosine should be 1.0 (identical citation pattern), got {cosine}"
    );

    // Also verify the r3-r4 pair: c_ij=1, c_i=2, c_j=2 → cosine = 1/√4 = 0.5
    let r3_id = doi_to_id.get("10.3001/ref3").unwrap();
    let r4_id = doi_to_id.get("10.3001/ref4").unwrap();
    let r3r4_edge = find_edge(edges, r3_id, r4_id).expect("R3–R4 edge missing");
    let cosine_34 = r3r4_edge["weight"].as_f64().unwrap();
    assert!((cosine_34 - 0.5).abs() < 0.001, "R3–R4 cosine should be 0.5, got {cosine_34}");
}

#[test]
fn cocitation_data_jaccard_normalization() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Jaccard,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let edges = json["edges"].as_array().unwrap();
    let doi_to_id = node_dois(&json);
    let r1_id = doi_to_id.get("10.3001/ref1").unwrap();
    let r2_id = doi_to_id.get("10.3001/ref2").unwrap();

    let edge = find_edge(edges, r1_id, r2_id).expect("R1–R2 edge missing");

    // c_ij=3, c_i=3, c_j=3 → jaccard = 3/(3+3−3) = 3/3 = 1.0
    let jaccard = edge["weight"].as_f64().unwrap();
    assert!((jaccard - 1.0).abs() < 0.001, "R1–R2 jaccard should be 1.0, got {jaccard}");

    // R3–R4: c_ij=1, c_i=2, c_j=2 → jaccard = 1/(2+2−1) = 1/3 ≈ 0.333
    let r3_id = doi_to_id.get("10.3001/ref3").unwrap();
    let r4_id = doi_to_id.get("10.3001/ref4").unwrap();
    let r3r4 = find_edge(edges, r3_id, r4_id).expect("R3–R4 edge missing");
    let jaccard_34 = r3r4["weight"].as_f64().unwrap();
    assert!(
        (jaccard_34 - (1.0 / 3.0)).abs() < 0.001,
        "R3–R4 jaccard should be ~0.333, got {jaccard_34}"
    );
}

#[test]
fn cocitation_data_pearson_normalization() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Pearson,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let edges = json["edges"].as_array().unwrap();
    let doi_to_id = node_dois(&json);
    let r1_id = doi_to_id.get("10.3001/ref1").unwrap();
    let r2_id = doi_to_id.get("10.3001/ref2").unwrap();

    let edge = find_edge(edges, r1_id, r2_id).expect("R1–R2 edge missing");

    // R1 and R2 are cited by the exact same set of articles (cocite1, cocite2, cocite3).
    // Perfect positive correlation → pearson = 1.0
    let pearson = edge["weight"].as_f64().unwrap();
    assert!(
        (pearson - 1.0).abs() < 0.001,
        "R1–R2 pearson should be 1.0 (identical citation patterns), got {pearson}"
    );
}

#[test]
fn cocitation_data_min_citation_count_filter() {
    let (conn, _doi_map) = setup_full_dataset();

    // min_citation_count=3 → only R1 and R2 qualify (each cited by 3 articles).
    // R1–R2 form a pair → 1 edge, 2 nodes.
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        3, // min_citation_count
        1, // min_co_citation
    )
    .expect("get_cocitation_network_json failed");

    let nodes = json["nodes"].as_array().unwrap();
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(nodes.len(), 2, "Only R1 and R2 pass min_citation_count=3");
    assert_eq!(edges.len(), 1, "Only the R1–R2 edge survives");
}

#[test]
fn cocitation_data_min_co_citation_filter() {
    let (conn, _doi_map) = setup_full_dataset();

    // min_co_citation=2 → only the R1–R2 pair (co-cited 3 times) survives.
    // All other pairs are co-cited only once.
    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1, // min_citation_count
        2, // min_co_citation
    )
    .expect("get_cocitation_network_json failed");

    let edges = json["edges"].as_array().unwrap();
    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(edges.len(), 1, "Only R1–R2 has co-citation >= 2");
    assert_eq!(nodes.len(), 2, "Only R1 and R2 appear in surviving edge");

    // Verify it's the R1–R2 edge.
    let doi_to_id = node_dois(&json);
    let r1_id = doi_to_id.get("10.3001/ref1").unwrap();
    let r2_id = doi_to_id.get("10.3001/ref2").unwrap();
    assert!(find_edge(edges, r1_id, r2_id).is_some(), "Surviving edge should be R1–R2");
}

#[test]
fn cocitation_data_meta_block_diagnostics() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let meta = &json["meta"];
    assert_eq!(meta["inScopeArticleCount"].as_i64(), Some(5), "5 in-scope articles");
    assert_eq!(meta["scope"].as_str(), Some("included"));
    assert_eq!(meta["normalization"].as_str(), Some("raw"));
    assert_eq!(meta["nodeCount"].as_i64(), Some(6), "6 candidate nodes");
    assert_eq!(meta["edgeCount"].as_i64(), Some(11), "11 co-citation edges");
    assert_eq!(
        meta["referencePaperCount"].as_i64(),
        Some(6),
        "6 distinct reference papers linked to in-scope articles"
    );
}

#[test]
fn cocitation_data_node_metadata_from_reference_papers() {
    let (conn, _doi_map) = setup_full_dataset();

    let json = get_cocitation_network_json(
        &conn,
        CocitationScope::IncludedArticles,
        CocitationNormalization::Raw,
        1,
        1,
    )
    .expect("get_cocitation_network_json failed");

    let nodes = json["nodes"].as_array().unwrap();

    // Find R1 and verify its metadata comes from the reference_papers table.
    let r1 = nodes
        .iter()
        .find(|n| n["doi"].as_str() == Some("10.3001/ref1"))
        .expect("R1 node not found");

    assert_eq!(r1["title"].as_str().unwrap(), "Pattern Recognition and Machine Learning");
    assert_eq!(r1["year"].as_i64(), Some(2006));
    assert_eq!(r1["journal"].as_str(), Some("Journal of Machine Learning Research"));
    // R1 is a shared reference, never promoted to an article.
    assert_eq!(r1["matchedArticleId"].as_null(), Some(()), "R1 should be unmatched");
}
