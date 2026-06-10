//! Integration tests for project backup export/import round-trip.
//! Verifies all tables survive serialize → deserialize correctly.

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::export::project::{export_project, import_project};
use bango_lib::models::article::NewArticle;
use rusqlite::params;

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

fn new_article(title: &str) -> NewArticle {
    NewArticle { title: title.to_string(), ..Default::default() }
}

/// Count rows in a table.
fn count_rows(conn: &rusqlite::Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| row.get(0)).unwrap_or(0)
}

/// Seed all core tables with one row each (non-biblio).
fn seed_core_data(conn: &rusqlite::Connection) {
    // Research aim
    conn.execute(
        "INSERT INTO research_aims (id, text, created_at) VALUES ('aim-1', 'Test aim', '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed aim");

    // Criterion
    conn.execute(
        "INSERT INTO criteria (id, type, text, priority, created_at) VALUES ('crit-1', 'inclusion', 'Must be relevant', 'high', '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed criterion");

    // Tags & labels
    conn.execute("INSERT INTO tags (id, name, source) VALUES ('tag-1', 'ml', 'ai_suggested')", [])
        .expect("seed tag");
    conn.execute(
        "INSERT INTO labels (id, name, source) VALUES ('lbl-1', 'priority', 'ai_generated')",
        [],
    )
    .expect("seed label");

    // Article
    let a =
        article_repo::insert_article(conn, &new_article("Test Article")).expect("insert article");
    article_repo::move_to_working(conn, &a.id).expect("move to working");

    // Article tags & labels
    conn.execute(
        "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, 'tag-1')",
        params![a.id],
    )
    .expect("seed article_tag");
    conn.execute(
        "INSERT INTO article_labels (article_id, label_id) VALUES (?1, 'lbl-1')",
        params![a.id],
    )
    .expect("seed article_label");

    // Audit entry
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) VALUES ('aud-1', ?1, '2026-01-01T00:00:00Z', 'import', 'imported', 'system')",
        params![a.id],
    ).expect("seed audit");

    // Reference paper
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, abstract_text, match_status, created_at, updated_at) VALUES ('rp-1', 'Ref Paper', '[\"Author\"]', 'Abstract', 'unmatched', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed ref paper");

    // Article reference link
    conn.execute(
        "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type, created_at) VALUES ('rl-1', ?1, 'rp-1', 0, '2026-01-01T00:00:00Z')",
        params![a.id],
    ).expect("seed ref link");

    // LLM config
    conn.execute(
        "INSERT INTO llm_config (id, provider, endpoint_url, model_name, temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) VALUES (1, 'openai', 'https://api.example.com', 'gpt-4', 0.2, 3, 500, 50000)",
        [],
    ).expect("seed llm config");
}

/// Seed all biblio tables with one row each.
fn seed_biblio_data(conn: &rusqlite::Connection, article_id: &str) {
    // biblio_authors
    conn.execute(
        "INSERT INTO biblio_authors (id, normalized_name, display_name, first_author_count, article_count, created_at) VALUES ('ba-1', 'smith j', 'Smith, J.', 1, 1, '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed biblio_author");

    // biblio_institutions
    conn.execute(
        "INSERT INTO biblio_institutions (id, normalized_name, country, city) VALUES ('bi-1', 'mit', 'US', 'Cambridge')",
        [],
    ).expect("seed biblio_institution");

    // biblio_article_authors
    conn.execute(
        "INSERT INTO biblio_article_authors (id, article_id, author_id, author_order, raw_name, raw_affiliation) VALUES ('baa-1', ?1, 'ba-1', 0, 'Smith J.', 'MIT')",
        params![article_id],
    ).expect("seed biblio_article_author");

    // biblio_author_affiliations
    conn.execute(
        "INSERT INTO biblio_author_affiliations (id, author_id, institution_id, article_id) VALUES ('baf-1', 'ba-1', 'bi-1', ?1)",
        params![article_id],
    ).expect("seed biblio_author_affiliation");

    // biblio_terms
    conn.execute(
        "INSERT INTO biblio_terms (id, normalized_term, raw_term, term_type, article_count, created_at) VALUES ('bt-1', 'machine learning', 'Machine Learning', 'keyword', 1, '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed biblio_term");

    // biblio_article_terms
    conn.execute(
        "INSERT INTO biblio_article_terms (id, article_id, term_id, frequency) VALUES ('bat-1', ?1, 'bt-1', 3)",
        params![article_id],
    ).expect("seed biblio_article_term");

    // biblio_network_meta
    conn.execute(
        "INSERT INTO biblio_network_meta (id, network_type, label, article_filter, params_json, node_count, edge_count, created_at) VALUES ('bnm-1', 'co_authorship', 'Test Network', NULL, NULL, 1, 0, '2026-01-01T00:00:00Z')",
        [],
    ).expect("seed biblio_network_meta");

    // biblio_network_nodes
    conn.execute(
        "INSERT INTO biblio_network_nodes (id, network_id, entity_id, label, weight, cluster, x, y) VALUES ('bnn-1', 'bnm-1', 'ba-1', 'Smith, J.', 1.0, NULL, 0.5, 0.5)",
        [],
    ).expect("seed biblio_network_node");

    // biblio_network_edges
    conn.execute(
        "INSERT INTO biblio_network_edges (id, network_id, source_id, target_id, weight) VALUES ('bne-1', 'bnm-1', 'ba-1', 'ba-1', 1.0)",
        [],
    ).expect("seed biblio_network_edge");
}

