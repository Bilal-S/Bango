//! Zotero read-command tests (Tier 2): connection status mapping,
//! collections listing, and the collection preview - all against a mockito
//! server standing in for the Zotero local API.
//! Binding inventory: `docs/test-plans/zotero-tests.md`.

use bango_lib::commands::zotero::{
    check_connection_inner, get_collection_preview_inner, get_collections_inner,
};

fn api_base(server: &mockito::Server) -> String {
    format!("{}/api", server.url())
}

/// In-memory DB for the preview's library-duplicate check (empty library).
fn preview_db() -> std::sync::Mutex<rusqlite::Connection> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    std::sync::Mutex::new(conn)
}

fn valid_item(key: &str, title: &str) -> String {
    format!(
        r#"{{"key":"{key}","version":1,"meta":{{"parsedDate":"2020-01-01"}},"data":{{"itemType":"journalArticle","title":"{title}","abstractNote":"An abstract.","creators":[{{"creatorType":"author","firstName":"A","lastName":"Author"}}],"tags":[{{"tag":"Physics"}}]}}}}"#
    )
}

fn attachment(key: &str, parent: &str, filename: &str) -> String {
    format!(
        r#"{{"key":"{key}","version":1,"data":{{"itemType":"attachment","linkMode":"imported_url","contentType":"application/pdf","filename":"{filename}","parentItem":"{parent}"}}}}"#
    )
}

/// Mock the three requests `get_collection_preview_inner` makes (items,
/// subcollections, bulk attachments) and return the mock base URL.
async fn preview_server(
    items_body: String,
    attachments_body: String,
) -> (mockito::ServerGuard, String) {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/users/0/collections/KEY/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Last-Modified-Version", "15")
        .with_body(items_body)
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
        .with_body(attachments_body)
        .create_async()
        .await;
    let base = api_base(&server);
    (server, base)
}

#[tokio::test]
async fn check_connection_ok() {
    // The mock is registered at "/api/" (slashed). The probe MUST send the
    // slashed form: a slashless GET /api answers the connector server's
    // "404 No endpoint found" fallback on live Zotero (regression coverage
    // for the trailing-slash probe fix).
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/")
        .with_status(200)
        .with_header("Zotero-API-Version", "3")
        .with_body("Nothing to see here.")
        .create_async()
        .await;
    let status = check_connection_inner(&api_base(&server)).await;
    assert_eq!(status.status, "ok");
    assert_eq!(status.api_version.as_deref(), Some("3"));
    assert!(status.hint.is_none());
}

#[tokio::test]
async fn check_connection_not_running() {
    // Nothing listens on port 1: connection refused -> not_running.
    let status = check_connection_inner("http://127.0.0.1:1/api").await;
    assert_eq!(status.status, "not_running");
    assert!(status.hint.is_some());
}

#[tokio::test]
async fn check_connection_api_disabled() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/api/").with_status(403).with_body("Forbidden").create_async().await;
    let status = check_connection_inner(&api_base(&server)).await;
    assert_eq!(status.status, "api_disabled");
    let hint = status.hint.expect("hint present");
    assert!(hint.contains("Settings -> Advanced"), "hint names the preference path: {hint}");
    assert!(hint.contains("Allow other applications"));
}

#[tokio::test]
async fn check_connection_unexpected_status_falls_back_to_error() {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/api/").with_status(500).with_body("internal error").create_async().await;
    let status = check_connection_inner(&api_base(&server)).await;
    assert_eq!(status.status, "error");
    let hint = status.hint.expect("hint carries the detail");
    assert!(hint.contains("500"), "status code surfaced: {hint}");
    assert!(hint.contains("internal error"), "body snippet surfaced: {hint}");
}

#[tokio::test]
async fn get_collections_returns_flat_tree() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"[
        {"key":"PARENT","version":1,"meta":{},"data":{"key":"PARENT","version":1,"name":"Super Collection","parentCollection":false}},
        {"key":"CHILD","version":1,"meta":{},"data":{"key":"CHILD","version":1,"name":"More Stuff","parentCollection":"PARENT"}},
        {"key":"ORPHAN","version":1,"meta":{},"data":{"key":"ORPHAN","version":1,"name":"No Parent Field"}}
    ]"#;
    server
        .mock("GET", "/api/users/0/collections")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;
    let collections = get_collections_inner(&api_base(&server)).await.expect("collections fetch");
    assert_eq!(collections.len(), 3);
    let parent = collections.iter().find(|c| c.key == "PARENT").expect("parent");
    assert_eq!(parent.name, "Super Collection");
    assert_eq!(parent.parent_key, None, "parentCollection: false -> null");
    let child = collections.iter().find(|c| c.key == "CHILD").expect("child");
    assert_eq!(child.parent_key.as_deref(), Some("PARENT"));
    let orphan = collections.iter().find(|c| c.key == "ORPHAN").expect("orphan");
    assert_eq!(orphan.parent_key, None, "absent parentCollection -> null");
}

