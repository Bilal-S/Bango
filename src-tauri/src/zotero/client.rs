//! Zotero local API HTTP client.
//!
//! One shared `OnceLock<reqwest::Client>` with a 5-second total timeout (every
//! payload is small local JSON), `redirect::Policy::none()` (the 302
//! `Location` of an attachment file is data, not something to follow), and the
//! `Zotero-API-Version: 3` header on every call. The local API has no rate
//! limits and does not paginate by default, so no retry or paging logic.
//!
//! Status mapping is total: refused/timeout -> `NotRunning`, `403` ->
//! `ApiDisabled`, any other non-success status or decode failure ->
//! `Http`/`Parse` carrying the status code and a body snippet.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use futures::StreamExt;
use percent_encoding::percent_decode_str;
use serde::de::DeserializeOwned;

use super::{
    ZoteroChildItem, ZoteroCollection, ZoteroError, ZoteroItem, ZoteroNoteItem, LOCAL_USER_ID,
};

const REQUEST_TIMEOUT_SECS: u64 = 5;

fn shared_client() -> Result<&'static reqwest::Client, ZoteroError> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Zotero-API-Version", reqwest::header::HeaderValue::from_static("3"));
            reqwest::Client::builder()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .redirect(reqwest::redirect::Policy::none())
                .default_headers(headers)
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ZoteroError::Http(format!("Failed to build HTTP client: {e}")))
}

/// Shared client accessor for sibling modules (the write client and the
/// connector call in `commands/zotero.rs`). Everything Zotero-bound goes
/// through this 5s-timeout client - a default `reqwest::Client::new()` has no
/// total timeout and can hang forever on a filtered port.
pub(crate) fn shared_client_5s() -> Result<&'static reqwest::Client, ZoteroError> {
    shared_client()
}

/// Map a reqwest send error: connection refused/timeout means Zotero is not
/// running; anything else is a generic HTTP failure.
fn map_send_error(e: reqwest::Error) -> ZoteroError {
    if e.is_connect() || e.is_timeout() {
        ZoteroError::NotRunning
    } else {
        ZoteroError::Http(e.to_string())
    }
}

/// Map a non-success status: `403` is the disabled-local-API marker, `404`
/// with the connector-server's "No endpoint found" body means the local API
/// was not reachable at that moment (startup race / preference not yet
/// active) and gets the actionable guidance message; anything else carries
/// the status and a body snippet.
fn map_status(status: reqwest::StatusCode, body: &str) -> ZoteroError {
    if status == reqwest::StatusCode::FORBIDDEN {
        ZoteroError::ApiDisabled
    } else if status == reqwest::StatusCode::NOT_FOUND
        && body.to_lowercase().contains("no endpoint found")
    {
        ZoteroError::ApiEndpointMissing(status.as_u16())
    } else {
        ZoteroError::Http(format!("HTTP {status}: {}", snippet(body)))
    }
}

/// First ~200 chars of a response body, for error messages and diagnostics.
pub(super) fn snippet(body: &str) -> &str {
    let trimmed = body.trim();
    let cut = trimmed.char_indices().nth(200).map_or(trimmed.len(), |(i, _)| i);
    &trimmed[..cut]
}

fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
}

fn header_i64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<i64> {
    header_string(headers, name).and_then(|v| v.parse().ok())
}

