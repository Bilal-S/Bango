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

#[test]
fn test_biblio_ordered_affiliation_matching() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // 1. Article with C3 matching
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, custom_field3, author_address, affiliation, publication_year) \
         VALUES ('art-1', 'included', 'C3 Match', 'Abs', '[\"Smith, J\", \"Doe, J\"]', 'Harvard Univ, Boston, MA; Yale Univ, New Haven, CT', 'Stanford Univ; MIT', 'Oxford; Cambridge', 2024)",
        [],
    ).expect("Insert art-1 failed");

    // 2. Article with AD fallback
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, custom_field3, author_address, affiliation, publication_year) \
         VALUES ('art-2', 'included', 'AD Fallback', 'Abs', '[\"Smith, J\", \"Brown, A\"]', 'Yale Univ', 'Stanford Univ, Stanford, CA; Oxford Univ, Oxford, UK', 'Cambridge', 2020)",
        [],
    ).expect("Insert art-2 failed");

    // 3. Run normalization
    normalize_authors_from_articles(&conn).expect("normalize_authors_from_articles failed");
    normalize_affiliations(&conn).expect("normalize_affiliations failed");

    // Fetch author IDs
    let smith_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'smith j'", [], |r| r.get(0)).unwrap();
    let doe_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'doe j'", [], |r| r.get(0)).unwrap();
    let brown_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'brown a'", [], |r| r.get(0)).unwrap();

    // Verify raw affiliations mapped to Smith J (order 0 in art-1: Harvard Univ, order 0 in art-2: Stanford Univ)
    let smith_affs: Vec<String> = {
        let mut stmt = conn.prepare("SELECT raw_affiliation FROM biblio_article_authors WHERE author_id = ?1 ORDER BY article_id").unwrap();
        stmt.query_map([&smith_id], |row| row.get(0)).unwrap().collect::<Result<Vec<Option<String>>, _>>().unwrap().into_iter().flatten().collect()
    };
    assert_eq!(smith_affs, vec!["Harvard Univ, Boston, MA", "Stanford Univ, Stanford, CA"]);

    // Verify Yale Univ mapped to Doe J (order 1 in art-1)
    let doe_affs: Vec<String> = {
        let mut stmt = conn.prepare("SELECT raw_affiliation FROM biblio_article_authors WHERE author_id = ?1").unwrap();
        stmt.query_map([&doe_id], |row| row.get(0)).unwrap().collect::<Result<Vec<Option<String>>, _>>().unwrap().into_iter().flatten().collect()
    };
    assert_eq!(doe_affs, vec!["Yale Univ, New Haven, CT"]);

    // Verify Oxford Univ mapped to Brown A (order 1 in art-2)
    let brown_affs: Vec<String> = {
        let mut stmt = conn.prepare("SELECT raw_affiliation FROM biblio_article_authors WHERE author_id = ?1").unwrap();
        stmt.query_map([&brown_id], |row| row.get(0)).unwrap().collect::<Result<Vec<Option<String>>, _>>().unwrap().into_iter().flatten().collect()
    };
    assert_eq!(brown_affs, vec!["Oxford Univ, Oxford, UK"]);

    // 4. Verify Sorting: get_institutions_by_author should return Smith's institutions sorted by publication date desc:
    // Harvard Univ is associated with art-1 (2024)
    // Stanford Univ is associated with art-2 (2020)
    // So Harvard Univ (2024) should be first, Stanford Univ (2020) should be second.
    let smith_insts = get_institutions_by_author(&conn, &smith_id).unwrap();
    assert_eq!(smith_insts.len(), 2);
    assert_eq!(smith_insts[0].normalized_name, "harvard univ");
    assert_eq!(smith_insts[1].normalized_name, "stanford univ");

    // If we associate Stanford Univ with a more recent article (e.g. 2025), it should shift to first place
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, custom_field3, author_address, affiliation, publication_year) \
         VALUES ('art-3', 'included', 'Recent Stanford', 'Abs', '[\"Smith, J\"]', 'Stanford Univ, Stanford, CA', 'MIT', 'Cambridge', 2025)",
        [],
    ).expect("Insert art-3 failed");

    // Re-run normalization
    conn.execute("DELETE FROM biblio_article_authors", []).unwrap();
    conn.execute("DELETE FROM biblio_authors", []).unwrap();
    conn.execute("DELETE FROM biblio_author_affiliations", []).unwrap();
    conn.execute("DELETE FROM biblio_institutions", []).unwrap();
    normalize_authors_from_articles(&conn).expect("normalize_authors_from_articles failed");
    normalize_affiliations(&conn).expect("normalize_affiliations failed");

    let new_smith_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'smith j'", [], |r| r.get(0)).unwrap();
    let new_smith_insts = get_institutions_by_author(&conn, &new_smith_id).unwrap();
    assert_eq!(new_smith_insts.len(), 2);
    // Stanford Univ (2025) should now be first, Harvard Univ (2024) should be second.
    assert_eq!(new_smith_insts[0].normalized_name, "stanford univ");
    assert_eq!(new_smith_insts[1].normalized_name, "harvard univ");
}

