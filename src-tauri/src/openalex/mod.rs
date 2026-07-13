//! OpenAlex search integration.
//!
//! Provides search, import, and DOI-check capabilities against the OpenAlex
//! open catalog of scholarly works. All HTTP, mapping, and reconstruction
//! live in Rust (consistent with all other external API integration in the
//! codebase). The frontend is a thin renderer.

pub mod client;
pub mod mapping;
pub mod reference_harvest;
pub mod search;
pub mod smart_search;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// An OpenAlex Work as returned by the `/works` endpoint (subset of fields
/// selected via the `select=` query param).
///
/// Uses `camelCase` for serialization to the frontend (matching the TS
/// interfaces) with `alias` for deserialization from the OpenAlex API's
/// snake_case JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexWork {
    pub id: String,
    pub doi: Option<String>,
    pub title: Option<String>,
    #[serde(alias = "publication_year")]
    pub publication_year: Option<i32>,
    #[serde(alias = "publication_date")]
    pub publication_date: Option<String>,
    pub authorships: Vec<OpenAlexAuthorship>,
    #[serde(alias = "primary_location")]
    pub primary_location: Option<OpenAlexPrimaryLocation>,
    #[serde(alias = "abstract_inverted_index")]
    pub abstract_inverted_index: Option<HashMap<String, Vec<i32>>>,
    pub biblio: Option<OpenAlexBiblio>,
    #[serde(alias = "cited_by_count")]
    pub cited_by_count: i32,
    pub language: Option<String>,
    pub keywords: Vec<OpenAlexKeyword>,
    #[serde(rename = "type", alias = "type")]
    pub work_type: Option<String>,
    #[serde(alias = "open_access")]
    pub open_access: Option<OpenAlexOpenAccess>,
    #[serde(alias = "is_retracted")]
    pub is_retracted: Option<bool>,
    /// Present only when fetched via the import path with
    /// `retrieve_reference_details` enabled. Search results deliberately
    /// exclude this field to keep payloads small.
    #[serde(default, alias = "referenced_works")]
    pub referenced_works: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexAuthorship {
    #[serde(alias = "author_position")]
    pub author_position: Option<String>,
    pub author: OpenAlexAuthor,
    pub institutions: Vec<OpenAlexInstitution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexAuthor {
    #[serde(alias = "display_name")]
    pub display_name: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexInstitution {
    #[serde(alias = "display_name")]
    pub display_name: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexPrimaryLocation {
    pub source: Option<OpenAlexSource>,
    #[serde(alias = "landing_page_url")]
    pub landing_page_url: Option<String>,
    #[serde(alias = "pdf_url")]
    pub pdf_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexSource {
    #[serde(alias = "display_name")]
    pub display_name: Option<String>,
    #[serde(alias = "issn_l")]
    pub issn_l: Option<String>,
    pub issn: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexBiblio {
    pub volume: Option<String>,
    pub issue: Option<String>,
    #[serde(alias = "first_page")]
    pub first_page: Option<String>,
    #[serde(alias = "last_page")]
    pub last_page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexKeyword {
    #[serde(alias = "display_name")]
    pub display_name: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexOpenAccess {
    #[serde(alias = "is_oa")]
    pub is_oa: Option<bool>,
    #[serde(alias = "oa_status")]
    pub oa_status: Option<String>,
    #[serde(alias = "oa_url")]
    pub oa_url: Option<String>,
}

/// The full search response from `/works`.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAlexApiResponse {
    pub meta: OpenAlexMeta,
    pub results: Vec<OpenAlexWork>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAlexMeta {
    pub count: i64,
    pub page: i64,
    pub per_page: i64,
}

/// A single search result item with reconstructed abstract + snippet +
/// library-membership flag. This is what the frontend renders.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexResultItem {
    pub work: OpenAlexWork,
    /// Reconstructed from `abstract_inverted_index`.
    pub abstract_text: String,
    /// 200-char word-boundary-truncated snippet.
    pub snippet: String,
    /// True if the work's DOI matches an existing article in the library.
    pub already_in_library: bool,
}

/// The search command's return type sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexSearchResponse {
    pub results: Vec<OpenAlexResultItem>,
    pub total_count: i64,
    pub page: i64,
    pub per_page: i64,
}

/// User-facing search parameters.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexSearchParams {
    pub query: String,
    #[serde(default)]
    pub filters: OpenAlexFilters,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    #[serde(default = "default_page")]
    pub page: u32,
}

fn default_sort() -> String {
    "relevance_score:desc".to_string()
}

fn default_per_page() -> u32 {
    25
}

fn default_page() -> u32 {
    1
}

/// User-controllable filters. `has_abstract:true` is always-on and not
/// represented here; `is_retracted:false` is default-on and removed only
/// when `show_retracted` is true.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexFilters {
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    #[serde(default)]
    pub work_types: Vec<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub is_oa: bool,
    #[serde(default)]
    pub show_retracted: bool,
}

/// Read the `openalex_api_key` from app_settings, decrypting if present.
/// Returns `None` if not set.
pub fn get_api_key(conn: &rusqlite::Connection) -> Result<Option<String>, AppError> {
    let encrypted = crate::db::app_settings_repo::get_setting(conn, "openalex_api_key")?;
    match encrypted {
        Some(enc) => {
            let key = crate::crypto::aes_gcm::derive_key_from_machine();
            let decrypted = crate::crypto::aes_gcm::decrypt(&enc, &key)
                .map_err(|_| AppError::Validation("Failed to decrypt OpenAlex API key".into()))?;
            let plaintext = String::from_utf8(decrypted)
                .map_err(|_| AppError::Validation("OpenAlex API key is not valid UTF-8".into()))?;
            if plaintext.is_empty() {
                Ok(None)
            } else {
                Ok(Some(plaintext))
            }
        }
        None => Ok(None),
    }
}

/// Encrypt and store the `openalex_api_key` in app_settings.
/// Pass `None` or an empty string to clear it.
pub fn set_api_key(conn: &rusqlite::Connection, key: Option<&str>) -> Result<(), AppError> {
    let value = match key {
        Some(k) if !k.is_empty() => {
            let machine_key = crate::crypto::aes_gcm::derive_key_from_machine();
            let encrypted = crate::crypto::aes_gcm::encrypt(k.as_bytes(), &machine_key)
                .map_err(|_| AppError::Validation("Failed to encrypt OpenAlex API key".into()))?;
            Some(encrypted)
        }
        _ => None,
    };
    crate::db::app_settings_repo::set_setting(conn, "openalex_api_key", value.as_deref())
}

/// Read the `openalex_mailto` from app_settings, falling back to the app
/// default so we always get the polite-pool rate limit.
pub fn get_mailto(conn: &rusqlite::Connection) -> Result<String, AppError> {
    let mailto = crate::db::app_settings_repo::get_setting(conn, "openalex_mailto")?;
    Ok(mailto.filter(|s| !s.is_empty()).unwrap_or_else(|| "research@bango.app".to_string()))
}
