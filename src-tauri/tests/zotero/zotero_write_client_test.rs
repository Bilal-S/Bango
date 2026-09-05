//! Zotero write-client tests (Tier 5): envelope/authorize/upload parsing,
//! write-error classification, batch tokens, the stored-key reuse policy,
//! and the mid-run key-expiry abort. Binding inventory:
//! `docs/test-plans/zotero-tests.md`.

use bango_lib::zotero::write_client::parse_write_envelope as parse_envelope;
use bango_lib::zotero::write_client::{
    build_write_token, classify_write_status, decide_write_auth, parse_authorize_response,
    parse_upload_authorization, UploadAuthorization, WriteAuthDecision, ZoteroWriteError,
};

#[test]
fn parse_write_envelope() {
    let json = serde_json::json!({
        "successful": {
            "1": {"key": "BBBBBBBB", "version": 3},
            "0": {"key": "AAAAAAAA", "version": 3}
        },
        "success": {"0": "AAAAAAAA", "1": "BBBBBBBB"},
        "unchanged": {"2": "CCCCCCCC"},
        "failed": {"3": "Title is empty"}
    });
    let envelope = parse_envelope(&json);
    // Keys extracted in envelope-index order.
    assert_eq!(envelope.successful_keys, vec!["AAAAAAAA", "BBBBBBBB"]);
    assert_eq!(envelope.success_by_index[0], (0, "AAAAAAAA".to_string()));
    assert_eq!(envelope.success_by_index[1], (1, "BBBBBBBB".to_string()));
    assert_eq!(envelope.unchanged_count, 1);
    assert_eq!(envelope.failed, vec![("3".to_string(), "Title is empty".to_string())]);
}

#[test]
fn write_auth_error_classification() {
    use reqwest::StatusCode;
    // 401 with the single-use-key marker -> KeyExpired; any other 401 ->
    // KeyRequired (authorize hint).
    assert!(matches!(
        classify_write_status(StatusCode::UNAUTHORIZED, "Invalid or expired API key", None),
        ZoteroWriteError::KeyExpired
    ));
    assert!(matches!(
        classify_write_status(
            StatusCode::UNAUTHORIZED,
            "API key required -- POST /api/local/authorize",
            None
        ),
        ZoteroWriteError::KeyRequired
    ));
    // 403 denied (authorize Deny button) vs the disabled local API.
    assert!(matches!(
        classify_write_status(StatusCode::FORBIDDEN, "{\"denied\":true}", None),
        ZoteroWriteError::Denied
    ));
    assert!(matches!(
        classify_write_status(StatusCode::FORBIDDEN, "Forbidden", None),
        ZoteroWriteError::ApiDisabled
    ));
    // 429 -> RateLimited with Retry-After seconds.
    assert!(matches!(
        classify_write_status(StatusCode::TOO_MANY_REQUESTS, "rate limited", Some(30)),
        ZoteroWriteError::RateLimited(30)
    ));
    // 428 -> server-id echo guidance.
    assert!(matches!(
        classify_write_status(reqwest::StatusCode::from_u16(428).unwrap(), "no id", None),
        ZoteroWriteError::Http(_)
    ));
    // 501 -> this Zotero cannot write (needs 10).
    assert!(matches!(
        classify_write_status(StatusCode::NOT_IMPLEMENTED, "", None),
        ZoteroWriteError::NeedsZotero10
    ));
}

