//! Zotero local WRITE API client (Zotero 10+): key authorization, batched
//! item creation with write tokens, attachment items, the 3-phase file
//! upload, and versioned deletes. Every request echoes `Zotero-Server-ID`;
//! the shared read client's 5s timeout is overridden for the authorize call
//! (the confirmation dialog blocks up to 120 s).

use std::time::Duration;

use serde::Deserialize;

use super::client::{shared_client_5s as shared_client, snippet};
use super::{ZoteroError, LOCAL_USER_ID};

/// Typed write errors; Tier 6 maps each to dialog copy. ApiDisabled /
/// NotRunning / Http all repeat the local-API preference path.
#[derive(Debug, thiserror::Error)]
pub enum ZoteroWriteError {
    #[error("Zotero is not running. Start Zotero and try again.")]
    NotRunning,
    #[error("The Zotero local API is disabled. Enable it in Zotero under Settings -> Advanced -> \"Allow other applications on this computer to communicate with Zotero\".")]
    ApiDisabled,
    #[error("Export to Zotero requires Zotero 10 or newer.")]
    NeedsZotero10,
    #[error("Authorization was denied in Zotero.")]
    Denied,
    #[error("Zotero rate-limited the authorization dialog. Try again in {0} seconds.")]
    RateLimited(u64),
    #[error(
        "Zotero did not remember the authorization - tick Remember in the dialog, then try again."
    )]
    KeyExpired,
    #[error(
        "The Zotero permission dialog timed out. Click Export again and allow the request in Zotero."
    )]
    DialogTimeout,
    #[error("A Zotero API key is required. Click Export again and allow the request in Zotero.")]
    KeyRequired,
    #[error("Zotero write failed: {0}")]
    Http(String),
}

impl From<ZoteroError> for ZoteroWriteError {
    fn from(e: ZoteroError) -> Self {
        match e {
            ZoteroError::NotRunning => ZoteroWriteError::NotRunning,
            ZoteroError::ApiDisabled => ZoteroWriteError::ApiDisabled,
            // Keep the actionable guidance copy if the API state changes
            // mid-export (never a raw "HTTP 404: No endpoint found").
            ZoteroError::ApiEndpointMissing(_) => ZoteroWriteError::Http(e.to_string()),
            ZoteroError::Http(m) => ZoteroWriteError::Http(m),
            ZoteroError::Parse(m) => ZoteroWriteError::Http(m),
            ZoteroError::NonFileScheme(m) => ZoteroWriteError::Http(m),
        }
    }
}

/// Stored-key reuse policy: re-authorize ONLY when the key is missing or the
/// live `Zotero-Server-ID` differs from the stored one (a different Zotero
/// instance/profile owns the old key). Enforced by unit tests, not buried in
/// client code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAuthDecision {
    UseStored,
    Authorize,
}

#[must_use]
pub fn decide_write_auth(
    stored_key: Option<&str>,
    stored_server_id: Option<&str>,
    live_server_id: Option<&str>,
) -> WriteAuthDecision {
    match (stored_key, stored_server_id, live_server_id) {
        (Some(key), Some(stored), Some(live)) if !key.is_empty() && stored == live => {
            WriteAuthDecision::UseStored
        }
        _ => WriteAuthDecision::Authorize,
    }
}

/// The standard write envelope: `successful`/`success`/`unchanged`/`failed`.
#[derive(Debug, Clone, Default)]
pub struct WriteEnvelope {
    /// Created item keys in envelope-index order.
    pub successful_keys: Vec<String>,
    /// Envelope index -> created key (maps batch positions to Zotero keys).
    pub success_by_index: Vec<(usize, String)>,
    pub unchanged_count: usize,
    /// Failed envelope index -> message.
    pub failed: Vec<(String, String)>,
}

/// Parse `{ "successful": {...}, "success": {...}, "unchanged": {}, "failed": {} }`.
#[must_use]
pub fn parse_write_envelope(json: &serde_json::Value) -> WriteEnvelope {
    let mut envelope = WriteEnvelope::default();
    if let Some(success) = json.get("success").and_then(|v| v.as_object()) {
        let mut indexed: Vec<(&String, &serde_json::Value)> = success.iter().collect();
        indexed.sort_by_key(|(index, _)| index.parse::<u32>().unwrap_or(u32::MAX));
        for (index, key) in indexed {
            if let Some(key) = key.as_str() {
                envelope.successful_keys.push(key.to_string());
                envelope
                    .success_by_index
                    .push((index.parse::<usize>().unwrap_or(0), key.to_string()));
            }
        }
    }
    envelope.unchanged_count =
        json.get("unchanged").and_then(|v| v.as_object()).map_or(0, serde_json::Map::len);
    if let Some(failed) = json.get("failed").and_then(|v| v.as_object()) {
        for (index, value) in failed {
            let message = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            envelope.failed.push((index.clone(), message));
        }
    }
    envelope
}

