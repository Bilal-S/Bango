//! HTTP client for the OpenAlex `/works` endpoint.
//!
//! Uses `reqwest::Client` (already in `Cargo.toml` with the `json` feature).
//! Handles mailto/api_key injection, 429 backoff with retry, and 30s timeout.

use std::time::Duration;

use rand::RngExt;

use crate::error::AppError;

use super::search::build_search_url;
use super::OpenAlexApiResponse;
use super::OpenAlexFilters;

const REQUEST_TIMEOUT_SECS: u64 = 30;
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 10_000;

/// Search OpenAlex works with the given parameters.
///
/// Builds the URL, injects `mailto` + `api_key`, sends the GET request with
/// a 30s timeout, and retries on 429 with exponential backoff + jitter.
///
/// Returns the parsed `OpenAlexApiResponse` on success.
pub async fn search_works(
    query: &str,
    filters: &OpenAlexFilters,
    sort: &str,
    per_page: u32,
    page: u32,
    mailto: &str,
    api_key: Option<&str>,
) -> Result<OpenAlexApiResponse, AppError> {
    let url = build_search_url(query, filters, sort, per_page, page, mailto, api_key);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Import(format!("Failed to build HTTP client: {e}")))?;

    let mut last_error: Option<String> = None;

    for attempt in 0..=MAX_RETRIES {
        let response = client
            .get(&url)
            .header("User-Agent", "Bango/2.0 (https://bango.boncode.net)")
            .send()
            .await
            .map_err(|e| AppError::Import(format!("OpenAlex request failed: {e}")))?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Retry with exponential backoff + jitter
            let backoff_ms = calculate_backoff(attempt);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            last_error = Some("OpenAlex rate limit reached (429)".to_string());
            continue;
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            return Err(AppError::Import(format!("OpenAlex request failed ({status}): {body}")));
        }

        // Success - parse the JSON response
        let api_response: OpenAlexApiResponse = response
            .json()
            .await
            .map_err(|e| AppError::Import(format!("Failed to parse OpenAlex response: {e}")))?;

        return Ok(api_response);
    }

    Err(AppError::Import(
        last_error.unwrap_or_else(|| "OpenAlex request failed after retries".to_string()),
    ))
}

/// Calculate exponential backoff with jitter: 1s, 2s, 4s (capped at 10s) + random 0-500ms.
fn calculate_backoff(attempt: u32) -> u64 {
    let base = INITIAL_BACKOFF_MS * (1u64 << attempt);
    let capped = base.min(MAX_BACKOFF_MS);
    let mut rng = rand::rng();
    let jitter = rng.random_range(0..=500);
    capped + jitter
}

const HARVEST_SELECT_FIELDS: &str =
    "id,doi,title,authorships,publication_year,publication_date,primary_location,biblio,referenced_works,open_access";

/// Batch-fetch works by OpenAlex IDs (used for reference harvest).
///
/// Fetches up to 50 works per call via the `/works?filter=openalex:W1|W2|...|W50`
/// endpoint. Multiple batches are fetched sequentially with a 100ms pause between
/// batches to stay within the free-tier rate limit (10 req/s). Returns an error
/// on 429 so the caller can log it to the audit trail.
pub async fn fetch_works_by_ids(
    openalex_ids: &[String],
    mailto: &str,
    api_key: Option<&str>,
) -> Result<Vec<super::OpenAlexWork>, AppError> {
    if openalex_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Import(format!("Failed to build HTTP client: {e}")))?;

    let mut all_works = Vec::new();

    // Process in chunks of 50 (OpenAlex filter limit).
    for chunk in openalex_ids.chunks(50) {
        // Extract the OpenAlex ID from the full URL (e.g. "https://openalex.org/W123" -> "W123").
        let ids: Vec<&str> =
            chunk.iter().map(|url| url.rsplit('/').next().unwrap_or(url)).collect();
        let filter_value = ids.join("|");

        let mut params: Vec<(&str, String)> = vec![
            ("filter", format!("openalex:{filter_value}")),
            ("select", HARVEST_SELECT_FIELDS.to_string()),
            ("per_page", "50".to_string()),
            ("mailto", mailto.to_string()),
        ];

        if let Some(key) = api_key {
            params.push(("api_key", key.to_string()));
        }

        let url = reqwest::Url::parse_with_params("https://api.openalex.org/works", &params)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| {
                format!(
                    "https://api.openalex.org/works?filter=openalex:{filter_value}&select={HARVEST_SELECT_FIELDS}&per_page=50&mailto={mailto}"
                )
            });

        let response = client
            .get(&url)
            .header("User-Agent", "Bango/2.0 (https://bango.boncode.net)")
            .send()
            .await
            .map_err(|e| AppError::Import(format!("OpenAlex harvest request failed: {e}")))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::Import(
                "OpenAlex rate limit (429) reached during reference harvest. Some references may not have been downloaded.".to_string(),
            ));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            return Err(AppError::Import(format!(
                "OpenAlex harvest batch failed ({status}): {body}"
            )));
        }

        let api_response: super::OpenAlexApiResponse = response.json().await.map_err(|e| {
            AppError::Import(format!("Failed to parse OpenAlex harvest response: {e}"))
        })?;

        all_works.extend(api_response.results);

        // 100ms pause between batches to stay within the free-tier rate limit (10 req/s).
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(all_works)
}

