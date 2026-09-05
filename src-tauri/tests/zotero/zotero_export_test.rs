//! Zotero export diff + preview tests (Tier 5). The DOI diff classifies the
//! scoped articles into missing / already-present / no-DOI; placeholder DOIs
//! are treated as absent. Binding inventory: `docs/test-plans/zotero-tests.md`.

use std::collections::HashSet;

use bango_lib::commands::zotero::export_zotero_preview_inner;
use bango_lib::models::article::{Article, ArticleStatus};
use bango_lib::zotero::export_mapping::{classify_export_articles, ExportArticleClass};

fn base_article() -> Article {
    Article {
        id: "a1".into(),
        sequence_id: 1,
        status: ArticleStatus::Working,
        screening_error: false,
        title: String::new(),
        abstract_text: String::new(),
        authors: Vec::new(),
        publication_year: None,
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: Vec::new(),
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        eissn: None,
        journal_index_id: None,
        reference_type: None,
        date: None,
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        user_notes: None,
        ris_extras: None,
        duplicate_of: None,
        ai_decision: None,
        ai_reasoning: None,
        ai_confidence: None,
        matched_inclusion_criteria: Vec::new(),
        matched_exclusion_criteria: Vec::new(),
        tags: Vec::new(),
        labels: Vec::new(),
        manual_override: false,
        import_source: None,
        imported_at: String::new(),
        changed_at: String::new(),
        screened_at: None,
        data_length: None,
        token_estimate: None,
        actual_tokens: None,
        full_text: None,
        full_text_ai_summary: None,
        num_cited: None,
        num_references: None,
        has_citation_details: false,
        has_reference_details: false,
        has_full_text: false,
        full_text_file_name: None,
        has_figures_or_tables: false,
        is_translated: false,
        translation_status: "none".into(),
        translation_error: None,
        translated_at: None,
    }
}

#[test]
fn diff_by_canonical_doi_classifies_articles() {
    let mut present_doi = base_article();
    present_doi.doi = Some("https://doi.org/10.1/PRESENT".into());
    let mut missing_doi = base_article();
    missing_doi.doi = Some("10.1/missing".into());
    let mut no_doi = base_article();
    no_doi.doi = None;

    // The collection already holds the PRESENT DOI in a different case/prefix
    // form - canonical comparison is case-insensitive.
    let mut collection_dois = HashSet::new();
    collection_dois.insert("10.1/present".to_string());

    let articles = [present_doi, missing_doi, no_doi];
    let classified = classify_export_articles(&articles, &collection_dois);
    assert_eq!(classified[0].1, ExportArticleClass::AlreadyPresent);
    assert_eq!(classified[1].1, ExportArticleClass::Missing);
    assert_eq!(classified[2].1, ExportArticleClass::NoDoi);
}

#[test]
fn diff_treats_placeholder_dois_as_no_doi() {
    for placeholder in ["NA", "N/A", "NULL", "-", "https://doi.org/NA"] {
        let mut article = base_article();
        article.doi = Some(placeholder.into());
        let articles = [article];
        let empty = HashSet::new();
        let classified = classify_export_articles(&articles, &empty);
        assert_eq!(
            classified[0].1,
            ExportArticleClass::NoDoi,
            "{placeholder} must classify as NoDoi (never matched, skipped + counted)"
        );
    }
}