/// The authorize endpoint response: `{ "key": <32 chars>, "remember": bool }`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizeResponse {
    pub key: String,
    pub remember: bool,
}

#[must_use]
pub fn parse_authorize_response(body: &str) -> Option<AuthorizeResponse> {
    serde_json::from_str(body).ok()
}

/// A fresh 32-char `Zotero-Write-Token` (idempotency token protecting
/// unversioned new-item POSTs against double submits).
#[must_use]
pub fn build_write_token() -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut rng = rand::rng();
    (0..32).map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char).collect()
}

/// Map a non-success write status to a typed error: 401 with "Invalid or
/// expired" is `KeyExpired` (single-use key mid-run), any other 401 is
/// `KeyRequired`; 403 with `{"denied":true}` is `Denied`, else `ApiDisabled`;
/// 429 is rate-limited (`Retry-After` seconds); 428 names the server-id
/// echo; 501 means this Zotero cannot write (needs 10).
pub fn classify_write_status(
    status: reqwest::StatusCode,
    body: &str,
    retry_after_secs: Option<u64>,
) -> ZoteroWriteError {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        if body.contains("Invalid or expired") {
            return ZoteroWriteError::KeyExpired;
        }
        return ZoteroWriteError::KeyRequired;
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        if body.contains("denied") {
            return ZoteroWriteError::Denied;
        }
        return ZoteroWriteError::ApiDisabled;
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return ZoteroWriteError::RateLimited(retry_after_secs.unwrap_or(60));
    }
    if status.as_u16() == 428 {
        return ZoteroWriteError::Http(
            "Zotero requires the Zotero-Server-ID header on writes (428).".to_string(),
        );
    }
    if status == reqwest::StatusCode::NOT_IMPLEMENTED {
        return ZoteroWriteError::NeedsZotero10;
    }
    ZoteroWriteError::Http(format!("HTTP {status}: {body}"))
}

/// Build the `imported_file` attachment child item JSON (contentType derived
/// from the filename extension).
#[must_use]
pub fn build_attachment_item_json(parent_key: &str, filename: &str) -> serde_json::Value {
    let content_type =
        if filename.to_lowercase().ends_with(".pdf") { "application/pdf" } else { "text/plain" };
    serde_json::json!({
        "itemType": "attachment",
        "parentItem": parent_key,
        "linkMode": "imported_file",
        "contentType": content_type,
        "filename": filename,
        "tags": [],
    })
}

/// The phase-1 upload authorization params (form fields + the mandatory
/// `If-None-Match: *` marker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadParams {
    pub md5: String,
    pub filename: String,
    pub filesize: u64,
    pub mtime_ms: u64,
    pub if_none_match_star: bool,
}

#[must_use]
pub fn build_upload_params(
    md5: &str,
    filename: &str,
    filesize: u64,
    mtime_ms: u64,
) -> UploadParams {
    UploadParams {
        md5: md5.to_string(),
        filename: filename.to_string(),
        filesize,
        mtime_ms,
        if_none_match_star: true,
    }
}

/// The phase-1 response: either an upload URL + key, or the file already
/// exists server-side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadAuthorization {
    Upload { url: String, upload_key: String },
    Exists,
}

#[must_use]
pub fn parse_upload_authorization(body: &str) -> Option<UploadAuthorization> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("exists").and_then(|v| v.as_i64()) == Some(1) {
        return Some(UploadAuthorization::Exists);
    }
    let url = value.get("url")?.as_str()?.to_string();
    let upload_key = value.get("uploadKey")?.as_str()?.to_string();
    Some(UploadAuthorization::Upload { url, upload_key })
}

/// `POST /api/local/authorize` - blocks while the confirmation dialog shows
/// (per-request 120 s timeout; allow -> `{key, remember}`, deny -> 403
/// `{"denied":true}`, >5 dialogs/min -> 429).
pub async fn authorize(
    base_url: &str,
    server_id: &str,
    app_name: &str,
) -> Result<(String, bool), ZoteroWriteError> {
    let url = format!("{base_url}/local/authorize");
    let response = shared_client()
        .map_err(ZoteroWriteError::from)?
        .post(&url)
        .timeout(Duration::from_secs(120))
        .header("Zotero-Server-ID", server_id)
        .json(&serde_json::json!({ "appName": app_name }))
        .send()
        .await
        .map_err(|e| classify_authorize_send(e.is_timeout(), e.is_connect(), e.to_string()))?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        eprintln!("[zotero] authorize {url} -> {status}: {}", snippet(&body));
        return Err(classify_write_status(status, &body, retry_after));
    }
    let parsed = parse_authorize_response(&body)
        .ok_or_else(|| ZoteroWriteError::Http(format!("Unexpected authorize response: {body}")))?;
    // Never log the granted key itself.
    eprintln!(
        "[zotero] authorize granted (remember={}, key length {})",
        parsed.remember,
        parsed.key.len()
    );
    Ok((parsed.key, parsed.remember))
}