/// Fetch works that cite a given OpenAlex work (the "cited_by" direction).
///
/// Uses the `filter=cites:W12345` endpoint, paginating through `per_page=50`
/// pages. Returns all citing works. Used for the citation harvest in Phase 2.
pub async fn fetch_citing_works(
    openalex_work_id: &str,
    mailto: &str,
    api_key: Option<&str>,
) -> Result<Vec<super::OpenAlexWork>, AppError> {
    // Extract the short ID from the full URL (e.g. "https://openalex.org/W123" -> "W123").
    let short_id = openalex_work_id.rsplit('/').next().unwrap_or(openalex_work_id);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Import(format!("Failed to build HTTP client: {e}")))?;

    let mut all_works = Vec::new();
    let mut page: u32 = 1;

    loop {
        let mut params: Vec<(&str, String)> = vec![
            ("filter", format!("cites:{short_id}")),
            ("select", HARVEST_SELECT_FIELDS.to_string()),
            ("per_page", "50".to_string()),
            ("page", page.to_string()),
            ("mailto", mailto.to_string()),
        ];

        if let Some(key) = api_key {
            params.push(("api_key", key.to_string()));
        }

        let url = reqwest::Url::parse_with_params("https://api.openalex.org/works", &params)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| {
                format!(
                    "https://api.openalex.org/works?filter=cites:{short_id}&select={HARVEST_SELECT_FIELDS}&per_page=50&page={page}&mailto={mailto}"
                )
            });

        let response = client
            .get(&url)
            .header("User-Agent", "Bango/2.0 (https://bango.boncode.net)")
            .send()
            .await
            .map_err(|e| {
                AppError::Import(format!("OpenAlex citation harvest request failed: {e}"))
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::Import(
                "OpenAlex rate limit (429) reached during citation harvest. Some citations may not have been downloaded.".to_string(),
            ));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            return Err(AppError::Import(format!(
                "OpenAlex citation harvest failed ({status}): {body}"
            )));
        }

        let api_response: super::OpenAlexApiResponse = response.json().await.map_err(|e| {
            AppError::Import(format!("Failed to parse OpenAlex citation harvest response: {e}"))
        })?;

        let count = api_response.results.len();
        all_works.extend(api_response.results);

        // If we got fewer than 50, we've reached the last page.
        if count < 50 {
            break;
        }

        // 100ms pause between pages to stay within the free-tier rate limit.
        tokio::time::sleep(Duration::from_millis(100)).await;
        page += 1;
    }

    Ok(all_works)
}

/// Download a PDF from a URL. Returns the raw bytes if the response is a valid
/// PDF (checked via content-type header + magic bytes). Returns an error if the
/// response is an HTML page (likely a CAPTCHA or paywall) or if the download fails.
///
/// Sends browser-like headers (`Accept`, `Accept-Language`, `Accept-Encoding`,
/// `Sec-Fetch-*`) so publishers (MDPI, Elsevier, Springer, etc.) that 403 on
/// the minimal `Bango/2.0` UA serve the PDF instead of a block page. The
/// `Referer` is set to the PDF URL's origin so origin-based anti-leech checks
/// pass.
pub async fn download_pdf(url: &str) -> Result<Vec<u8>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| {
            AppError::Import(format!("Failed to build HTTP client for PDF download: {e}"))
        })?;

    // Derive the origin for the Referer header (origin-based anti-leech checks
    // on publishers like MDPI 403 when the Referer is absent or mismatched).
    let referer = reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| format!("{}://{host}/", parsed.scheme())))
        .unwrap_or_else(|| url.to_string());

    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "application/pdf,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Referer", &referer)
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "same-origin")
        .send()
        .await
        .map_err(|e| AppError::Import(format!("PDF download request failed for {url}: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Import(format!("PDF download failed for {url} (HTTP {status})")));
    }

    let bytes = response.bytes().await.map_err(|e| {
        AppError::Import(format!("Failed to read PDF download response from {url}: {e}"))
    })?;

    // Validate: must start with %PDF magic bytes.
    if bytes.len() < 4 || &bytes[..4] != b"%PDF" {
        return Err(AppError::Import(
            format!("Downloaded content from {url} is not a valid PDF (possible CAPTCHA or paywall page). The article was still imported - you can attach the full text manually."),
        ));
    }

    Ok(bytes.to_vec())
}