#[test]
fn test_export_import_round_trip_all_tables() {
    let conn = setup_db();
    seed_core_data(&conn);

    // Get article id for biblio seeding
    let article_id: String = conn
        .query_row("SELECT id FROM articles LIMIT 1", [], |row| row.get(0))
        .expect("get article id");
    seed_biblio_data(&conn, &article_id);

    // Record pre-export counts for every table
    let pre = [
        ("research_aims", count_rows(&conn, "research_aims")),
        ("criteria", count_rows(&conn, "criteria")),
        ("articles", count_rows(&conn, "articles")),
        ("tags", count_rows(&conn, "tags")),
        ("labels", count_rows(&conn, "labels")),
        ("article_tags", count_rows(&conn, "article_tags")),
        ("article_labels", count_rows(&conn, "article_labels")),
        ("audit_entries", count_rows(&conn, "audit_entries")),
        ("reference_papers", count_rows(&conn, "reference_papers")),
        ("article_reference_links", count_rows(&conn, "article_reference_links")),
        ("llm_config", count_rows(&conn, "llm_config")),
        ("biblio_authors", count_rows(&conn, "biblio_authors")),
        ("biblio_article_authors", count_rows(&conn, "biblio_article_authors")),
        ("biblio_institutions", count_rows(&conn, "biblio_institutions")),
        ("biblio_author_affiliations", count_rows(&conn, "biblio_author_affiliations")),
        ("biblio_terms", count_rows(&conn, "biblio_terms")),
        ("biblio_article_terms", count_rows(&conn, "biblio_article_terms")),
        ("biblio_network_meta", count_rows(&conn, "biblio_network_meta")),
        ("biblio_network_nodes", count_rows(&conn, "biblio_network_nodes")),
        ("biblio_network_edges", count_rows(&conn, "biblio_network_edges")),
    ];

    // Verify all tables were seeded
    for (table, count) in &pre {
        assert_eq!(*count, 1, "Pre-condition: {} should have 1 row, got {}", table, count);
    }

    // Export
    let json = export_project(&conn).expect("export_project failed");

    // Parse JSON to verify structure
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON parse failed");
    assert_eq!(parsed["researchAims"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["criteria"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["articles"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["tags"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["labels"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["articleTags"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["articleLabels"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["auditEntries"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["referencePapers"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["articleReferenceLinks"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioAuthors"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioArticleAuthors"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioInstitutions"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioAuthorAffiliations"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioTerms"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioArticleTerms"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioNetworkMeta"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioNetworkNodes"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["biblioNetworkEdges"].as_array().unwrap().len(), 1);
    assert!(parsed["llmConfig"].is_object());

    // Import into a fresh database
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import_project failed");

    // Verify post-import counts match pre-export counts
    let tables = [
        "research_aims",
        "criteria",
        "articles",
        "tags",
        "labels",
        "article_tags",
        "article_labels",
        "audit_entries",
        "reference_papers",
        "article_reference_links",
        "llm_config",
        "biblio_authors",
        "biblio_article_authors",
        "biblio_institutions",
        "biblio_author_affiliations",
        "biblio_terms",
        "biblio_article_terms",
        "biblio_network_meta",
        "biblio_network_nodes",
        "biblio_network_edges",
    ];
    for table in &tables {
        let post_count = count_rows(&conn2, table);
        assert_eq!(post_count, 1, "Post-import: {} should have 1 row, got {}", table, post_count);
    }
}

#[test]
fn test_import_old_backup_without_biblio_data() {
    let conn = setup_db();

    // Minimal backup JSON without any biblio fields (simulates old v3.0 backup)
    let old_backup = r#"{
        "metadata": {
            "specVersion": "3.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "appName": "Bango",
            "appVersion": "2.0.0"
        },
        "researchAims": [],
        "criteria": [],
        "articles": [],
        "tags": [],
        "labels": [],
        "articleTags": [],
        "articleLabels": [],
        "auditEntries": [],
        "llmConfig": null
    }"#;

    import_project(&conn, old_backup).expect("import of old backup should succeed");

    // All biblio tables should be empty (no panic, no error)
    assert_eq!(count_rows(&conn, "biblio_authors"), 0);
    assert_eq!(count_rows(&conn, "biblio_article_authors"), 0);
    assert_eq!(count_rows(&conn, "biblio_institutions"), 0);
    assert_eq!(count_rows(&conn, "biblio_author_affiliations"), 0);
    assert_eq!(count_rows(&conn, "biblio_terms"), 0);
    assert_eq!(count_rows(&conn, "biblio_article_terms"), 0);
    assert_eq!(count_rows(&conn, "biblio_network_meta"), 0);
    assert_eq!(count_rows(&conn, "biblio_network_nodes"), 0);
    assert_eq!(count_rows(&conn, "biblio_network_edges"), 0);
}

#[test]
fn test_export_import_preserves_article_data() {
    let conn = setup_db();

    // Insert article with rich metadata
    conn.execute(
        "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors, publication_year, doi, journal, keywords, import_source, imported_at, changed_at)
         VALUES ('art-1', 1, 'included', 'Deep Learning for NLP', 'A comprehensive survey', '[\"Smith, J.\"]', 2024, '10.1234/test', 'Nature', '[\"deep-learning\"]', 'test.ris', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    ).expect("insert article");

    let json = export_project(&conn).expect("export");

    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import");

    // Verify article fields survived round-trip
    let (title, abstract_text, doi, journal, year): (String, String, String, String, i64) = conn2
        .query_row(
            "SELECT title, abstract_text, doi, journal, publication_year FROM articles WHERE id = 'art-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .expect("query article");

    assert_eq!(title, "Deep Learning for NLP");
    assert_eq!(abstract_text, "A comprehensive survey");
    assert_eq!(doi, "10.1234/test");
    assert_eq!(journal, "Nature");
    assert_eq!(year, 2024);
}