/// Map a send error from the AUTHORIZE call: a timeout means the user never
/// answered the 120 s dialog (distinct from Zotero not running - a refusal to
/// act on the dialog must not claim Zotero is down).
#[must_use]
pub fn classify_authorize_send(
    is_timeout: bool,
    is_connect: bool,
    message: String,
) -> ZoteroWriteError {
    if is_timeout {
        ZoteroWriteError::DialogTimeout
    } else if is_connect {
        ZoteroWriteError::NotRunning
    } else {
        ZoteroWriteError::Http(message)
    }
}

fn map_write_send_error(e: reqwest::Error) -> ZoteroWriteError {
    if e.is_connect() || e.is_timeout() {
        ZoteroWriteError::NotRunning
    } else {
        ZoteroWriteError::Http(e.to_string())
    }
}

/// POST one batch of new items (at most 50, a fresh `Zotero-Write-Token` per
/// batch). Returns the parsed envelope.
pub async fn post_items_batch(
    base_url: &str,
    server_id: &str,
    api_key: &str,
    items: &[serde_json::Value],
) -> Result<WriteEnvelope, ZoteroWriteError> {
    let url = format!("{base_url}/users/{LOCAL_USER_ID}/items");
    let response = shared_client()
        .map_err(ZoteroWriteError::from)?
        .post(&url)
        .header("Zotero-Server-ID", server_id)
        .header("Zotero-API-Key", api_key)
        .header("Zotero-Write-Token", build_write_token())
        .json(&items)
        .send()
        .await
        .map_err(map_write_send_error)?;
    envelope_from_response(response).await
}

/// Create one `imported_file` attachment child and return its new key.
pub async fn create_attachment_item(
    base_url: &str,
    server_id: &str,
    api_key: &str,
    parent_key: &str,
    filename: &str,
) -> Result<String, ZoteroWriteError> {
    let item = build_attachment_item_json(parent_key, filename);
    let envelope = post_items_batch(base_url, server_id, api_key, &[item]).await?;
    envelope.successful_keys.first().cloned().ok_or_else(|| {
        ZoteroWriteError::Http("Attachment item creation returned no key".to_string())
    })
}

/// Versioned delete (`If-Unmodified-Since-Version` is mandatory, else 428).
pub async fn delete_item(
    base_url: &str,
    server_id: &str,
    api_key: &str,
    item_key: &str,
    item_version: i64,
) -> Result<(), ZoteroWriteError> {
    let url = format!("{base_url}/users/{LOCAL_USER_ID}/items/{item_key}");
    let response = shared_client()
        .map_err(ZoteroWriteError::from)?
        .delete(&url)
        .header("Zotero-Server-ID", server_id)
        .header("Zotero-API-Key", api_key)
        .header("If-Unmodified-Since-Version", item_version.to_string())
        .send()
        .await
        .map_err(map_write_send_error)?;
    let status = response.status();
    eprintln!("[zotero] DELETE item {item_key} (version {item_version}) -> {status}");
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        eprintln!("[zotero] DELETE {url} -> {status}: {}", snippet(&body));
        return Err(classify_write_status(status, &body, None));
    }
    Ok(())
}

async fn envelope_from_response(
    response: reqwest::Response,
) -> Result<WriteEnvelope, ZoteroWriteError> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        eprintln!("[zotero] write -> {status}: {}", snippet(&body));
        return Err(classify_write_status(status, &body, retry_after));
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| ZoteroWriteError::Http(format!("Failed to parse write envelope: {e}")))?;
    let envelope = parse_write_envelope(&value);
    eprintln!(
        "[zotero] write -> {status}: {} successful, {} unchanged, {} failed",
        envelope.successful_keys.len(),
        envelope.unchanged_count,
        envelope.failed.len()
    );
    for (index, message) in envelope.failed.iter().take(5) {
        eprintln!("[zotero]   write failed[{index}]: {message}");
    }
    Ok(envelope)
}

