//! Integration tests for project backup export/import round-trip.
//! Verifies all tables survive serialize → deserialize correctly.

use bango_lib::db::article_repo;
use bango_lib::db::audit_repo;
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
    // Biblio tables are NOT exported - they are dynamically regenerated by biblio_normalize.
    // The backup should emit them as empty arrays.
    assert_eq!(parsed["biblioAuthors"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioArticleAuthors"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioInstitutions"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioAuthorAffiliations"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioTerms"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioArticleTerms"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioNetworkMeta"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioNetworkNodes"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["biblioNetworkEdges"].as_array().unwrap().len(), 0);
    assert!(parsed["llmConfig"].is_object());

    // Import into a fresh database
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import_project failed");

    // Verify post-import counts: source tables should round-trip, but biblio tables
    // are not exported so they should be empty (regenerated later by biblio_normalize).
    let source_tables = [
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
    ];
    for table in &source_tables {
        let post_count = count_rows(&conn2, table);
        assert_eq!(post_count, 1, "Post-import: {} should have 1 row, got {}", table, post_count);
    }

    // Biblio tables are dynamically generated - they should be empty after import.
    let biblio_tables = [
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
    for table in &biblio_tables {
        let post_count = count_rows(&conn2, table);
        assert_eq!(
            post_count, 0,
            "Post-import: {} should be empty (not exported), got {}",
            table, post_count
        );
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

/// Plan-A originals archive must survive a backup/restore cycle (plan §5).
/// Regression for the data-loss gap where `export_project` did not serialize
/// `article_original_content` / `article_original_chunks`.
#[test]
fn export_import_preserves_translation_originals() {
    let conn = setup_db();
    seed_core_data(&conn);
    let article_id: String = conn
        .query_row("SELECT id FROM articles LIMIT 1", [], |row| row.get(0))
        .expect("get article id");

    // Seed original content + original chunks (simulating a completed Plan-A
    // translation that rewrote the working row to English).
    conn.execute(
        "INSERT INTO article_original_content \
         (article_id, original_title, original_abstract_text, original_full_text, \
         source_language, stored_at) \
         VALUES (?1, 'Titre français', 'Résumé français détaillé.', \
         'Corps de texte français complet.', 'French', '2026-01-01T00:00:00Z')",
        params![article_id],
    )
    .expect("seed original content");
    conn.execute(
        "INSERT INTO article_original_chunks \
         (id, article_id, chunk_index, section, content, word_count) \
         VALUES (1, ?1, 0, 'Methods', 'méthodes françaises ici', 3)",
        params![article_id],
    )
    .expect("seed original chunk");

    // Export + import into a fresh DB.
    let json = export_project(&conn).expect("export");
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import");

    // The original content row must round-trip.
    assert_eq!(count_rows(&conn2, "article_original_content"), 1, "original content must survive");
    let (orig_title, orig_lang): (String, String) = conn2
        .query_row(
            "SELECT original_title, source_language FROM article_original_content \
             WHERE article_id = ?1",
            params![article_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("query original content");
    assert_eq!(orig_title, "Titre français");
    assert_eq!(orig_lang, "French");

    // The original chunk must round-trip.
    assert_eq!(count_rows(&conn2, "article_original_chunks"), 1, "original chunks must survive");
    let chunk_content: String = conn2
        .query_row(
            "SELECT content FROM article_original_chunks WHERE article_id = ?1",
            params![article_id],
            |row| row.get(0),
        )
        .expect("query original chunk");
    assert_eq!(chunk_content, "méthodes françaises ici");
}

/// Tier 3 regression test: `import_project` must purge `article_chunks` rows.
/// Without the explicit `DELETE FROM article_chunks` in the purge sequence,
/// foreign_keys=OFF during import prevents the `ON DELETE CASCADE` on
/// `article_chunks.article_id REFERENCES articles(id)` from firing, leaving
/// orphaned chunk rows that survive the article-table wipe.
#[test]
fn import_project_clears_article_chunks() {
    // 1. Seed a DB with one article + chunk rows.
    let conn = setup_db();
    seed_core_data(&conn);
    let article_id: String = conn
        .query_row("SELECT id FROM articles LIMIT 1", [], |row| row.get(0))
        .expect("get article id");
    conn.execute(
        "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count)
         VALUES (?1, 0, 'Methods', 'sugar tax study design rct children', 6)",
        params![article_id],
    )
    .expect("seed chunk");
    conn.execute(
        "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count)
         VALUES (?1, 1, 'Results', 'effect size 0.45 ci 0.21 0.69', 7)",
        params![article_id],
    )
    .expect("seed chunk 2");
    assert_eq!(count_rows(&conn, "article_chunks"), 2, "pre: chunks seeded");

    // 2. Export (backup JSON deliberately excludes article_chunks - it is derived
    //    at attach time). Then import into a fresh DB that already has stale chunks.
    let json = export_project(&conn).expect("export");

    let conn2 = setup_db();
    // Seed conn2 with a *different* article + chunks so we verify the import
    // wipes pre-existing chunks (not just that it starts empty).
    let stale = article_repo::insert_article(&conn2, &new_article("Stale")).expect("insert stale");
    article_repo::move_to_working(&conn2, &stale.id).expect("move stale");
    conn2
        .execute(
            "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count)
             VALUES (?1, 0, 'Methods', 'stale chunk text here', 4)",
            params![stale.id],
        )
        .expect("seed stale chunk into conn2");
    assert_eq!(count_rows(&conn2, "article_chunks"), 1, "pre: stale chunk present");

    // 3. Import - should wipe article_chunks (orphaned chunks must not survive).
    import_project(&conn2, &json).expect("import");

    // 4. Assert no orphaned chunks remain.
    assert_eq!(
        count_rows(&conn2, "article_chunks"),
        0,
        "import_project must purge article_chunks (orphan-prevention)"
    );
    // And the imported article round-tripped.
    assert_eq!(count_rows(&conn2, "articles"), 1, "imported article present");
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

/// System-level audit entries (article_id = NULL, details = NULL) must survive
/// a backup/restore round-trip with NULLs intact. Regression test for the bug
/// where `get_str` returned "" for JSON null, corrupting NULL → empty string.
#[test]
fn export_import_preserves_null_audit_article_id() {
    let conn = setup_db();

    // Seed an article so the FK on article-bound entries doesn't fail.
    let a = article_repo::insert_article(&conn, &new_article("Test Article")).expect("insert");
    article_repo::move_to_working(&conn, &a.id).expect("move to working");

    // Create a system-level audit entry (article_id = NULL, details = NULL).
    audit_repo::log_error(&conn, "System error: LLM connection failed").expect("log_error");

    // Create an article-bound audit entry for comparison.
    audit_repo::create_entry(&conn, &a.id, "import", None, None, Some("Imported"), "system")
        .expect("create_entry");

    assert_eq!(count_rows(&conn, "audit_entries"), 2);

    // Export → import into a fresh DB.
    let json = export_project(&conn).expect("export");
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import");

    // Both rows must survive.
    assert_eq!(count_rows(&conn2, "audit_entries"), 2);

    // The system-level row must still have article_id IS NULL (not "").
    let null_count: i64 = conn2
        .query_row("SELECT COUNT(*) FROM audit_entries WHERE article_id IS NULL", [], |row| {
            row.get(0)
        })
        .expect("count null article_id");
    assert_eq!(null_count, 1, "system-level audit entry must preserve article_id = NULL");

    // The system-level row must also have details preserved.
    let details: String = conn2
        .query_row("SELECT details FROM audit_entries WHERE article_id IS NULL", [], |row| {
            row.get(0)
        })
        .expect("query system audit details");
    assert_eq!(details, "System error: LLM connection failed");

    // The article-bound row must have the correct article_id.
    let bound_id: String = conn2
        .query_row("SELECT article_id FROM audit_entries WHERE article_id IS NOT NULL", [], |row| {
            row.get(0)
        })
        .expect("query article-bound audit");
    assert_eq!(bound_id, a.id);
}

/// Project-portable `app_settings` (screening rules, summary mode,
/// auto-translate, screening-mode params) must survive a backup → restore
/// cycle. Machine-local state (storage root, premium flag, staleness flags)
/// must NOT be exported.
#[test]
fn export_import_round_trips_portable_app_settings() {
    use bango_lib::db::app_settings_repo;

    let conn = setup_db();
    seed_core_data(&conn);

    // Seed a portable setting (the new custom screening logic).
    let custom_logic = "Inclusion 1 AND 2 must match; then consider 3 OR 4.";
    app_settings_repo::set_screening_custom_logic(&conn, custom_logic)
        .expect("set screening_custom_logic");
    // Seed another portable setting (screening mode).
    app_settings_repo::set_screening_mode(&conn, app_settings_repo::ScreeningMode::Enhanced)
        .expect("set screening_mode");
    // Seed a machine-local setting that must NOT be exported.
    app_settings_repo::set_setting(&conn, "storage_root", Some("/tmp/machine-local-path"))
        .expect("set storage_root");

    // Export → parse JSON → verify portable settings are present + machine-local
    // settings are absent.
    let json = export_project(&conn).expect("export");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let settings = parsed["appSettings"].as_array().expect("appSettings array");
    let keys: Vec<&str> = settings.iter().filter_map(|s| s["key"].as_str()).collect();
    assert!(
        keys.contains(&"screening_custom_logic"),
        "portable setting screening_custom_logic must be exported: {keys:?}"
    );
    assert!(
        keys.contains(&"screening_mode"),
        "portable setting screening_mode must be exported: {keys:?}"
    );
    assert!(
        !keys.contains(&"storage_root"),
        "machine-local storage_root must NOT be exported: {keys:?}"
    );
    assert!(
        !keys.contains(&"flag_premium"),
        "machine-local flag_premium must NOT be exported: {keys:?}"
    );

    // Import into a fresh DB → verify the portable settings round-tripped.
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import");

    let restored_logic = app_settings_repo::get_screening_custom_logic(&conn2)
        .expect("get_screening_custom_logic")
        .expect("screening_custom_logic must be restored");
    assert_eq!(restored_logic, custom_logic, "screening_custom_logic must round-trip verbatim");

    let restored_mode = app_settings_repo::get_screening_mode(&conn2).expect("get_screening_mode");
    assert_eq!(
        restored_mode,
        app_settings_repo::ScreeningMode::Enhanced,
        "screening_mode must round-trip"
    );

    // The fresh DB's storage_root must NOT have been clobbered by the backup's
    // value (which was deliberately excluded). It should resolve to the
    // platform default (or whatever the fresh DB computed), not /tmp/...
    let restored_root = app_settings_repo::get_storage_root(&conn2).expect("get_storage_root");
    assert_ne!(
        restored_root, "/tmp/machine-local-path",
        "machine-local storage_root must NOT be imported (would clobber target machine's path)"
    );
}

/// An old backup without an `appSettings` field must import cleanly
/// (`#[serde(default)]`). Regression for backward-compat with pre-feature
/// backups.
#[test]
fn import_old_backup_without_app_settings_field() {
    use bango_lib::db::app_settings_repo;

    let conn = setup_db();
    let old_backup = r#"{
        "metadata": {
            "specVersion": "3.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "appName": "Bango",
            "appVersion": "2.5.6"
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

    // No app_settings were in the backup → the target machine's values are
    // untouched (absent → defaults).
    assert_eq!(
        app_settings_repo::get_screening_custom_logic(&conn).expect("get"),
        None,
        "absent setting in old backup must not set anything"
    );
}

/// A hand-edited backup that adds a non-allowlisted key to `appSettings`
/// must NOT be imported (defense-in-depth against the allowlist being
/// bypassed by manual editing).
#[test]
fn import_ignores_non_allowlisted_app_settings() {
    use bango_lib::db::app_settings_repo;

    let conn = setup_db();
    let malicious_backup = r#"{
        "metadata": {
            "specVersion": "3.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "appName": "Bango",
            "appVersion": "2.5.6"
        },
        "researchAims": [],
        "criteria": [],
        "articles": [],
        "tags": [],
        "labels": [],
        "articleTags": [],
        "articleLabels": [],
        "auditEntries": [],
        "llmConfig": null,
        "appSettings": [
            {"key": "storage_root", "value": "/tmp/attacker-path"},
            {"key": "flag_premium", "value": "true"}
        ]
    }"#;

    import_project(&conn, malicious_backup).expect("import should succeed (ignoring bad keys)");

    // storage_root must NOT have been overwritten by the malicious value.
    let root = app_settings_repo::get_storage_root(&conn).expect("get_storage_root");
    assert_ne!(root, "/tmp/attacker-path", "non-allowlisted storage_root must be ignored");
    // flag_premium must NOT have been flipped to true.
    let premium = app_settings_repo::get_setting(&conn, "flag_premium").expect("get_setting");
    assert_ne!(premium.as_deref(), Some("true"), "non-allowlisted flag_premium must be ignored");
}

/// Genuine orphan audit entries (article_id references a non-existent
/// article) must be dropped on export so they don't propagate into backups
/// and crash the import path on FK-bound tables. Runtime deletes already
/// cascade via ON DELETE CASCADE; this test covers the defense-in-depth case
/// where an orphan was inserted while foreign_keys were OFF (e.g. an older
/// non-transactional import). Covers the export filter in `export_project`.
#[test]
fn export_drops_genuine_orphan_audit_entry() {
    let conn = setup_db();

    // Seed one article + a bound audit entry (the legitimate case).
    let a = article_repo::insert_article(&conn, &new_article("Survivor")).expect("insert");
    article_repo::move_to_working(&conn, &a.id).expect("move to working");
    audit_repo::create_entry(&conn, &a.id, "import", None, None, Some("ok"), "system")
        .expect("create bound audit");

    // Simulate an orphan: disable FK, insert a row pointing at a ghost article,
    // then re-enable FK. This mirrors how orphans historically entered the DB
    // (imports ran with foreign_keys=OFF before the explicit purge sequence
    // existed; a malformed backup could insert a row referencing an absent id).
    conn.execute("PRAGMA foreign_keys = OFF", []).expect("disable fk");
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
         VALUES ('orphan-1', 'ghost-article-id', '2026-01-01T00:00:00Z', 'import', 'orphan', 'system')",
        [],
    )
    .expect("insert orphan");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    // Pre-condition: the orphan row exists.
    let orphan_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_entries WHERE id = 'orphan-1'", [], |row| row.get(0))
        .expect("count orphans");
    assert_eq!(orphan_count, 1, "pre: orphan must exist");

    // Export.
    let json = export_project(&conn).expect("export");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");

    // The orphan must be absent from the backup JSON.
    let orphan_in_backup = parsed["auditEntries"]
        .as_array()
        .expect("auditEntries array")
        .iter()
        .any(|e| e["id"] == "orphan-1");
    assert!(!orphan_in_backup, "genuine orphan audit entry must be dropped on export");

    // The legitimate bound entry must survive.
    let bound_in_backup = parsed["auditEntries"]
        .as_array()
        .expect("auditEntries array")
        .iter()
        .any(|e| e["articleId"] == a.id);
    assert!(bound_in_backup, "legitimate article-bound audit entry must survive export");
}

/// System-level audit entries (article_id IS NULL, e.g. errors, search
/// strategies) AND historical empty-string rows (article_id = '', from older
/// write paths) must BOTH survive export. The export filter preserves both
/// shapes; dropping either would silently delete legitimate audit-trail
/// history. Covers the export filter in `export_project`.
#[test]
fn export_preserves_null_and_empty_string_system_entries() {
    let conn = setup_db();

    // Seed an article so the table is non-empty (keeps the test realistic).
    let a = article_repo::insert_article(&conn, &new_article("Context")).expect("insert");
    article_repo::move_to_working(&conn, &a.id).expect("move to working");

    // System-level entry written the modern way: article_id = NULL.
    audit_repo::log_error(&conn, "modern system error").expect("log_error");

    // System-level entry written the historical way: article_id = '' (the
    // malformed shape found in old backups + the shipped demo project before
    // this fix). Inserted with FK off to bypass the constraint.
    conn.execute("PRAGMA foreign_keys = OFF", []).expect("disable fk");
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
         VALUES ('legacy-empty-1', '', '2026-01-01T00:00:00Z', 'error', 'legacy system error', 'system')",
        [],
    )
    .expect("insert legacy empty-string entry");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    // Export.
    let json = export_project(&conn).expect("export");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let entries = parsed["auditEntries"].as_array().expect("auditEntries array");

    // Both system entries must be present.
    let has_null_entry = entries
        .iter()
        .any(|e| e["id"] != "legacy-empty-1" && e["articleId"].is_null() && e["action"] == "error");
    assert!(has_null_entry, "NULL article_id system entry must survive export");

    let has_empty_entry = entries.iter().any(|e| e["id"] == "legacy-empty-1");
    assert!(has_empty_entry, "empty-string article_id system entry must survive export");
}

/// Historical backups (and the pre-fix demo project) carry system-level audit
/// entries with `"articleId": ""` (empty string) instead of the correct `null`.
/// On import, these must be normalized to SQL NULL so they don't violate the
/// `FOREIGN KEY (article_id) REFERENCES articles(id)` constraint on the
/// v006-rebuilt table. The row is preserved as a system-level entry, never
/// silently dropped. Covers the import normalization in `import_project`.
#[test]
fn import_normalizes_empty_string_article_id_to_null() {
    let conn = setup_db();

    // Craft a minimal backup with one system-level entry carrying articleId = "".
    let backup = r#"{
        "metadata": {
            "specVersion": "3.0",
            "exportedAt": "2026-01-01T00:00:00Z",
            "appName": "Bango",
            "appVersion": "2.5.6"
        },
        "researchAims": [],
        "criteria": [],
        "articles": [],
        "tags": [],
        "labels": [],
        "articleTags": [],
        "articleLabels": [],
        "auditEntries": [
            {
                "id": "sys-empty-1",
                "articleId": "",
                "timestamp": "2026-01-01T00:00:00Z",
                "action": "error",
                "details": "legacy system error",
                "source": "system"
            }
        ],
        "llmConfig": null
    }"#;

    import_project(&conn, backup).expect("import should succeed");

    // The entry must survive (not dropped) and be normalized to NULL.
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM audit_entries WHERE id = 'sys-empty-1'", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(row_count, 1, "system entry must be preserved (not silently dropped)");

    let is_null: bool = conn
        .query_row(
            "SELECT article_id IS NULL FROM audit_entries WHERE id = 'sys-empty-1'",
            [],
            |row| row.get(0),
        )
        .expect("check null");
    assert!(is_null, "empty-string articleId must be normalized to NULL on import");

    // The details must survive too (round-trip integrity).
    let details: String = conn
        .query_row("SELECT details FROM audit_entries WHERE id = 'sys-empty-1'", [], |row| {
            row.get(0)
        })
        .expect("query details");
    assert_eq!(details, "legacy system error");
}

/// Audit entries with NULL details (e.g. dedup_flag with no details) must
/// survive a round-trip with details = NULL, not empty string.
#[test]
fn export_import_preserves_null_audit_details() {
    let conn = setup_db();

    let a = article_repo::insert_article(&conn, &new_article("Test Article")).expect("insert");
    article_repo::move_to_working(&conn, &a.id).expect("move to working");

    // Create an audit entry with details = NULL.
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, details, source) \
         VALUES (?1, ?2, ?3, 'dedup_flag', NULL, 'user')",
        rusqlite::params![id, a.id, now],
    )
    .expect("insert null-details audit");

    // Verify pre-condition.
    let is_null: bool = conn
        .query_row(
            "SELECT details IS NULL FROM audit_entries WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .expect("check details null");
    assert!(is_null, "pre: details must be NULL");

    // Export → import.
    let json = export_project(&conn).expect("export");
    let conn2 = setup_db();
    import_project(&conn2, &json).expect("import");

    // Details must still be NULL.
    let is_null: bool = conn2
        .query_row(
            "SELECT details IS NULL FROM audit_entries WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .expect("check details null after import");
    assert!(is_null, "post: details must remain NULL after round-trip");
}