#[test]
fn authorize_response_parses() {
    let parsed =
        parse_authorize_response(r#"{"key":"abc123def456abc123def456abc123de","remember":true}"#)
            .expect("parses");
    assert_eq!(parsed.key, "abc123def456abc123def456abc123de");
    assert!(parsed.remember);
    let single_use = parse_authorize_response(r#"{"key":"k","remember":false}"#).expect("parses");
    assert!(!single_use.remember);
    assert!(parse_authorize_response("not json").is_none());
}

#[test]
fn ordered_attachment_body_puts_link_mode_before_path_fields() {
    // Zotero's local API applies fields in document order and rejects a
    // filename (attachment path) that precedes linkMode (verified live).
    let item = bango_lib::zotero::write_client::build_attachment_item_json(
        "PARENT1",
        "Doe - A study.pdf",
        "Doe - A study.pdf",
    );
    let body = bango_lib::zotero::write_client::ordered_attachment_body(&item);
    let link_mode = body.find("\"linkMode\"").expect("linkMode present");
    let filename = body.find("\"filename\"").expect("filename present");
    let content_type = body.find("\"contentType\"").expect("contentType present");
    assert!(link_mode < filename, "linkMode must precede filename: {body}");
    assert!(link_mode < content_type, "linkMode must precede contentType: {body}");
    // The ordered body round-trips to the exact same field set.
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid json");
    assert_eq!(parsed[0], item);
}

#[test]
fn build_attachment_item_json() {
    let pdf = bango_lib::zotero::write_client::build_attachment_item_json(
        "PARENT1",
        "Jones - The awakening.pdf",
        "Jones - The awakening.pdf",
    );
    assert_eq!(pdf["itemType"], "attachment");
    assert_eq!(pdf["parentItem"], "PARENT1");
    assert_eq!(pdf["linkMode"], "imported_file");
    assert_eq!(pdf["contentType"], "application/pdf");
    assert_eq!(pdf["filename"], "Jones - The awakening.pdf");
    assert_eq!(pdf["title"], "Jones - The awakening.pdf");
    let txt = bango_lib::zotero::write_client::build_attachment_item_json(
        "PARENT1",
        "notes.txt",
        "Doe - Notes.txt",
    );
    assert_eq!(txt["contentType"], "text/plain");
    assert_eq!(txt["title"], "Doe - Notes.txt");
}

#[test]
fn build_upload_params() {
    let params = bango_lib::zotero::write_client::build_upload_params(
        "d41d8cd98f00b204e9800998ecf8427e",
        "paper.pdf",
        1024,
        1_700_000_000_000,
    );
    assert_eq!(params.md5, "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(params.filename, "paper.pdf");
    assert_eq!(params.filesize, 1024);
    assert_eq!(params.mtime_ms, 1_700_000_000_000);
    assert!(params.if_none_match_star, "If-None-Match: * is mandatory");
}

#[test]
fn build_upload_auth_body_percent_encodes_spaces() {
    // Spaces must reach Zotero as %20, never '+': the local form decoder
    // passes '+' through literally into the stored filename.
    let body = bango_lib::zotero::write_client::build_upload_auth_body(
        "d41d8cd98f00b204e9800998ecf8427e",
        "Jones - The awakening.pdf",
        1024,
        1_700_000_000_000,
    );
    assert_eq!(
        body,
        "md5=d41d8cd98f00b204e9800998ecf8427e&filename=Jones%20-%20The%20awakening.pdf&filesize=1024&mtime=1700000000000"
    );
    // %, &, = and + never leak into the raw body.
    let tricky =
        bango_lib::zotero::write_client::build_upload_auth_body("a", "a%b&c=d+e.pdf", 1, 2);
    assert_eq!(tricky, "md5=a&filename=a%25b%26c%3Dd%2Be.pdf&filesize=1&mtime=2");
}

#[tokio::test]
async fn attachment_item_creation_surfaces_envelope_failures() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/api/users/0/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"successful":{},"success":{},"unchanged":{},"failed":{"0":"Title is empty"}}"#,
        )
        .create_async()
        .await;
    let base = format!("{}/api", server.url());
    let err = bango_lib::zotero::write_client::create_attachment_item(
        &base,
        "SID",
        "KEY",
        "PARENT1",
        "paper.pdf",
        "Doe - Paper.pdf",
    )
    .await
    .err()
    .expect("must fail without a created key");
    let message = err.to_string();
    assert!(message.contains("returned no key"), "{message}");
    assert!(message.contains("[0] Title is empty"), "{message}");
}

#[test]
fn upload_authorization_response_branches() {
    let upload = parse_upload_authorization(
        r#"{"url":"http://localhost:23119/api/local/uploads/KEY1","uploadKey":"KEY1"}"#,
    )
    .expect("parses");
    assert_eq!(
        upload,
        UploadAuthorization::Upload {
            url: "http://localhost:23119/api/local/uploads/KEY1".to_string(),
            upload_key: "KEY1".to_string()
        }
    );
    assert_eq!(parse_upload_authorization(r#"{"exists":1}"#), Some(UploadAuthorization::Exists));
    assert!(parse_upload_authorization("{}").is_none());
}

#[test]
fn batches_chunked_at_50_with_fresh_tokens() {
    // Batch splitting at 50 (120 items -> 50/50/20).
    let items: Vec<usize> = (0..120).collect();
    let sizes: Vec<usize> = items.chunks(50).map(<[usize]>::len).collect();
    assert_eq!(sizes, vec![50, 50, 20]);
    // A unique 32-char Zotero-Write-Token per batch.
    let tokens: Vec<String> = (0..10).map(|_| build_write_token()).collect();
    for token in &tokens {
        assert_eq!(token.len(), 32, "token {token}");
    }
    let mut unique = tokens.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), tokens.len(), "every batch token is unique");
}