#[tokio::test]
async fn get_collection_preview_counts_articles() {
    let items =
        format!("[{},{}]", valid_item("ITEM1", "Alpha Paper"), valid_item("ITEM2", "Beta Paper"));
    let attachments = format!("[{}]", attachment("ATT1", "ITEM1", "alpha.pdf"));
    let (_server, base) = preview_server(items, attachments).await;
    let db = preview_db();
    let preview = get_collection_preview_inner(&base, &db, "KEY").await.expect("preview");
    assert_eq!(preview.total_items, 2);
    assert_eq!(preview.mapped_articles, 2);
    assert_eq!(preview.preview.total_records, 2);
    assert_eq!(preview.preview.valid_records, 2);
    assert_eq!(preview.preview.error_count, 0);
    // One of the two items has a pdf child.
    assert_eq!(preview.attachment_count, 1);
    // Both items carry the same tag -> one distinct sanitized tag.
    assert_eq!(preview.tag_count, 1);
    assert_eq!(preview.preview.preview_articles.len(), 2);
}

#[tokio::test]
async fn get_collection_preview_validates_like_ris() {
    let missing_abstract = r#"{"key":"NOABS","version":1,"meta":{},"data":{"itemType":"journalArticle","title":"No Abstract","creators":[{"creatorType":"author","firstName":"A","lastName":"Author"}]}}"#;
    let items = format!("[{},{}]", valid_item("ITEM1", "Alpha Paper"), missing_abstract);
    let (_server, base) = preview_server(items, "[]".to_string()).await;
    let db = preview_db();
    let preview = get_collection_preview_inner(&base, &db, "KEY").await.expect("preview");
    // Strict RIS validation: title+abstract+authors required.
    assert_eq!(preview.preview.valid_records, 1);
    assert!(preview.preview.error_count >= 1);
    let group = preview
        .preview
        .error_groups
        .iter()
        .find(|g| g.message == "Missing required fields")
        .expect("missing-fields group");
    assert_eq!(group.count, 1);
    // Only the valid item appears in the preview rows.
    assert_eq!(preview.preview.preview_articles.len(), 1);
    assert_eq!(preview.preview.preview_articles[0].title, "Alpha Paper");
}

#[tokio::test]
async fn get_collection_preview_skips_unsupported_types() {
    let webpage = r#"{"key":"WEB1","version":1,"meta":{},"data":{"itemType":"webpage","title":"A Web Page","abstractNote":"Has everything","creators":[{"creatorType":"author","firstName":"A","lastName":"Author"}]}}"#;
    let items = format!("[{},{}]", valid_item("ITEM1", "Alpha Paper"), webpage);
    let (_server, base) = preview_server(items, "[]".to_string()).await;
    let db = preview_db();
    let preview = get_collection_preview_inner(&base, &db, "KEY").await.expect("preview");
    assert_eq!(preview.total_items, 2);
    assert_eq!(preview.mapped_articles, 1, "webpage is not mapped");
    let group = preview
        .preview
        .error_groups
        .iter()
        .find(|g| g.message == "Unsupported Zotero item type")
        .expect("unsupported group");
    assert_eq!(group.count, 1);
    // Items are title-sorted: "A Web Page" sorts before "Alpha Paper".
    assert_eq!(group.record_indices, vec![1]);
}

#[tokio::test]
async fn get_collection_preview_returns_keys_and_version() {
    let items =
        format!("[{},{}]", valid_item("ITEM1", "Alpha Paper"), valid_item("ITEM2", "Beta Paper"));
    let (_server, base) = preview_server(items, "[]".to_string()).await;
    let db = preview_db();
    let preview = get_collection_preview_inner(&base, &db, "KEY").await.expect("preview");
    // articleKeys align with previewArticles (same order, valid records only).
    assert_eq!(preview.article_keys, vec!["ITEM1", "ITEM2"]);
    assert_eq!(preview.preview.preview_articles[0].title, "Alpha Paper");
    assert_eq!(preview.preview.preview_articles[1].title, "Beta Paper");
    // Library version captured from Last-Modified-Version (the change guard).
    assert_eq!(preview.library_version, Some(15));
}

#[tokio::test]
async fn get_collection_preview_dedups_items_across_collections() {
    // The same item lives in the parent AND the subcollection: the recursive
    // walk must fetch/preview it exactly once.
    let shared = valid_item("SHARED", "Shared Paper");
    let only_parent = valid_item("PARENTONLY", "Alpha Only");
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/users/0/collections/KEY/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Last-Modified-Version", "15")
        .with_body(format!("[{shared},{only_parent}]"))
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/KEY/collections")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"key":"SUB","version":1,"data":{"key":"SUB","version":1,"name":"Sub","parentCollection":"KEY"}}]"#)
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/SUB/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!("[{shared}]"))
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/SUB/collections")
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
        .with_body("[]")
        .create_async()
        .await;

    let preview = get_collection_preview_inner(&api_base(&server), &preview_db(), "KEY")
        .await
        .expect("preview");
    assert_eq!(preview.total_items, 2, "SHARED counted once");
    assert_eq!(preview.preview.valid_records, 2);
    assert_eq!(preview.article_keys.len(), 2);
    assert!(
        !preview.article_keys.contains(&"SHARED".to_string())
            || preview.article_keys.iter().filter(|k| *k == "SHARED").count() == 1
    );
}

