//! Zotero local API integration (import wizard + export dialog).
//!
//! Talks only to the local HTTP API of a running Zotero desktop client
//! (`http://localhost:23119/api/`, API version 3). No cloud endpoints, ever.
//! Reads need no authentication; a disabled local API returns `403`, a
//! stopped Zotero refuses connections, and every response carries
//! `Last-Modified-Version` (the library version used as the preview ->
//! import change guard).

pub mod client;
pub mod export_mapping;
pub mod mapping;
pub mod write_client;

use serde::Deserialize;

use crate::error::AppError;

/// Base URL of the Zotero local API (default connector port 23119).
pub const DEFAULT_BASE_URL: &str = "http://localhost:23119/api";

/// The local API only serves the locally logged-in user; any other user id
/// returns `400`, so reads always pin `/users/0/`.
pub const LOCAL_USER_ID: &str = "0";

/// Client errors with a total status mapping: refused/timeout -> `NotRunning`,
/// `403` -> `ApiDisabled`, any other non-success status or decode failure ->
/// `Http`/`Parse` carrying the status code and a body snippet.
#[derive(Debug, thiserror::Error)]
pub enum ZoteroError {
    #[error("Zotero is not running. Start Zotero and try again.")]
    NotRunning,
    #[error(
        "The Zotero local API is disabled. Enable it in Zotero under Settings -> Advanced -> \"Allow other applications on this computer to communicate with Zotero\"."
    )]
    ApiDisabled,
    /// The connector server answered but the local API endpoint was missing
    /// (404 "No endpoint found"): Zotero was not ready yet (startup race or
    /// preference not yet active) or this build has no local API.
    #[error(
        "Could not communicate with Zotero, please make sure your Zotero program is running and you have enabled \"Allow other applications on this computer to communicate with Zotero\" in your advanced Zotero Settings. ({0})"
    )]
    ApiEndpointMissing(u16),
    #[error("Zotero request failed: {0}")]
    Http(String),
    #[error("Failed to parse Zotero response: {0}")]
    Parse(String),
    #[error("Attachment location is not a local file path: {0}")]
    NonFileScheme(String),
}

impl From<ZoteroError> for AppError {
    fn from(e: ZoteroError) -> Self {
        AppError::Import(e.to_string())
    }
}

/// Deserialize `false` (Zotero's "no parent" marker), null, or absence to
/// `None` and a string to `Some`. Used for `data.parentCollection`.
fn false_or_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false)) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(other) => {
            Err(serde::de::Error::custom(format!("expected a string or false, got {other}")))
        }
    }
}

/// A Zotero collection as returned by `GET /users/0/collections` (flat list).
#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroCollection {
    pub key: String,
    #[serde(default)]
    pub version: i64,
    pub data: ZoteroCollectionData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroCollectionData {
    pub name: String,
    /// `false` (or absent) for root collections -> `None`.
    #[serde(rename = "parentCollection", default, deserialize_with = "false_or_string")]
    pub parent_collection: Option<String>,
}

/// A top-level scholarly Zotero item (`/collections/{key}/items`).
#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroItem {
    pub key: String,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub meta: ZoteroItemMeta,
    pub data: ZoteroItemData,
}

/// The `data` object of an item. Every field except `item_type` is optional;
/// unknown fields (Zotero emits many per-type empty strings) are ignored.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroItemData {
    #[serde(rename = "itemType")]
    pub item_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub abstract_note: Option<String>,
    #[serde(default)]
    pub creators: Vec<ZoteroCreator>,
    #[serde(default)]
    pub tags: Vec<ZoteroTag>,
    #[serde(rename = "DOI", default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub publication_title: Option<String>,
    #[serde(default)]
    pub volume: Option<String>,
    #[serde(default)]
    pub issue: Option<String>,
    #[serde(default)]
    pub pages: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(rename = "ISSN", default)]
    pub issn: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub place: Option<String>,
    #[serde(default)]
    pub extra: Option<String>,
    /// Present only on child items (attachments/notes). Top-level items are
    /// partitioned from children by its absence.
    #[serde(default)]
    pub parent_item: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroCreator {
    #[serde(default)]
    pub creator_type: String,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    /// Single-field (institutional) author name, used verbatim.
    #[serde(default)]
    pub name: Option<String>,
}

/// A Zotero tag entry (`type: 1` marks automatic tags).
#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroTag {
    pub tag: String,
    #[serde(rename = "type", default)]
    pub tag_type: Option<i64>,
}

/// A child item (attachment or note) as returned by the bulk
/// `GET /users/0/items?itemType=attachment` request or `/items/{key}/children`.
#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroChildItem {
    pub key: String,
    #[serde(default)]
    pub version: i64,
    pub data: ZoteroChildData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroChildData {
    #[serde(rename = "itemType")]
    pub item_type: String,
    /// The parent item's key; groups attachments to their scholarly item.
    #[serde(default)]
    pub parent_item: Option<String>,
    /// `imported_file` / `linked_file` / `imported_url` / `linked_url`.
    #[serde(default)]
    pub link_mode: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub tags: Vec<ZoteroTag>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroItemMeta {
    /// Full parsed date (`YYYY-MM-DD`, `YYYY-MM`, or `YYYY`) when Zotero can
    /// interpret `data.date`.
    #[serde(default)]
    pub parsed_date: Option<String>,
}

/// A child note item as returned by the bulk `GET /users/0/items?itemType=note`
/// request or `/items/{key}/children`. Notes are plain HTML in `data.note`
/// (first line = display title); `dateAdded` orders the merged Bango text.
#[derive(Debug, Clone, Deserialize)]
pub struct ZoteroNoteItem {
    pub key: String,
    #[serde(default)]
    pub version: i64,
    pub data: ZoteroNoteData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroNoteData {
    #[serde(rename = "itemType")]
    pub item_type: String,
    /// The note body as HTML.
    #[serde(default)]
    pub note: Option<String>,
    /// The parent item's key.
    #[serde(default)]
    pub parent_item: Option<String>,
    #[serde(default)]
    pub tags: Vec<ZoteroTag>,
    /// ISO-8601 creation timestamp (sort key for the merged import).
    #[serde(default)]
    pub date_added: Option<String>,
    #[serde(default)]
    pub date_modified: Option<String>,
}
