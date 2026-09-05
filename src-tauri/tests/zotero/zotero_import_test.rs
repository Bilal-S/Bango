//! Zotero import pipeline tests (Tier 3): the canonical import sequence
//! (insert -> classify -> journal links -> tags -> staleness), key-based
//! exclusion, the library-version guard, the capacity guard, and the
//! attachment phase (attach, non-fatal failure, duplicate skip).
//! Binding inventory: `docs/test-plans/zotero-tests.md`.

use std::sync::Mutex;

use bango_lib::commands::zotero::{
    import_zotero_collection_core, import_zotero_db_phase, ZoteroImportDbParams,
};
use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
use bango_lib::db::migration::run_migrations;
use bango_lib::ris::types::RisRecord;
use tempfile::TempDir;

fn test_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn configure_storage_root(conn: &rusqlite::Connection, root: &std::path::Path) {
    set_setting(conn, STORAGE_ROOT_KEY, root.to_str()).unwrap();
    std::fs::create_dir_all(root.join("fulltext")).unwrap();
}

fn ris_record(title: &str) -> RisRecord {
    RisRecord {
        reference_type: Some("JOUR".to_string()),
        title: Some(title.to_string()),
        abstract_text: Some("An abstract.".to_string()),
        authors: vec!["Author, A".to_string()],
        publication_year: Some(2020),
        doi: Some(format!("10.1/{}", title.to_lowercase().replace(' ', "-"))),
        ..RisRecord::default()
    }
}

fn valid_item_json(key: &str, title: &str, tags: &[&str]) -> String {
    let tags_json: Vec<String> = tags.iter().map(|t| format!(r#"{{"tag":"{t}"}}"#)).collect();
    format!(
        r#"{{"key":"{key}","version":1,"meta":{{"parsedDate":"2020-01-01"}},"data":{{"itemType":"journalArticle","title":"{title}","abstractNote":"An abstract.","creators":[{{"creatorType":"author","firstName":"A","lastName":"Author"}}],"DOI":"10.1/{key}","tags":[{}]}}}}"#,
        tags_json.join(",")
    )
}

fn attachment_json(key: &str, parent: &str, content_type: &str, filename: &str) -> String {
    format!(
        r#"{{"key":"{key}","version":1,"data":{{"itemType":"attachment","linkMode":"imported_file","contentType":"{content_type}","filename":"{filename}","parentItem":"{parent}"}}}}"#
    )
}

/// Mock the collection fetch requests (items + subcollections + bulk
/// attachments) and return (server, api base).
async fn import_server(items: String, attachments: String) -> (mockito::ServerGuard, String) {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/users/0/collections/KEY/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Last-Modified-Version", "15")
        .with_body(items)
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/KEY/collections")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(attachments)
        .create_async()
        .await;
    let base = format!("{}/api", server.url());
    (server, base)
}

async fn run_core(
    db: &Mutex<rusqlite::Connection>,
    base: &str,
    excluded: &[String],
    expected_version: i64,
) -> Result<bango_lib::commands::zotero::ZoteroImportResult, bango_lib::error::AppError> {
    run_core_with(db, base, excluded, expected_version, false).await
}

/// `run_core` with an explicit `skip_duplicates` (the review-step checkbox).
async fn run_core_with(
    db: &Mutex<rusqlite::Connection>,
    base: &str,
    excluded: &[String],
    expected_version: i64,
    skip_duplicates: bool,
) -> Result<bango_lib::commands::zotero::ZoteroImportResult, bango_lib::error::AppError> {
    import_zotero_collection_core(
        base,
        db,
        "KEY",
        excluded,
        expected_version,
        skip_duplicates,
        &|_, _, _, _| {},
        &|_, _| {},
    )
    .await
}

#[test]
fn import_zotero_collection_inserts_articles() {
    let conn = test_db();
    let records = vec![ris_record("Alpha Paper"), ris_record("Beta Paper")];
    let keys = vec!["ITEM1".to_string(), "ITEM2".to_string()];
    let result = import_zotero_db_phase(
        &conn,
        ZoteroImportDbParams {
            records: &records,
            keys: &keys,
            skipped_by_user: 0,
            skipped_validation: 0,
            skip_duplicates: false,
            validation_errors: vec![],
            error_groups: vec![],
            tags_by_key: &Default::default(),
        },
    )
    .expect("db phase");
    assert_eq!(result.import_payload.imported_count, 2);
    // import_source + per-article 'import' audit entries inherited.
    let sourced: i64 = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE import_source = 'zotero'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sourced, 2);
    let audited: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE action = 'import' AND details = 'Imported from zotero'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audited, 2);
    // Article/key/status triples align with the inserted rows.
    assert_eq!(result.article_key_status.len(), 2);
    assert_eq!(result.article_key_status[0].1, "ITEM1");
}