#[tokio::test]
async fn subcollection_fetch_failure_propagates() {
    // A transient failure on /collections/{key}/collections must surface as
    // an error, not a silent partial import.
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/users/0/collections/KEY/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("Last-Modified-Version", "15")
        .with_body(format!("[{}]", valid_item("ITEM1", "Alpha Paper")))
        .create_async()
        .await;
    server
        .mock("GET", "/api/users/0/collections/KEY/collections")
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_body("boom")
        .create_async()
        .await;

    let result = get_collection_preview_inner(&api_base(&server), &preview_db(), "KEY").await;
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("subcollection fetch failure must propagate"),
    };
    assert!(err.to_string().contains("500"), "got: {err}");
}

#[tokio::test]
async fn check_connection_reports_version_even_when_api_disabled() {
    // The connector ping answers while the local API pref is OFF, so the
    // version gate renders even on api_disabled (Zotero 9 + pref off shows
    // "requires Zotero 10", not the enable-API card).
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/connector/ping")
        .with_status(200)
        .with_header("X-Zotero-Version", "9.0.1")
        .create_async()
        .await;
    server.mock("GET", "/api/").with_status(403).with_body("Forbidden").create_async().await;

    let status = check_connection_inner(&api_base(&server)).await;
    assert_eq!(status.status, "api_disabled");
    assert_eq!(status.zotero_version.as_deref(), Some("9.0.1"));
}

#[tokio::test]
async fn check_connection_404_no_endpoint_shows_enable_hint() {
    // Zotero's connector server answers 404 "No endpoint found" when the
    // local API was not reachable at that moment (startup race / preference
    // not yet active). The user gets actionable guidance, not the raw error.
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/")
        .with_status(404)
        .with_body("No endpoint found")
        .create_async()
        .await;

    let status = check_connection_inner(&api_base(&server)).await;
    assert_eq!(status.status, "api_disabled");
    let hint = status.hint.expect("hint carries the guidance");
    assert!(hint.contains("Could not communicate with Zotero"), "got: {hint}");
    assert!(hint.contains("make sure your Zotero program is running"), "got: {hint}");
    assert!(hint.contains("Allow other applications on this computer to communicate with Zotero"));
    assert!(hint.ends_with("(404)"), "status code suffix: {hint}");
}

#[tokio::test]
async fn not_found_without_connector_signature_stays_raw() {
    // A genuine API 404 (e.g. a missing collection) keeps its accurate raw
    // form - only the connector-server "No endpoint found" signature is
    // rewritten to the guidance message.
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/users/0/collections/MISSING/items")
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body("{\"error\":\"Collection not found\"}")
        .create_async()
        .await;

    let err = match get_collection_preview_inner(&api_base(&server), &preview_db(), "MISSING").await
    {
        Err(e) => e,
        Ok(_) => panic!("404 must error"),
    };
    let message = err.to_string();
    assert!(message.contains("HTTP 404"), "raw form kept: {message}");
    assert!(!message.contains("Could not communicate with Zotero"), "no rewrite: {message}");
}

#[tokio::test]
async fn get_collection_preview_counts_library_duplicates() {
    use bango_lib::db::article_repo;
    use bango_lib::models::article::NewArticle;

    // Seed the library with one article carrying 10.1/present.
    let db = preview_db();
    {
        let conn = db.lock().unwrap();
        let seeded = vec![NewArticle {
            title: "Present In Library".to_string(),
            abstract_text: "Abstract.".to_string(),
            authors: vec!["Author, A".to_string()],
            doi: Some("10.1/present".to_string()),
            ..NewArticle::default()
        }];
        article_repo::insert_articles_batch(&conn, &seeded, "test").expect("seed");
    }

    // Two valid items: one with a prefix/case variant of the library DOI,
    // one without a DOI at all.
    let duplicate_item = valid_item("DUP1", "Duplicate Paper")
        .replace("\"tags\":[", "\"DOI\":\"https://doi.org/10.1/PRESENT\",\"tags\":[");
    let unique_item = valid_item("ITEM1", "Unique Paper");
    let items = format!("[{duplicate_item},{unique_item}]");
    let (_server, base) = preview_server(items, "[]".to_string()).await;

    let preview = get_collection_preview_inner(&base, &db, "KEY").await.expect("preview");
    assert_eq!(preview.preview.valid_records, 2);
    assert_eq!(preview.preview.duplicate_count, 1, "canonical DOI match counts once");
}
