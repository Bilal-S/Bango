use bango_lib::db::biblio_repo::{
    build_coauthor_edges, compute_author_metrics, get_coauthor_network_json,
    get_institutions_by_author, normalize_affiliations, normalize_authors_from_articles,
};
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

#[test]
fn test_biblio_normalization_pipeline() {
    // 1. Setup DB
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // 2. Insert mock articles with affiliations and citation counts
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, affiliation, publication_year, num_cited) \
         VALUES ('art-1', 'included', 'Title 1', 'Abstract 1', '[\"Smith, J\", \"Doe, J\"]', 'MIT, Cambridge, MA, USA; Stanford University, Stanford, CA, USA', 2020, 10)",
        [],
    ).expect("Failed to insert article 1");

    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, affiliation, publication_year, num_cited) \
         VALUES ('art-2', 'included', 'Title 2', 'Abstract 2', '[\"Doe, J\", \"Brown, A\"]', 'Stanford University, Stanford, CA, USA; Oxford University, Oxford, UK', 2022, 5)",
        [],
    ).expect("Failed to insert article 2");

    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, affiliation, publication_year, num_cited) \
         VALUES ('art-3', 'included', 'Title 3', 'Abstract 3', '[\"Smith, J\"]', 'Dept of CS, MIT, Cambridge, MA', 2021, 3)",
        [],
    ).expect("Failed to insert article 3");

    // 3. Run normalization pipeline steps
    let authors_count =
        normalize_authors_from_articles(&conn).expect("normalize_authors_from_articles failed");
    assert_eq!(authors_count, 3, "Should have created 3 unique authors");

    let (inst_created, links_created) =
        normalize_affiliations(&conn).expect("normalize_affiliations failed");
    // Institutions expected: "mit", "stanford university", "oxford university" (3 total)
    // Links expected:
    // - art-1, Smith J -> mit
    // - art-1, Doe J -> stanford university
    // - art-2, Doe J -> stanford university
    // - art-2, Brown A -> oxford university
    // - art-3, Smith J -> mit
    // (5 total links)
    assert_eq!(inst_created, 3, "Should have created 3 institutions");
    assert_eq!(links_created, 5, "Should have created 5 author-institution links");

    compute_author_metrics(&conn).expect("compute_author_metrics failed");
    build_coauthor_edges(&conn).expect("build_coauthor_edges failed");

    // 4. Verify Author metrics
    struct AuthorMetric {
        display_name: String,
        article_count: i64,
        total_citations: i64,
        avg_year: f64,
    }

    let mut stmt = conn.prepare(
        "SELECT display_name, article_count, total_citations, avg_year FROM biblio_authors ORDER BY normalized_name"
    ).unwrap();
    let metrics: Vec<AuthorMetric> = stmt
        .query_map([], |row| {
            Ok(AuthorMetric {
                display_name: row.get(0)?,
                article_count: row.get(1)?,
                total_citations: row.get(2)?,
                avg_year: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(metrics.len(), 3);

    // Ordered alphabetically by normalized_name: "brown a", "doe j", "smith j"
    let brown = &metrics[0];
    assert_eq!(brown.display_name, "Brown, A");
    assert_eq!(brown.article_count, 1);
    assert_eq!(brown.total_citations, 5);
    assert_eq!(brown.avg_year, 2022.0);

    let doe = &metrics[1];
    assert_eq!(doe.display_name, "Doe, J");
    assert_eq!(doe.article_count, 2);
    assert_eq!(doe.total_citations, 15);
    assert_eq!(doe.avg_year, 2021.0); // (2020 + 2022) / 2

    let smith = &metrics[2];
    assert_eq!(smith.display_name, "Smith, J");
    assert_eq!(smith.article_count, 2);
    assert_eq!(smith.total_citations, 13);
    assert_eq!(smith.avg_year, 2020.5); // (2020 + 2021) / 2

    // 5. Verify Institutions mapping
    let smith_author_id: String = conn
        .query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'smith j'", [], |row| {
            row.get(0)
        })
        .unwrap();
    let institutions = get_institutions_by_author(&conn, &smith_author_id)
        .expect("get_institutions_by_author failed");
    assert_eq!(institutions.len(), 1);
    assert_eq!(institutions[0].normalized_name, "mit");
    assert_eq!(institutions[0].city.as_deref(), Some("Cambridge"));
    assert_eq!(institutions[0].country.as_deref(), Some("USA"));

    let doe_author_id: String = conn
        .query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'doe j'", [], |row| {
            row.get(0)
        })
        .unwrap();
    let institutions_doe = get_institutions_by_author(&conn, &doe_author_id)
        .expect("get_institutions_by_author failed");
    assert_eq!(institutions_doe.len(), 1);
    assert_eq!(institutions_doe[0].normalized_name, "stanford university");
    assert_eq!(institutions_doe[0].city.as_deref(), Some("Stanford"));
    assert_eq!(institutions_doe[0].country.as_deref(), Some("USA"));

    // 6. Verify Network JSON structure
    let json = get_coauthor_network_json(&conn).expect("get_coauthor_network_json failed");
    let nodes = json.get("nodes").unwrap().as_array().unwrap();
    let edges = json.get("edges").unwrap().as_array().unwrap();

    // Check node count
    assert_eq!(nodes.len(), 3);
    // Check edge count
    // Smith J - Doe J (on art-1)
    // Doe J - Brown A (on art-2)
    // total 2 edges
    assert_eq!(edges.len(), 2);
}