#[test]
fn import_zotero_collection_runs_classify() {
    let conn = test_db();
    // Two identical records: the first moves to working, the second stays a
    // duplicate (existing classify behavior).
    let records = vec![ris_record("Same Title"), ris_record("Same Title")];
    // Same DOI -> identical records (dedup engine compares title/authors/year/doi).
    let mut second = ris_record("Same Title");
    second.doi = records[0].doi.clone();
    let records = vec![records[0].clone(), second];
    let keys = vec!["ITEM1".to_string(), "ITEM2".to_string()];
    let result = import_zotero_db_phase(
        &conn,
        ZoteroImportDbParams {
            records: &records,
            keys: &keys,
            skipped_by_user: 0,
            skipped_validation: 0,
            skip_duplicates: false,
            validation_errors: vec![],
            error_groups: vec![],
            tags_by_key: &Default::default(),
        },
    )
    .expect("db phase");
    let statuses: Vec<&str> =
        result.article_key_status.iter().map(|(_, _, s)| s.as_str()).collect();
    assert!(statuses.contains(&"working"), "one article moves to working: {statuses:?}");
    assert!(statuses.contains(&"duplicate"), "the twin stays duplicate: {statuses:?}");
}

#[test]
fn import_zotero_collection_assigns_tags() {
    let conn = test_db();
    let records = vec![ris_record("Alpha Paper")];
    let keys = vec!["ITEM1".to_string()];
    let mut tags_by_key = std::collections::HashMap::new();
    tags_by_key
        .insert("ITEM1".to_string(), vec!["machine-learning".to_string(), "physics".to_string()]);
    let result = import_zotero_db_phase(
        &conn,
        ZoteroImportDbParams {
            records: &records,
            keys: &keys,
            skipped_by_user: 0,
            skipped_validation: 0,
            skip_duplicates: false,
            validation_errors: vec![],
            error_groups: vec![],
            tags_by_key: &tags_by_key,
        },
    )
    .expect("db phase");
    // Tags created with the ris_keyword source (Zotero tags are bibliographic
    // keywords, not user- or AI-created).
    let sources: Vec<String> = conn
        .prepare("SELECT name FROM tags WHERE source = 'ris_keyword' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(sources, vec!["machine-learning", "physics"]);
    let article_id = &result.article_key_status[0].0;
    let links: i64 = conn
        .query_row("SELECT COUNT(*) FROM article_tags WHERE article_id = ?1", [article_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(links, 2);
    // One representation: keywords stays empty for Zotero imports.
    let keywords: String = conn
        .query_row("SELECT keywords FROM articles WHERE id = ?1", [article_id], |r| r.get(0))
        .unwrap();
    assert_eq!(keywords, "[]");
    // changed_at bumped (bulk-tag pattern).
    let changed: Option<String> = conn
        .query_row("SELECT changed_at FROM articles WHERE id = ?1", [article_id], |r| r.get(0))
        .unwrap();
    assert!(changed.is_some(), "changed_at must be set after tag linking");
}

#[tokio::test]
async fn import_zotero_collection_respects_excluded_keys() {
    let conn = test_db();
    let db = Mutex::new(conn);
    let items = format!(
        "[{},{}]",
        valid_item_json("ITEM1", "Alpha Paper", &[]),
        valid_item_json("ITEM2", "Beta Paper", &[])
    );
    let (_server, base) = import_server(items, "[]".to_string()).await;
    let excluded = vec!["ITEM2".to_string(), "UNKNOWN".to_string()];
    let result = run_core(&db, &base, &excluded, 15).await.expect("import");
    assert_eq!(result.result.imported_count, 1, "only the non-excluded item imports");
    assert_eq!(result.result.skipped_by_user, 1, "unknown keys are ignored");
    let titles: Vec<String> = {
        let conn = db.lock().unwrap();
        let collected: Vec<String> = conn
            .prepare("SELECT title FROM articles ORDER BY title")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        collected
    };
    assert_eq!(titles, vec!["Alpha Paper".to_string()]);
}

#[tokio::test]
async fn import_zotero_collection_skips_library_duplicates() {
    let conn = test_db();
    // Seed the library with an article sharing ITEM1's canonical DOI; with
    // the review-step Skip flag on, only the other item imports.
    let seeded = vec![bango_lib::models::article::NewArticle {
        title: "Already In Library".to_string(),
        abstract_text: "Abstract.".to_string(),
        authors: vec!["Author, A".to_string()],
        doi: Some("10.1/ITEM1".to_string()),
        ..bango_lib::models::article::NewArticle::default()
    }];
    bango_lib::db::article_repo::insert_articles_batch(&conn, &seeded, "test").unwrap();
    let db = Mutex::new(conn);
    let items = format!(
        "[{},{}]",
        valid_item_json("ITEM1", "Alpha Paper", &[]),
        valid_item_json("ITEM2", "Beta Paper", &[])
    );
    let (_server, base) = import_server(items, "[]".to_string()).await;
    let result = run_core_with(&db, &base, &[], 15, true).await.expect("import");
    assert_eq!(result.result.imported_count, 1, "the library duplicate is not inserted");
    assert_eq!(result.result.skipped_duplicates, 1);
    let titles: Vec<String> = {
        let conn = db.lock().unwrap();
        let collected: Vec<String> = conn
            .prepare("SELECT title FROM articles ORDER BY title")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        collected
    };
    assert_eq!(titles, vec!["Already In Library".to_string(), "Beta Paper".to_string()]);
}

#[tokio::test]
async fn import_zotero_collection_aborts_on_library_version_change() {
    let conn = test_db();
    let db = Mutex::new(conn);
    let items = format!("[{}]", valid_item_json("ITEM1", "Alpha Paper", &[]));
    let (_server, base) = import_server(items, "[]".to_string()).await;
    // The mock reports Last-Modified-Version 15; a stale expectation aborts.
    let err = match run_core(&db, &base, &[], 14).await {
        Err(e) => e,
        Ok(_) => panic!("guard must fire"),
    };
    assert!(err.to_string().contains("changed since the preview"), "got: {err}");
    let count: i64 =
        db.lock().unwrap().query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "nothing is written when the guard fires");
}

#[tokio::test]
async fn import_zotero_collection_capacity_guard_surfaces() {
    let conn = test_db();
    // Fill the library to 9,999 of the 10,000 slots with fast raw inserts
    // (no per-article audit/read overhead), then import 2 items: the guard
    // inherited from insert_articles_batch must surface its error.
    {
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..9_999 {
            tx.execute(
                "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors, keywords)
                 VALUES (?1, ?2, 'working', ?3, 'x', '[]', '[]')",
                rusqlite::params![format!("fill-{i}"), (i + 1) as i64, format!("Filler {i}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let db = Mutex::new(conn);
    let items = format!(
        "[{},{}]",
        valid_item_json("ITEM1", "Alpha Paper", &[]),
        valid_item_json("ITEM2", "Beta Paper", &[])
    );
    let (_server, base) = import_server(items, "[]".to_string()).await;
    let err = match run_core(&db, &base, &[], 15).await {
        Err(e) => e,
        Ok(_) => panic!("capacity guard must fire"),
    };
    assert!(err.to_string().contains("slots remain"), "guard surfaces: {err}");
    let count: i64 =
        db.lock().unwrap().query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 9_999, "no partial import");
}

#[tokio::test]
async fn import_zotero_collection_attaches_pdf() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let db = Mutex::new(conn);

    // A real .txt full text on disk (txt exercises the attach path without a
    // binary PDF fixture); the 302 Location points at it.
    let txt = tmp.path().join("paper.txt");
    std::fs::write(&txt, "Introduction.\n\nBody text with content for chunking.").unwrap();

    let items = format!("[{}]", valid_item_json("ITEM1", "Alpha Paper", &[]));
    let attachments =
        format!("[{}]", attachment_json("ATT1", "ITEM1", "application/pdf", "paper.txt"));
    let (mut server, base) = import_server(items, attachments).await;
    server
        .mock("GET", "/api/users/0/items/ATT1/file")
        .match_query(mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", &format!("file://{}", txt.display()))
        .create_async()
        .await;

    let result = run_core(&db, &base, &[], 15).await.expect("import");
    assert_eq!(result.attached_count, 1, "attachment attached");
    assert_eq!(result.attachment_failed_count, 0);
    let (has_full_text,): (bool,) = {
        let conn = db.lock().unwrap();
        conn.query_row("SELECT has_full_text FROM articles", [], |r| Ok((r.get(0)?,))).unwrap()
    };
    assert!(has_full_text, "full text attached via attach_full_text_inner");
}

#[tokio::test]
async fn import_zotero_collection_attachment_failure_non_fatal() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let db = Mutex::new(conn);

    let items = format!("[{}]", valid_item_json("ITEM1", "Alpha Paper", &[]));
    let attachments =
        format!("[{}]", attachment_json("ATT1", "ITEM1", "application/pdf", "missing.pdf"));
    let (mut server, base) = import_server(items, attachments).await;
    // 302 to a file that does not exist: attach fails, the import survives.
    server
        .mock("GET", "/api/users/0/items/ATT1/file")
        .match_query(mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", "file:///nonexistent/zotero/missing.pdf")
        .create_async()
        .await;

    let result =
        run_core(&db, &base, &[], 15).await.expect("import succeeds despite attach failure");
    assert_eq!(result.result.imported_count, 1);
    assert_eq!(result.attached_count, 0);
    assert_eq!(result.attachment_failed_count, 1);
    // Per-article audit error (OpenAlex pattern) so it surfaces in the
    // article's Audit Timeline, not only generic Diagnostics.
    let audit_errors: i64 = {
        let conn = db.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE action = 'error' AND details LIKE 'Zotero attachment%'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(audit_errors, 1);
}

#[tokio::test]
async fn import_zotero_collection_duplicate_skips_attachment() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let db = Mutex::new(conn);

    // Two identical items (same DOI): the second is classified duplicate and
    // its attachment must never be fetched.
    let item1 = valid_item_json("ITEM1", "Same Title", &[]).replace("10.1/ITEM1", "10.1/same");
    let item2 = valid_item_json("ITEM2", "Same Title", &[]).replace("10.1/ITEM2", "10.1/same");
    // Equal titles sort stably: ITEM1 stays first -> working, ITEM2 duplicate.
    let items = format!("[{item1},{item2}]");
    let attachments =
        format!("[{}]", attachment_json("ATT_DUP", "ITEM2", "application/pdf", "dup.pdf"));
    let (mut server, base) = import_server(items, attachments).await;
    let file_mock = server
        .mock("GET", "/api/users/0/items/ATT_DUP/file")
        .match_query(mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", "file:///nonexistent/dup.pdf")
        .expect(0)
        .create_async()
        .await;

    let result = run_core(&db, &base, &[], 15).await.expect("import");
    assert_eq!(result.result.imported_count, 2);
    assert_eq!(result.attached_count, 0, "duplicates are skipped by the attachment phase");
    assert_eq!(result.attachment_failed_count, 0);
    file_mock.assert_async().await;
}

#[tokio::test]
async fn import_zotero_collection_counts_skipped_validation() {
    let conn = test_db();
    let db = Mutex::new(conn);
    // One valid, one missing abstract, one unsupported (webpage) item.
    let missing_abstract = r#"{"key":"NOABS","version":1,"meta":{},"data":{"itemType":"journalArticle","title":"No Abstract","creators":[{"creatorType":"author","firstName":"A","lastName":"Author"}]}}"#;
    let webpage = r#"{"key":"WEB1","version":1,"meta":{},"data":{"itemType":"webpage","title":"A Page","abstractNote":"x","creators":[{"creatorType":"author","name":"N"}]}}"#;
    let items = format!(
        "[{},{},{}]",
        valid_item_json("ITEM1", "Alpha Paper", &[]),
        missing_abstract,
        webpage
    );
    let (_server, base) = import_server(items, "[]".to_string()).await;
    let result = run_core(&db, &base, &[], 15).await.expect("import");
    // skipped_count mirrors RIS accounting: unsupported + missing-field records.
    assert_eq!(result.result.imported_count, 1);
    assert_eq!(result.result.skipped_count, 2, "unsupported + missing-abstract");
}

#[tokio::test]
async fn import_zotero_collection_url_only_attachment_skips() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let db = Mutex::new(conn);

    let items = format!("[{}]", valid_item_json("ITEM1", "Alpha Paper", &[]));
    // A pdf candidate whose file endpoint 302s to an http:// URL (a
    // URL-resident attachment): expected skip, not a failure.
    let attachments =
        format!("[{}]", attachment_json("ATT1", "ITEM1", "application/pdf", "paper.pdf"));
    let (mut server, base) = import_server(items, attachments).await;
    server
        .mock("GET", "/api/users/0/items/ATT1/file")
        .match_query(mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", "https://example.com/snapshot.html")
        .create_async()
        .await;

    let result = run_core(&db, &base, &[], 15).await.expect("import");
    assert_eq!(result.attached_count, 0);
    assert_eq!(result.attachment_failed_count, 0, "URL-only is not a failure");
    assert_eq!(result.attachment_skipped_count, 1);
    // No per-article audit error for an expected skip.
    let audit_errors: i64 = {
        let conn = db.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE action = 'error' AND details LIKE 'Zotero attachment%'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(audit_errors, 0);
}

#[tokio::test]
async fn import_zotero_collection_counts_non_candidate_siblings() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let db = Mutex::new(conn);

    let txt = tmp.path().join("paper.txt");
    std::fs::write(&txt, "Introduction.\n\nBody text with content for chunking.").unwrap();

    // A pdf candidate PLUS an epub sibling: attached 1, skipped 1.
    let items = format!("[{}]", valid_item_json("ITEM1", "Alpha Paper", &[]));
    let attachments = format!(
        "[{},{}]",
        attachment_json("ATT1", "ITEM1", "application/pdf", "paper.txt"),
        attachment_json("ATT2", "ITEM1", "application/epub+zip", "book.epub")
    );
    let (mut server, base) = import_server(items, attachments).await;
    server
        .mock("GET", "/api/users/0/items/ATT1/file")
        .match_query(mockito::Matcher::Any)
        .with_status(302)
        .with_header("Location", &format!("file://{}", txt.display()))
        .create_async()
        .await;

    let result = run_core(&db, &base, &[], 15).await.expect("import");
    assert_eq!(result.attached_count, 1);
    assert_eq!(result.attachment_failed_count, 0);
    assert_eq!(result.attachment_skipped_count, 1, "the epub sibling counts");
}