#[tokio::test]
async fn preview_counts_match_diff() {
    use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
    use bango_lib::db::article_repo;
    use bango_lib::db::migration::run_migrations;
    use bango_lib::models::article::NewArticle;

    let tmp = tempfile::TempDir::new().unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    set_setting(&conn, STORAGE_ROOT_KEY, tmp.path().to_str()).unwrap();
    std::fs::create_dir_all(tmp.path().join("fulltext")).unwrap();

    // Three articles: one already in the collection (matching DOI), one
    // missing WITH an attachable full-text file, one missing without a DOI.
    let articles = vec![
        NewArticle {
            title: "Already Present".into(),
            abstract_text: "Abstract.".into(),
            authors: vec!["Author, A".into()],
            doi: Some("10.1/present".into()),
            ..NewArticle::default()
        },
        NewArticle {
            title: "Missing With File".into(),
            abstract_text: "Abstract.".into(),
            authors: vec!["Author, A".into()],
            doi: Some("10.1/missing-file".into()),
            ..NewArticle::default()
        },
        NewArticle {
            title: "No DOI".into(),
            abstract_text: "Abstract.".into(),
            authors: vec!["Author, A".into()],
            ..NewArticle::default()
        },
    ];
    let inserted = article_repo::insert_articles_batch(&conn, &articles, "test").unwrap();
    // Attach a full-text file to the missing article (has_full_text + file).
    std::fs::write(tmp.path().join("fulltext").join("missing-file.txt"), "body text").unwrap();
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text_file_name = 'missing-file.txt' WHERE id = ?1",
        [&inserted[1].id],
    )
    .unwrap();

    // Mockito: the collection's top items hold the PRESENT DOI.
    let mut server = mockito::Server::new_async().await;
    let top_items = r#"[{"key":"Z1","version":1,"meta":{},"data":{"itemType":"journalArticle","title":"Present in Zotero","DOI":"https://doi.org/10.1/PRESENT"}}]"#;
    server
        .mock("GET", "/api/users/0/collections/KEY/items/top")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Last-Modified-Version", "42")
        .with_body(top_items)
        .create_async()
        .await;

    let db = std::sync::Mutex::new(conn);
    let base = format!("{}/api", server.url());
    let preview =
        export_zotero_preview_inner(&base, &db, "KEY", "all", false).await.expect("preview");

    // The counts mirror the DOI diff exactly.
    assert_eq!(preview.total_articles, 3);
    assert_eq!(preview.already_present_count, 1);
    assert_eq!(preview.missing_count, 1);
    assert_eq!(preview.no_doi_count, 1);
    assert_eq!(preview.file_count, 1, "the missing article has an attachable .txt");
}

#[tokio::test]
async fn export_posts_child_notes_and_counts() {
    use bango_lib::commands::zotero::export_zotero_collection_core;
    use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
    use bango_lib::db::migration::run_migrations;

    let tmp = tempfile::TempDir::new().unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    set_setting(&conn, STORAGE_ROOT_KEY, tmp.path().to_str()).unwrap();

    // One article with merged-format user notes (two blocks) and a DOI.
    conn.execute(
        "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors, keywords, doi, user_notes)
         VALUES ('e-1', 1, 'working', 'Noted Article', 'x', '[]', '[]', '10.9/noted',
                 'First note
---
line two

Second note
---
body text')",
        [],
    )
    .unwrap();

    // A stored key bound to the live server id -> silent reuse (no dialog).
    let machine_key = bango_lib::crypto::aes_gcm::derive_key_from_machine();
    let encrypted = bango_lib::crypto::aes_gcm::encrypt(b"test-key", &machine_key).unwrap();
    set_setting(&conn, "zotero_api_key", Some(&encrypted)).unwrap();
    set_setting(&conn, "zotero_server_id", Some("SID1")).unwrap();

    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("X-Zotero-Version", "10.0.1")
        .with_header("Zotero-Server-ID", "SID1")
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"key":"KEY","version":1,"data":{"key":"KEY","version":1,"name":"Target","parentCollection":false}}]"#)
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/KEY/items/top")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;
    // Item batch vs note batch are distinguished by body content.
    server
        .mock("POST", "/api/users/0/items")
        .match_body(mockito::Matcher::Regex(String::from("\"itemType\":\"journalArticle\"")))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"successful":{},"success":{"0":"ITEM1"},"unchanged":{},"failed":{}}"#)
        .create_async()
        .await;
    server
        .mock("POST", "/api/users/0/items")
        .match_body(mockito::Matcher::Regex(String::from("\"itemType\":\"note\"")))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"successful":{},"success":{"0":"NOTE1","1":"NOTE2"},"unchanged":{},"failed":{}}"#,
        )
        .create_async()
        .await;

    let db = std::sync::Mutex::new(conn);
    let base = format!("{}/api", server.url());
    let result =
        export_zotero_collection_core(&base, &db, "KEY", "all", false, false, &|_, _, _, _| {})
            .await
            .expect("export");
    assert_eq!(result.exported_count, 1);
    // One Zotero child-note item per title/---/body block.
    assert_eq!(result.note_exported_count, 2);
    assert_eq!(result.note_failed_count, 0);
}