/// GET `url` and deserialize the JSON body, returning the response headers.
async fn get_json<T: DeserializeOwned>(
    url: &str,
) -> Result<(T, reqwest::header::HeaderMap), ZoteroError> {
    let client = shared_client()?;
    let response = client.get(url).send().await.map_err(|e| {
        eprintln!("[zotero] GET {url} send failed: {e}");
        map_send_error(e)
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("[zotero] GET {url} -> {status}: {}", snippet(&body));
        return Err(map_status(status, &body));
    }
    let headers = response.headers().clone();
    let text = response.text().await.map_err(|e| {
        eprintln!("[zotero] GET {url} body read failed: {e}");
        ZoteroError::Http(format!("Failed to read Zotero response: {e}"))
    })?;
    let parsed = serde_json::from_str(&text).map_err(|e| {
        eprintln!("[zotero] GET {url} parse failed: {e} (body: {})", snippet(&text));
        ZoteroError::Parse(format!("{e} (body: {})", snippet(&text)))
    })?;
    Ok((parsed, headers))
}

/// Connection probe result. `api_version` is the `Zotero-API-Version`
/// response header; `zotero_version`/`server_id` come from `X-Zotero-Version`
/// and `Zotero-Server-ID` (present on every response, verified live).
#[derive(Debug, Clone, Default)]
pub struct ZoteroConnectionInfo {
    pub api_version: Option<String>,
    pub zotero_version: Option<String>,
    pub server_id: Option<String>,
}

/// Probe `GET /api/`. The local API only registers the route WITH its
/// trailing slash - a slashless `GET /api` answers the connector server's
/// `404 No endpoint found` fallback (live-verified on Zotero 10.0.1), so the
/// probe always sends the slashed form. Errors carry the mapped status
/// (`NotRunning` / `ApiDisabled` / `Http`), never a hang - the shared client
/// has a total timeout.
pub async fn check_connection(base_url: &str) -> Result<ZoteroConnectionInfo, ZoteroError> {
    let client = shared_client()?;
    let probe_url =
        if base_url.ends_with('/') { base_url.to_string() } else { format!("{base_url}/") };
    let response = client.get(&probe_url).send().await.map_err(|e| {
        eprintln!("[zotero] connection probe {probe_url} send failed: {e}");
        map_send_error(e)
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("[zotero] connection probe {probe_url} -> {status}: {}", snippet(&body));
        return Err(map_status(status, &body));
    }
    let headers = response.headers().clone();
    Ok(ZoteroConnectionInfo {
        api_version: header_string(&headers, "Zotero-API-Version"),
        zotero_version: header_string(&headers, "X-Zotero-Version"),
        server_id: header_string(&headers, "Zotero-Server-ID"),
    })
}

/// Connector ping (`GET /connector/ping`) - answers even while the local API
/// preference is OFF, so it is the canonical `X-Zotero-Version` source for
/// the write gate (a Zotero 9 with the API disabled must see the "requires
/// Zotero 10" gate, not the enable-API card). Best-effort: any failure ->
/// `None`; never hangs (shared 5s client).
pub async fn connector_ping_version(base_url: &str) -> Option<String> {
    let connector_base = base_url.trim_end_matches("/api");
    let url = format!("{connector_base}/connector/ping");
    let client = shared_client().ok()?;
    let response = client.get(&url).send().await.ok()?;
    let status = response.status();
    if !status.is_success() {
        eprintln!("[zotero] connector ping {url} -> {status}");
        return None;
    }
    header_string(response.headers(), "X-Zotero-Version")
}

/// The flat collection list (`GET /users/0/collections`).
pub async fn fetch_collections(base_url: &str) -> Result<Vec<ZoteroCollection>, ZoteroError> {
    let url = format!("{base_url}/users/{LOCAL_USER_ID}/collections?format=json");
    let (collections, _) = get_json(&url).await?;
    Ok(collections)
}

/// Top-level items of a collection INCLUDING subcollections, plus the
/// `Last-Modified-Version` library version. The local API's
/// `/collections/{key}/items` is NOT recursive (verified live on Zotero
/// 10.0.1), so subcollections are walked explicitly via
/// `/collections/{key}/collections` with a seen-set guard. Child
/// attachments/notes are partitioned out here; they come from the bulk
/// attachment fetch instead.
pub struct ZoteroItemsPage {
    pub items: Vec<ZoteroItem>,
    pub library_version: Option<i64>,
}

pub async fn fetch_collection_items(
    base_url: &str,
    collection_key: &str,
) -> Result<ZoteroItemsPage, ZoteroError> {
    let mut to_visit = vec![collection_key.to_string()];
    let mut seen_collections: HashSet<String> = HashSet::new();
    let mut seen_items: HashSet<String> = HashSet::new();
    let mut items: Vec<ZoteroItem> = Vec::new();
    let mut library_version: Option<i64> = None;

    while let Some(key) = to_visit.pop() {
        if !seen_collections.insert(key.clone()) {
            continue;
        }
        let items_url = format!(
            "{base_url}/users/{LOCAL_USER_ID}/collections/{key}/items?format=json&sort=title&direction=asc"
        );
        let (page_items, headers): (Vec<ZoteroItem>, _) = get_json(&items_url).await?;
        if library_version.is_none() {
            library_version = header_i64(&headers, "Last-Modified-Version");
        }
        items.extend(
            page_items
                .into_iter()
                .filter(|item| item.data.parent_item.is_none())
                // An item can live in a parent collection AND a subcollection;
                // dedup by item key so it is fetched, previewed, and imported
                // exactly once (the version guard cannot catch this case).
                .filter(|item| seen_items.insert(item.key.clone())),
        );

        let sub_url =
            format!("{base_url}/users/{LOCAL_USER_ID}/collections/{key}/collections?format=json");
        let (subcollections, _): (Vec<ZoteroCollection>, _) = get_json(&sub_url).await?;
        for sub in subcollections {
            to_visit.push(sub.key);
        }
    }

    // Stable order across the merged tree (the API sorts per collection).
    items.sort_by(|a, b| {
        a.data.title.as_deref().unwrap_or_default().cmp(b.data.title.as_deref().unwrap_or_default())
    });

    Ok(ZoteroItemsPage { items, library_version })
}

/// NON-recursive top-level items of a collection (`/items/top`) plus the
/// library version - the export DOI-diff baseline.
pub async fn fetch_collection_top_items(
    base_url: &str,
    collection_key: &str,
) -> Result<ZoteroItemsPage, ZoteroError> {
    let url = format!(
        "{base_url}/users/{LOCAL_USER_ID}/collections/{collection_key}/items/top?format=json"
    );
    let (items, headers): (Vec<ZoteroItem>, _) = get_json(&url).await?;
    Ok(ZoteroItemsPage { items, library_version: header_i64(&headers, "Last-Modified-Version") })
}

/// Attachments of the given parent items. Primary path is ONE bulk
/// `GET /users/0/items?itemType=attachment` request (kills the per-item N+1);
/// if that endpoint is unavailable, fall back to per-item `/children`
/// requests through a bounded pool of 4.
pub async fn fetch_all_attachments(
    base_url: &str,
    parent_keys: &[String],
) -> Result<Vec<ZoteroChildItem>, ZoteroError> {
    if parent_keys.is_empty() {
        return Ok(Vec::new());
    }
    let bulk_url =
        format!("{base_url}/users/{LOCAL_USER_ID}/items?itemType=attachment&format=json");
    if let Ok((all, _)) = get_json::<Vec<ZoteroChildItem>>(&bulk_url).await {
        let parents: HashSet<&str> = parent_keys.iter().map(String::as_str).collect();
        return Ok(all
            .into_iter()
            .filter(|child| child.data.parent_item.as_deref().is_some_and(|p| parents.contains(p)))
            .collect());
    }

    // Owned copies avoid the HRTB closure-lifetime issue with async blocks.
    let owned_keys: Vec<String> = parent_keys.to_vec();
    let requests = owned_keys.into_iter().map(|key| {
        let url = format!("{base_url}/users/{LOCAL_USER_ID}/items/{key}/children?format=json");
        async move { get_json::<Vec<ZoteroChildItem>>(&url).await.map(|(children, _)| children) }
    });
    let results = futures::stream::iter(requests).buffer_unordered(4).collect::<Vec<_>>().await;
    let mut children = Vec::new();
    for result in results {
        children.extend(result?);
    }
    Ok(children)
}

/// Child notes of the given parent items. Mirrors `fetch_all_attachments`:
/// primary path is ONE bulk `GET /users/0/items?itemType=note` request; if
/// that endpoint is unavailable, fall back to per-item `/children` requests
/// (filtered to `itemType = note`) through a bounded pool of 4.
pub async fn fetch_all_notes(
    base_url: &str,
    parent_keys: &[String],
) -> Result<Vec<ZoteroNoteItem>, ZoteroError> {
    if parent_keys.is_empty() {
        return Ok(Vec::new());
    }
    let bulk_url = format!("{base_url}/users/{LOCAL_USER_ID}/items?itemType=note&format=json");
    if let Ok((all, _)) = get_json::<Vec<ZoteroNoteItem>>(&bulk_url).await {
        let parents: HashSet<&str> = parent_keys.iter().map(String::as_str).collect();
        return Ok(all
            .into_iter()
            .filter(|note| note.data.parent_item.as_deref().is_some_and(|p| parents.contains(p)))
            .collect());
    }

    let owned_keys: Vec<String> = parent_keys.to_vec();
    let requests = owned_keys.into_iter().map(|key| {
        let url = format!("{base_url}/users/{LOCAL_USER_ID}/items/{key}/children?format=json");
        async move {
            get_json::<Vec<ZoteroNoteItem>>(&url).await.map(|(children, _)| {
                children.into_iter().filter(|c| c.data.item_type == "note").collect::<Vec<_>>()
            })
        }
    });
    let results = futures::stream::iter(requests).buffer_unordered(4).collect::<Vec<_>>().await;
    let mut notes = Vec::new();
    for result in results {
        notes.extend(result?);
    }
    Ok(notes)
}

/// Resolve the on-disk file of a stored attachment. `GET /users/0/items/{key}/file`
/// answers `302` with a `file://` `Location` (redirects are NOT followed -
/// the Location is the data), which is resolved to a `PathBuf`. The defensive
/// `200`-with-body case writes the bytes to a temp file under
/// `std::env::temp_dir()` and returns that path; `attach_full_text_inner`
/// still validates the extension downstream.
pub async fn fetch_attachment_file_path(
    base_url: &str,
    attachment_key: &str,
) -> Result<PathBuf, ZoteroError> {
    let url = format!("{base_url}/users/{LOCAL_USER_ID}/items/{attachment_key}/file");
    let client = shared_client()?;
    let response = client.get(&url).send().await.map_err(map_send_error)?;
    let status = response.status();
    if status.as_u16() == 301 || status.as_u16() == 302 {
        let location = response
            .headers()
            .get("Location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                eprintln!(
                    "[zotero] attachment {attachment_key} file -> {status} without a Location header"
                );
                ZoteroError::Http(format!(
                    "Zotero returned {status} without a Location header for attachment {attachment_key}"
                ))
            })?;
        return resolve_attachment_path(location);
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!(
            "[zotero] attachment {attachment_key} file {url} -> {status}: {}",
            snippet(&body)
        );
        return Err(map_status(status, &body));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ZoteroError::Http(format!("Failed to read attachment body: {e}")))?;
    let ext = if bytes.len() >= 4 && &bytes[..4] == b"%PDF" { "pdf" } else { "txt" };
    let path = std::env::temp_dir().join(format!("zotero_{attachment_key}.{ext}"));
    std::fs::write(&path, &bytes)
        .map_err(|e| ZoteroError::Http(format!("Failed to write attachment temp file: {e}")))?;
    Ok(path)
}