/// The 3-phase file upload (stored-file attachments under 4 GB):
/// 1. `POST .../items/<attachmentKey>/file` with md5/filename/filesize/mtime
///    + `If-None-Match: *` -> `{url, uploadKey}` or `{"exists": 1}`.
/// 2. POST the bytes to `url` (201).
/// 3. POST the file endpoint again with `upload=<uploadKey>` (204).
///
/// `exists: 1` counts as attached without any byte transfer.
/// Phase-tag a write error: typed errors (KeyExpired, Denied, ...) keep
/// their classification; the generic `Http` fallback gains the upload phase
/// + URL so the Diagnostics entry names exactly which step failed.
fn phase_tag(error: ZoteroWriteError, phase: u8, url: &str) -> ZoteroWriteError {
    match error {
        ZoteroWriteError::Http(message) => {
            ZoteroWriteError::Http(format!("file upload phase {phase} ({url}): {message}"))
        }
        other => other,
    }
}

pub async fn upload_file(
    base_url: &str,
    server_id: &str,
    api_key: &str,
    attachment_key: &str,
    file_bytes: &[u8],
    params: &UploadParams,
) -> Result<UploadAuthorization, ZoteroWriteError> {
    let file_url = format!("{base_url}/users/{LOCAL_USER_ID}/items/{attachment_key}/file");

    // Phase 1: upload authorization.
    let phase1 = shared_client()
        .map_err(ZoteroWriteError::from)?
        .post(&file_url)
        .header("Zotero-Server-ID", server_id)
        .header("Zotero-API-Key", api_key)
        .header("If-None-Match", "*")
        .form(&[
            ("md5", params.md5.as_str()),
            ("filename", params.filename.as_str()),
            ("filesize", &params.filesize.to_string()),
            ("mtime", &params.mtime_ms.to_string()),
        ])
        .send()
        .await
        .map_err(map_write_send_error)?;
    let status = phase1.status();
    let retry_after = phase1
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let body = phase1.text().await.unwrap_or_default();
    eprintln!(
        "[zotero] upload auth {attachment_key} ({}, {}B, md5 {}) -> {status}",
        params.filename, params.filesize, params.md5
    );
    if !status.is_success() {
        eprintln!("[zotero] upload auth {file_url} -> {status}: {}", snippet(&body));
        return Err(phase_tag(classify_write_status(status, &body, retry_after), 1, &file_url));
    }
    let authorization = parse_upload_authorization(&body).ok_or_else(|| {
        ZoteroWriteError::Http(format!("Unexpected upload authorization: {body}"))
    })?;
    let UploadAuthorization::Upload { url, upload_key } = authorization else {
        eprintln!("[zotero] upload auth {attachment_key}: exists=1 (already uploaded)");
        return Ok(UploadAuthorization::Exists);
    };

    // Phase 2: upload the bytes to the authorized URL.
    let phase2 = shared_client()
        .map_err(ZoteroWriteError::from)?
        .post(&url)
        .header("Zotero-Server-ID", server_id)
        .header("Zotero-API-Key", api_key)
        .header("Content-Type", "application/x-zotero-file")
        .body(file_bytes.to_vec())
        .send()
        .await
        .map_err(map_write_send_error)?;
    let phase2_status = phase2.status();
    eprintln!(
        "[zotero] upload bytes {attachment_key} ({}B, application/x-zotero-file) -> {phase2_status}",
        file_bytes.len()
    );
    if !phase2_status.is_success() {
        let body = phase2.text().await.unwrap_or_default();
        eprintln!("[zotero] upload bytes {url} -> {phase2_status}: {}", snippet(&body));
        return Err(phase_tag(classify_write_status(phase2_status, &body, None), 2, &url));
    }

    // Phase 3: register the upload.
    let phase3 = shared_client()
        .map_err(ZoteroWriteError::from)?
        .post(&file_url)
        .header("Zotero-Server-ID", server_id)
        .header("Zotero-API-Key", api_key)
        .form(&[("upload", upload_key.as_str())])
        .send()
        .await
        .map_err(map_write_send_error)?;
    let phase3_status = phase3.status();
    eprintln!("[zotero] register upload {attachment_key} (upload {upload_key}) -> {phase3_status}");
    if !phase3_status.is_success() {
        let body = phase3.text().await.unwrap_or_default();
        eprintln!("[zotero] register upload {file_url} -> {phase3_status}: {}", snippet(&body));
        return Err(phase_tag(classify_write_status(phase3_status, &body, None), 3, &file_url));
    }
    eprintln!(
        "[zotero] upload complete {attachment_key} ({}, {}B)",
        params.filename, params.filesize
    );
    Ok(UploadAuthorization::Upload { url, upload_key })
}

/// MD5 hex digest of the file bytes (upload param).
pub fn md5_hex(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