#[test]
fn decide_write_auth_reuses_stored_key() {
    // Stored key + matching live server id -> silent reuse, no dialog.
    assert_eq!(
        decide_write_auth(Some("stored-key"), Some("SID1"), Some("SID1")),
        WriteAuthDecision::UseStored
    );
}

#[test]
fn decide_write_auth_requires_authorize_when() {
    // Missing key -> Authorize.
    assert_eq!(decide_write_auth(None, Some("SID1"), Some("SID1")), WriteAuthDecision::Authorize);
    // Server-id mismatch (different Zotero instance owns the old key).
    assert_eq!(
        decide_write_auth(Some("stored-key"), Some("OLD"), Some("NEW")),
        WriteAuthDecision::Authorize
    );
    // Missing stored server id or live id -> cannot compare -> Authorize.
    assert_eq!(
        decide_write_auth(Some("stored-key"), None, Some("SID1")),
        WriteAuthDecision::Authorize
    );
    assert_eq!(
        decide_write_auth(Some("stored-key"), Some("SID1"), None),
        WriteAuthDecision::Authorize
    );
    // Empty key is treated as missing.
    assert_eq!(
        decide_write_auth(Some(""), Some("SID1"), Some("SID1")),
        WriteAuthDecision::Authorize
    );
}

#[tokio::test]
async fn key_expired_mid_run_aborts_with_guidance() {
    use bango_lib::commands::zotero::export_zotero_collection_core;
    use bango_lib::db::app_settings_repo::{get_setting, set_setting, STORAGE_ROOT_KEY};
    use bango_lib::db::migration::run_migrations;

    let tmp = tempfile::TempDir::new().unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    set_setting(&conn, STORAGE_ROOT_KEY, tmp.path().to_str()).unwrap();

    // 51 articles -> two batches (50 + 1), each with a unique DOI.
    {
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..51 {
            tx.execute(
                "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors, keywords, doi)
                 VALUES (?1, ?2, 'working', ?3, 'x', '[]', '[]', ?4)",
                rusqlite::params![format!("e-{i}"), (i + 1) as i64, format!("Export {i}"), format!("10.9/x{i}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }

    // A stored key bound to the same server id -> UseStored (no dialog).
    let machine_key = bango_lib::crypto::aes_gcm::derive_key_from_machine();
    let encrypted = bango_lib::crypto::aes_gcm::encrypt(b"single-use-key", &machine_key).unwrap();
    set_setting(&conn, "zotero_api_key", Some(&encrypted)).unwrap();
    set_setting(&conn, "zotero_server_id", Some("SID9")).unwrap();

    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/api/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("X-Zotero-Version", "10.0.1")
        .with_header("Zotero-Server-ID", "SID9")
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
    // POST items: first batch succeeds (empty envelope), second batch hits
    // the single-use-key 401. Mockito matches in reverse creation order, so
    // the 401 mock is created FIRST and used only after the 200 exhausts.
    server
        .mock("POST", "/api/users/0/items")
        .match_query(mockito::Matcher::Any)
        .with_status(401)
        .with_body("Invalid or expired API key")
        .create_async()
        .await;
    server
        .mock("POST", "/api/users/0/items")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"successful":{},"success":{},"unchanged":{},"failed":{}}"#)
        .create_async()
        .await;

    let db = std::sync::Mutex::new(conn);
    let base = format!("{}/api", server.url());
    let err =
        export_zotero_collection_core(&base, &db, "KEY", "all", false, false, &|_, _, _, _| {})
            .await
            .err()
            .expect("run must abort");
    assert!(err.to_string().contains("did not remember"), "guidance surfaces: {err}");

    // The stale stored key is cleared so the next attempt re-authorizes once.
    let cleared = {
        let conn = db.lock().unwrap();
        get_setting(&conn, "zotero_api_key").unwrap()
    };
    assert!(cleared.is_none(), "stored key cleared after KeyExpired");
}

#[test]
fn authorize_send_error_classification() {
    use bango_lib::zotero::write_client::classify_authorize_send;
    // A timeout on the authorize call means the user never answered the 120 s
    // dialog - distinct from Zotero not running.
    assert!(matches!(
        classify_authorize_send(true, false, "timed out".to_string()),
        ZoteroWriteError::DialogTimeout
    ));
    assert!(classify_authorize_send(true, false, "timed out".to_string())
        .to_string()
        .contains("permission dialog timed out"));
    assert!(matches!(
        classify_authorize_send(false, true, String::new()),
        ZoteroWriteError::NotRunning
    ));
    assert!(matches!(
        classify_authorize_send(false, false, "dns failure".to_string()),
        ZoteroWriteError::Http(_)
    ));
}