/// Resolve a `file://` Location header to a `PathBuf`. Pure string logic -
/// no `Url::to_file_path()` - so every branch runs (and is tested) on every
/// platform:
///
/// 1. Parse with `reqwest::Url`; require scheme `file`, otherwise the
///    `NonFileScheme` error (e.g. an `http://` Location).
/// 2. Host present (UNC share, `file://server/share/a.pdf`): percent-decode
///    the path and build `\\{host}{path}` with backslashes.
/// 3. Host empty: percent-decode the path. A Windows drive (leading slash,
///    drive letter, colon - `/C:/Users/...`) loses its leading slash;
///    anything else is kept as the POSIX path.
pub fn resolve_attachment_path(location: &str) -> Result<PathBuf, ZoteroError> {
    let parsed = reqwest::Url::parse(location)
        .map_err(|_| ZoteroError::Parse(format!("Invalid attachment location: {location}")))?;
    if parsed.scheme() != "file" {
        return Err(ZoteroError::NonFileScheme(location.to_string()));
    }
    let decoded = percent_decode_str(parsed.path()).decode_utf8_lossy();
    let host = parsed.host_str().unwrap_or_default();
    if !host.is_empty() {
        // UNC share: file://server/share/a.pdf -> \\server\share\a.pdf
        let path = decoded.replace('/', "\\");
        return Ok(PathBuf::from(format!("\\\\{host}{path}")));
    }
    let bytes = decoded.as_bytes();
    if decoded.starts_with('/')
        && bytes.len() >= 3
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
    {
        // Windows drive letter: /C:/Users/... -> C:/Users/...
        return Ok(PathBuf::from(decoded[1..].to_string()));
    }
    Ok(PathBuf::from(decoded.into_owned()))
}