#[test]
fn test_co_author_w_affiliation_integration() {
    use std::fs;
    use std::path::PathBuf;
    use bango_lib::ris::parser::parse_ris;
    use bango_lib::commands::import::ris_record_to_new_article;
    use bango_lib::db::article_repo;

    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Load and parse co-author-w-affilitation.ris
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets/co-author-w-affilitation.ris");
    let content = fs::read_to_string(path).expect("fixture not found");
    let parse_result = parse_ris(&content).expect("Parse failed");
    
    // Verify records parsed correctly
    assert_eq!(parse_result.records.len(), 12);
    
    // Map to NewArticles and insert
    let new_articles: Vec<_> = parse_result.records.iter().map(ris_record_to_new_article).collect();
    let inserted = article_repo::insert_articles_batch(&conn, &new_articles, "co-author-w-affilitation.ris")
        .expect("Insert failed");
    assert_eq!(inserted.len(), 12);

    // Set all articles status to 'included' so they are picked up by biblio normalization
    conn.execute("UPDATE articles SET status = 'included'", []).unwrap();

    // Run normalization pipeline
    let authors_count = normalize_authors_from_articles(&conn).expect("normalize_authors_from_articles failed");
    assert!(authors_count > 0);

    let (inst_created, links_created) = normalize_affiliations(&conn).expect("normalize_affiliations failed");
    assert!(inst_created > 0);
    assert!(links_created > 0);

    // Let's verify specific mappings:
    // First paper:
    // AU: Smith, John
    // AU: Lee, Alice
    // AU: Patel, Nisha
    // C3: Department of Supply Chain Management, Sheffield Hallam University...
    // C3: School of Business, University of Manchester...
    // C3: Faculty of Management, University of Leeds...

    // Patel, Nisha (normalized name "patel n") should be mapped to University of Leeds
    let patel_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'patel n'", [], |r| r.get(0)).unwrap();
    let patel_insts = get_institutions_by_author(&conn, &patel_id).unwrap();
    assert_eq!(patel_insts.len(), 1);
    assert!(patel_insts[0].normalized_name.contains("university of leeds") || patel_insts[0].normalized_name.contains("leeds"));

    // Osei, Kwame (normalized name "osei k" from Paper 4) should be mapped to University of Ghana
    let osei_id: String = conn.query_row("SELECT id FROM biblio_authors WHERE normalized_name = 'osei k'", [], |r| r.get(0)).unwrap();
    let osei_insts = get_institutions_by_author(&conn, &osei_id).unwrap();
    assert_eq!(osei_insts.len(), 1);
    assert!(osei_insts[0].normalized_name.contains("university of ghana") || osei_insts[0].normalized_name.contains("ghana"));
}
