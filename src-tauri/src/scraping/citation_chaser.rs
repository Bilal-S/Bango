//! Citation Chaser scraping: automates RIS download for references/citations.
//!
//! Cancellation, empty-result detection, and the download contract are documented inline.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use headless_chrome::protocol::cdp::Browser::{
    SetDownloadBehavior, SetDownloadBehaviorBehaviorOption,
};
use headless_chrome::{Browser, LaunchOptions, Tab};
use rand::RngExt;

use super::browser::{detect_browser, BrowserError};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors during citation-chaser scraping.
#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("Browser detection failed: {0}")]
    Browser(#[from] BrowserError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Failed to launch browser: {0}")]
    Launch(String),

    #[error("Navigation error: {0}")]
    Navigation(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("Click failed: {0}")]
    ClickFailed(String),

    /// The article has no references/citations in Lens.org. Frontend routes
    /// this as a skip, not an error.
    #[error("No data: {0}")]
    NoData(String),

    /// User cancelled. Browser closed, partial RIS removed.
    #[error("Cancelled")]
    Cancelled,

    /// Download transport failure (TLS, HTTP, reqwest/curl error).
    #[error("Download failed: {0}")]
    Download(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options controlling which data to scrape.
#[derive(Debug, Clone)]
pub struct ScrapeOptions {
    /// Download the **citations** RIS file (articles that cite the given DOI).
    pub get_citations: bool,
    /// Download the **references** RIS file (articles referenced by the given DOI).
    pub get_references: bool,
}

impl Default for ScrapeOptions {
    fn default() -> Self {
        Self { get_citations: true, get_references: true }
    }
}

/// Result of a successful scrape.
#[derive(Debug)]
pub struct ScrapeResult {
    /// Path to the downloaded references RIS file (if `get_references` was `true`).
    pub references_ris: Option<PathBuf>,
    /// Path to the downloaded citations RIS file (if `get_citations` was `true`).
    pub citations_ris: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BASE_URL: &str = "https://estech.shinyapps.io/citationchaser/";

/// Maximum time to wait for an element to appear (seconds).
const ELEMENT_TIMEOUT_SECS: u64 = 120;

/// Poll interval when waiting for downloads or elements (milliseconds).
const POLL_INTERVAL_MS: u64 = 1000;

/// How long to wait after clicking Search for an empty-result / disconnect
/// signal before falling back to [`wait_for_download_enabled`]. Lens resolves
/// empty results quickly (under ~10s live); 20s gives comfortable margin
/// without the 120s hang the zero-reference case used to cause.
const EMPTY_RESULT_TIMEOUT_SECS: u64 = 20;

/// Browser-like `User-Agent` for the reqwest download path. Matches the
/// `openalex::client::download_pdf` pattern so shinyapps.io's CDN treats the
/// request like a desktop browser.
const BROWSER_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/120.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// Cancel token
// ---------------------------------------------------------------------------

/// Shared cancellation flag. Tauri command layer stores the active token in
/// `ScrapingState`; frontend's `cancel_scraping` signals it.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Create a fresh, uncancelled token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` once [`CancelToken::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Signal cancellation. All poll loops polling this token will exit at
    /// their next tick (within `POLL_INTERVAL_MS`).
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Sleep for `dur`, returning `true` early if the token fires. Checks before
/// and after sleep so a cancel is caught within one `POLL_INTERVAL_MS` tick.
fn sleep_or_cancel(cancel: &CancelToken, dur: Duration) -> bool {
    if cancel.is_cancelled() {
        return true;
    }
    thread::sleep(dur);
    cancel.is_cancelled()
}

// ---------------------------------------------------------------------------
// ScrapeKind + empty-result detection
// ---------------------------------------------------------------------------

/// Which Citation Chaser flow is running. Drives the empty-result signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrapeKind {
    References,
    Citations,
}

impl ScrapeKind {
    /// The HTML id of the "Search ..." trigger button.
    const fn find_btn_id(self) -> &'static str {
        match self {
            Self::References => "find_refs",
            Self::Citations => "find_cits",
        }
    }

    /// The HTML id of the download link.
    const fn download_link_id(self) -> &'static str {
        match self {
            Self::References => "refs_ris",
            Self::Citations => "cits_ris",
        }
    }

    /// The filename suffix for the cleaned DOI.
    const fn filename_suffix(self) -> &'static str {
        match self {
            Self::References => "_references.ris",
            Self::Citations => "_citations.ris",
        }
    }

    /// The button's user-facing label (used in log messages).
    const fn search_label(self) -> &'static str {
        match self {
            Self::References => "Search for all referenced articles in Lens.org",
            Self::Citations => "Search for all citing articles in Lens.org",
        }
    }
}

/// Signatures indicating empty result set or disconnected Shiny session.
///
/// `body_text` is `document.body.innerText`. Pure `#[must_use]`.
#[must_use]
fn detect_empty_or_disconnect(body_text: &str, kind: ScrapeKind) -> Option<&'static str> {
    if body_text.contains("Disconnected from the server") {
        return Some("Citation Chaser session disconnected");
    }
    match kind {
        ScrapeKind::References if body_text.contains("had 0 references") => {
            Some("Article has 0 references in Lens.org")
        }
        ScrapeKind::Citations if body_text.contains("no recorded citations in the Lens.org") => {
            Some("Article has 0 recorded citations in Lens.org")
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Sanitise a DOI for use as a cross-platform filename.
pub fn clean_doi_filename(doi: &str) -> String {
    const INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    doi.chars().map(|c| if INVALID.contains(&c) { '_' } else { c }).collect()
}

/// Randomized delay 500-2500ms to mimic human behaviour. Returns `true` if cancelled.
fn human_delay(cancel: &CancelToken) -> bool {
    let mut rng = rand::rng();
    let delay_ms = rng.random_range(500..=2500);
    eprintln!("  ⏳ Waiting {delay_ms}ms (human-like delay)…");
    sleep_or_cancel(cancel, Duration::from_millis(delay_ms))
}

/// Extract the `href` attribute from an element found by its HTML id.
fn get_element_href(tab: &Tab, element_id: &str) -> Result<String, ScrapeError> {
    let js = format!("document.getElementById('{element_id}').href");
    let href = tab
        .evaluate(&js, false)
        .map_err(|e| {
            ScrapeError::ElementNotFound(format!("Failed to get href for #{element_id}: {e}"))
        })?
        .value
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| ScrapeError::ElementNotFound(format!("No href value for #{element_id}")))?;

    Ok(href)
}

/// Read `document.body.innerText` via `tab.evaluate`. Used by the empty-result
/// detector.
fn get_body_text(tab: &Tab) -> Result<String, ScrapeError> {
    let text = tab
        .evaluate("document.body.innerText", false)
        .map_err(|e| ScrapeError::ElementNotFound(format!("Failed to read body text: {e}")))?
        .value
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    Ok(text)
}

/// Download via `curl` (fallback).
fn download_with_curl(
    url: &str,
    output_dir: &Path,
    filename: &str,
) -> Result<PathBuf, ScrapeError> {
    let output_path = output_dir.join(filename);
    eprintln!("  📥 Downloading via curl (fallback): {url}");
    eprintln!("  📁 Saving to: {}", output_path.display());

    let status = std::process::Command::new("curl")
        .args(["-sL", "-o"])
        .arg(&output_path)
        .arg(url)
        .status()
        .map_err(|e| ScrapeError::Io(std::io::Error::other(format!("Failed to run curl: {e}"))))?;

    if !status.success() {
        return Err(ScrapeError::Download(format!(
            "curl exited with status {:?} for {url}",
            status.code()
        )));
    }

    let size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    eprintln!("  ✅ Downloaded: {} ({} bytes)", output_path.display(), size);

    Ok(output_path)
}

/// Download via `reqwest::blocking` (primary path).
///
/// Safe because called from `spawn_blocking` (tokio blocking pool), not an async worker.
fn download_with_reqwest(
    url: &str,
    output_dir: &Path,
    filename: &str,
) -> Result<PathBuf, ScrapeError> {
    let output_path = output_dir.join(filename);
    eprintln!("  📥 Downloading via reqwest: {url}");

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| ScrapeError::Download(format!("reqwest client build failed: {e}")))?;

    let response = client
        .get(url)
        .header("User-Agent", BROWSER_UA)
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Referer", BASE_URL)
        .send()
        .map_err(|e| ScrapeError::Download(format!("reqwest request failed for {url}: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(ScrapeError::Download(format!("HTTP {status} for {url}")));
    }

    let bytes = response
        .bytes()
        .map_err(|e| ScrapeError::Download(format!("reqwest body read failed for {url}: {e}")))?;

    fs::write(&output_path, &bytes)?;
    eprintln!("  ✅ Downloaded: {} ({} bytes)", output_path.display(), bytes.len());

    Ok(output_path)
}

/// Download `url` to `output_dir/filename`. Tries reqwest first, falls back to curl.
fn download_file(url: &str, output_dir: &Path, filename: &str) -> Result<PathBuf, ScrapeError> {
    match download_with_reqwest(url, output_dir, filename) {
        Ok(path) => Ok(path),
        Err(reqwest_err) => match download_with_curl(url, output_dir, filename) {
            Ok(path) => Ok(path),
            Err(curl_err) => Err(ScrapeError::Download(format!(
                "{reqwest_err}; curl fallback also failed: {curl_err}"
            ))),
        },
    }
}

/// Validate downloaded RIS is non-empty and contains `TY  -`. On failure,
/// removes the file (so the existence-shortcut does not cache it) and returns
/// [`ScrapeError::NoData`].
fn validate_ris_nonempty(path: &Path) -> Result<(), ScrapeError> {
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if bytes == 0 {
        let _ = fs::remove_file(path);
        return Err(ScrapeError::NoData("Downloaded RIS is empty".to_string()));
    }
    let text = fs::read_to_string(path)?;
    if !text.contains("TY  -") {
        let _ = fs::remove_file(path);
        return Err(ScrapeError::NoData("Downloaded RIS has no TY records".to_string()));
    }
    Ok(())
}

/// Poll for an XPath element, click when found. Checks cancel token each iteration.
fn click_xpath_with_retry(
    tab: &Tab,
    cancel: &CancelToken,
    xpath: &str,
    description: &str,
) -> Result<(), ScrapeError> {
    eprintln!("  🔍 Looking for element: {description}");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return Err(ScrapeError::Cancelled);
        }
        match tab.find_element_by_xpath(xpath) {
            Ok(element) => {
                eprintln!("  ✅ Found element, clicking: {description}");
                element
                    .click()
                    .map_err(|e| ScrapeError::ClickFailed(format!("{description}: {e}")))?;
                return Ok(());
            }
            Err(_) => {
                let elapsed = (Instant::now() - deadline
                    + Duration::from_secs(ELEMENT_TIMEOUT_SECS))
                .as_secs();
                if elapsed / 5 > logged_at / 5 {
                    eprintln!("  ⏳ Still looking for '{description}'… {elapsed}s elapsed");
                    logged_at = elapsed;
                }
                if sleep_or_cancel(cancel, Duration::from_millis(POLL_INTERVAL_MS)) {
                    return Err(ScrapeError::Cancelled);
                }
            }
        }
    }

    Err(ScrapeError::ElementNotFound(format!(
        "{description} (xpath={xpath}) not found within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

/// Wait for an element (no click). Checks cancel token each iteration.
fn wait_for_element(
    tab: &Tab,
    cancel: &CancelToken,
    xpath: &str,
    description: &str,
) -> Result<(), ScrapeError> {
    eprintln!("  🔍 Waiting for element: {description}");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return Err(ScrapeError::Cancelled);
        }
        match tab.find_element_by_xpath(xpath) {
            Ok(_) => {
                eprintln!("  ✅ Found element: {description}");
                return Ok(());
            }
            Err(_) => {
                let elapsed = (Instant::now() - deadline
                    + Duration::from_secs(ELEMENT_TIMEOUT_SECS))
                .as_secs();
                if elapsed / 5 > logged_at / 5 {
                    eprintln!("  ⏳ Still waiting for '{description}'… {elapsed}s elapsed");
                    logged_at = elapsed;
                }
                if sleep_or_cancel(cancel, Duration::from_millis(POLL_INTERVAL_MS)) {
                    return Err(ScrapeError::Cancelled);
                }
            }
        }
    }

    Err(ScrapeError::ElementNotFound(format!(
        "{description} (xpath={xpath}) not found within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

/// Wait for Shiny download link `disabled` class to be removed.
fn wait_for_download_enabled(
    tab: &Tab,
    cancel: &CancelToken,
    element_id: &str,
) -> Result<(), ScrapeError> {
    eprintln!("  📊 Waiting for download link #{element_id} to become enabled…");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;
    let xpath = format!("//*[@id='{element_id}']");

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return Err(ScrapeError::Cancelled);
        }
        match tab.find_element_by_xpath(&xpath) {
            Ok(element) => {
                // Check if the element still has the "disabled" class.
                let classes =
                    element.get_attributes().unwrap_or_default().unwrap_or_default().join(" ");
                if !classes.contains("disabled") {
                    eprintln!("  ✅ Download link #{element_id} is enabled");
                    return Ok(());
                }
                let elapsed = (Instant::now() - deadline
                    + Duration::from_secs(ELEMENT_TIMEOUT_SECS))
                .as_secs();
                if elapsed / 5 > logged_at / 5 {
                    eprintln!("  ⏳ Download link still disabled… {elapsed}s elapsed");
                    logged_at = elapsed;
                }
            }
            Err(_) => {
                let elapsed = (Instant::now() - deadline
                    + Duration::from_secs(ELEMENT_TIMEOUT_SECS))
                .as_secs();
                if elapsed / 5 > logged_at / 5 {
                    eprintln!("  ⏳ Download link #{element_id} not found yet… {elapsed}s elapsed");
                    logged_at = elapsed;
                }
            }
        }
        if sleep_or_cancel(cancel, Duration::from_millis(POLL_INTERVAL_MS)) {
            return Err(ScrapeError::Cancelled);
        }
    }

    Err(ScrapeError::ElementNotFound(format!(
        "Download link #{element_id} did not become enabled within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

/// After clicking Search, poll `document.body.innerText` for empty-result /
/// disconnect signatures. Returns `NoData` on match, `Ok(())` on timeout.
///
/// Fix for zero-references 120s hang: Lens resolves empty results in <20s.
fn wait_for_empty_or_disconnect(
    tab: &Tab,
    cancel: &CancelToken,
    kind: ScrapeKind,
) -> Result<(), ScrapeError> {
    eprintln!("  🔎 Watching for empty-result / disconnect signals…");
    let deadline = Instant::now() + Duration::from_secs(EMPTY_RESULT_TIMEOUT_SECS);

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return Err(ScrapeError::Cancelled);
        }
        if let Ok(body_text) = get_body_text(tab) {
            if let Some(reason) = detect_empty_or_disconnect(&body_text, kind) {
                eprintln!("  ⚠️  Empty-result signal: {reason}");
                return Err(ScrapeError::NoData(reason.to_string()));
            }
        }
        if sleep_or_cancel(cancel, Duration::from_millis(POLL_INTERVAL_MS)) {
            return Err(ScrapeError::Cancelled);
        }
    }

    eprintln!("  ✅ No empty-result signal within {EMPTY_RESULT_TIMEOUT_SECS}s; proceeding");
    Ok(())
}

// ---------------------------------------------------------------------------
// Core scraping flow
// ---------------------------------------------------------------------------

/// Scrape Citation Chaser for the given DOI.
///
/// # Errors
///
/// Returns [`ScrapeError::Browser`] if Chromium not installed.
/// Returns [`ScrapeError::Validation`] if both get_citations/get_references are false.
/// Returns [`ScrapeError::NoData`] if article has no references/citations.
/// Returns [`ScrapeError::Cancelled`] if cancel token fires.
pub fn scrape_citation_chaser(
    doi: &str,
    output_dir: &Path,
    options: &ScrapeOptions,
    cancel: &CancelToken,
) -> Result<ScrapeResult, ScrapeError> {
    if !options.get_citations && !options.get_references {
        return Err(ScrapeError::Validation(
            "At least one of get_citations or get_references must be true".to_string(),
        ));
    }

    // Detect browser ──────────────────────────────────────────────────────
    eprintln!("🔍 Detecting browser…");
    let browser_info = detect_browser()?;
    eprintln!("✅ Found browser: {}", browser_info.executable.display());

    fs::create_dir_all(output_dir)?;
    eprintln!("📁 Output directory: {}", output_dir.display());

    eprintln!("🚀 Launching headless browser…");
    let browser = Browser::new(LaunchOptions {
        headless: true,
        sandbox: false,
        path: Some(browser_info.executable),
        args: vec![OsStr::new("--disable-gpu"), OsStr::new("--disable-dev-shm-usage")],
        ..Default::default()
    })
    .map_err(|e| ScrapeError::Launch(format!("Failed to launch browser: {e}")))?;

    let result = run_scrape(&browser, doi, output_dir, options, cancel);

    // Always close the browser, even on error/cancel.
    eprintln!("🧹 Closing browser…");
    if let Ok(tab) = browser.new_tab() {
        let _ = tab.close(true);
    }

    // On cancel, remove partial RIS files so existence-shortcut doesn't cache them.
    if matches!(result, Err(ScrapeError::Cancelled)) {
        let safe_doi = clean_doi_filename(doi);
        if options.get_references {
            let p = output_dir.join(format!("{safe_doi}_references.ris"));
            let _ = fs::remove_file(p);
        }
        if options.get_citations {
            let p = output_dir.join(format!("{safe_doi}_citations.ris"));
            let _ = fs::remove_file(p);
        }
    }

    result
}

/// Inner scrape loop, separated so outer always closes the browser.
fn run_scrape(
    browser: &Browser,
    doi: &str,
    output_dir: &Path,
    options: &ScrapeOptions,
    cancel: &CancelToken,
) -> Result<ScrapeResult, ScrapeError> {
    let tab =
        browser.new_tab().map_err(|e| ScrapeError::Launch(format!("Failed to create tab: {e}")))?;
    eprintln!("✅ Browser tab created");

    tab.set_default_timeout(Duration::from_secs(ELEMENT_TIMEOUT_SECS));

    eprintln!("⚙️  Configuring downloads to: {}", output_dir.display());
    tab.call_method(SetDownloadBehavior {
        behavior: SetDownloadBehaviorBehaviorOption::Allow,
        download_path: Some(output_dir.to_string_lossy().to_string()),
        browser_context_id: None,
        events_enabled: Some(true),
    })
    .map_err(|e| ScrapeError::Launch(format!("Failed to set download behavior: {e}")))?;

    // Navigate to Citation Chaser
    let url = format!("{BASE_URL}?dois={doi}");
    eprintln!("🌐 Navigating to: {url}");
    tab.navigate_to(&url)
        .map_err(|e| ScrapeError::Navigation(format!("Failed to navigate to {url}: {e}")))?;
    eprintln!("✅ Page loaded");

    // Wait for Shiny app to render.
    wait_for_element(&tab, cancel, "//a[contains(text(), 'References')]", "Shiny app nav tabs")?;
    eprintln!("✅ Shiny app is ready");

    let mut references_ris: Option<PathBuf> = None;
    let mut citations_ris: Option<PathBuf> = None;

    // References flow
    if options.get_references {
        references_ris = Some(scrape_flow(&tab, cancel, doi, output_dir, ScrapeKind::References)?);
        if let Some(ref path) = references_ris {
            eprintln!("✅ References RIS saved: {}", path.display());
        }
        if human_delay(cancel) {
            eprintln!("  ⚠️  Cancel detected after references flow");
        }
    }

    // Citations flow
    if options.get_citations {
        // Don't start if references cancelled.
        if cancel.is_cancelled() {
            return Err(ScrapeError::Cancelled);
        }
        eprintln!("====== CITATIONS flow =======");
        citations_ris = Some(scrape_flow(&tab, cancel, doi, output_dir, ScrapeKind::Citations)?);
        if let Some(ref path) = citations_ris {
            eprintln!("✅ Citations RIS saved: {}", path.display());
        }
    }

    Ok(ScrapeResult { references_ris, citations_ris })
}

/// Run one Citation Chaser flow (references or citations).
///
/// 1. Click tab  2. Click Search  3. Watch for empty/disconnect  4. Wait for
///    download link  5. Extract href, download, validate
fn scrape_flow(
    tab: &Tab,
    cancel: &CancelToken,
    doi: &str,
    output_dir: &Path,
    kind: ScrapeKind,
) -> Result<PathBuf, ScrapeError> {
    let tab_label = match kind {
        ScrapeKind::References => "References",
        ScrapeKind::Citations => "Citations",
    };

    // 1. Click tab.
    click_xpath_with_retry(
        tab,
        cancel,
        &format!("//a[contains(text(), '{tab_label}')]"),
        &format!("Click {tab_label} tab"),
    )?;
    if human_delay(cancel) {
        return Err(ScrapeError::Cancelled);
    }

    // 2. Click search trigger button.
    click_xpath_with_retry(
        tab,
        cancel,
        &format!("//*[@id='{}']", kind.find_btn_id()),
        &format!("Click '{}' button", kind.search_label()),
    )?;

    // 3. Watch for empty-result / disconnect. Short-circuit with NoData instead
    // of polling the download link for 120s.
    wait_for_empty_or_disconnect(tab, cancel, kind)?;

    // 4. Wait for download link enabled.
    wait_for_download_enabled(tab, cancel, kind.download_link_id())?;

    // 5. Extract download URL.
    let href = get_element_href(tab, kind.download_link_id())?;
    eprintln!("  🔗 Download URL: {href}");

    let filename = format!("{}{}", clean_doi_filename(doi), kind.filename_suffix());

    // Check cancel before the non-interruptible download call.
    if cancel.is_cancelled() {
        return Err(ScrapeError::Cancelled);
    }
    let path = download_file(&href, output_dir, &filename)?;

    // 6. Defense-in-depth: zero-citations serves 0-byte file with valid href.
    validate_ris_nonempty(&path)?;

    Ok(path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_doi_filename_replaces_invalid_chars() {
        assert_eq!(clean_doi_filename("10.1016/j.jaad.2023.01.013"), "10.1016_j.jaad.2023.01.013");
        assert_eq!(clean_doi_filename("10.1002/csr.70574"), "10.1002_csr.70574");
        // Already-clean DOIs are unchanged.
        assert_eq!(
            clean_doi_filename("10.1371_journal.pmed.1004371"),
            "10.1371_journal.pmed.1004371"
        );
    }

    #[test]
    fn detect_empty_or_disconnect_zero_references() {
        let body = "...\nYour input article(s) had 0 references and unique records after deduplication\n...";
        assert_eq!(
            detect_empty_or_disconnect(body, ScrapeKind::References),
            Some("Article has 0 references in Lens.org")
        );
        // Citations kind must NOT match the references signature.
        assert_eq!(detect_empty_or_disconnect(body, ScrapeKind::Citations), None);
    }

    #[test]
    fn detect_empty_or_disconnect_zero_citations() {
        let body = "...\nWarning: Your input articles have no recorded citations in the Lens.org database\n...";
        assert_eq!(
            detect_empty_or_disconnect(body, ScrapeKind::Citations),
            Some("Article has 0 recorded citations in Lens.org")
        );
        // References kind must NOT match the citations signature.
        assert_eq!(detect_empty_or_disconnect(body, ScrapeKind::References), None);
    }

    #[test]
    fn detect_empty_or_disconnect_disconnected_wins_on_both_kinds() {
        let body = "Disconnected from the server. Reload";
        assert_eq!(
            detect_empty_or_disconnect(body, ScrapeKind::References),
            Some("Citation Chaser session disconnected")
        );
        assert_eq!(
            detect_empty_or_disconnect(body, ScrapeKind::Citations),
            Some("Citation Chaser session disconnected")
        );
    }

    #[test]
    fn detect_empty_or_disconnect_normal_body_is_none() {
        let body = "Once you have loaded your input articles, you can search for all referenced articles across them.";
        assert_eq!(detect_empty_or_disconnect(body, ScrapeKind::References), None);
        assert_eq!(detect_empty_or_disconnect(body, ScrapeKind::Citations), None);
    }

    #[test]
    fn detect_empty_or_disconnect_empty_body_is_none() {
        assert_eq!(detect_empty_or_disconnect("", ScrapeKind::References), None);
        assert_eq!(detect_empty_or_disconnect("", ScrapeKind::Citations), None);
    }

    #[test]
    fn cancel_token_initial_state_is_false() {
        let t = CancelToken::new();
        assert!(!t.is_cancelled());
    }

    #[test]
    fn cancel_token_cancel_flips_flag() {
        let t = CancelToken::new();
        assert!(!t.is_cancelled());
        t.cancel();
        assert!(t.is_cancelled());
    }

    #[test]
    fn cancel_token_clone_shares_state() {
        let t = CancelToken::new();
        let t2 = t.clone();
        assert!(!t2.is_cancelled());
        t.cancel();
        assert!(t2.is_cancelled(), "clone must reflect the cancel signal");
    }

    #[test]
    fn sleep_or_cancel_returns_true_when_already_cancelled() {
        let t = CancelToken::new();
        t.cancel();
        // Should return immediately (true) without sleeping.
        let start = Instant::now();
        let fired = sleep_or_cancel(&t, Duration::from_millis(50));
        assert!(fired);
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn validate_ris_nonempty_rejects_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.ris");
        std::fs::write(&path, b"").expect("write");
        let err = validate_ris_nonempty(&path).unwrap_err();
        assert!(matches!(err, ScrapeError::NoData(_)));
        assert!(!path.exists(), "empty RIS should be removed");
    }

    #[test]
    fn validate_ris_nonempty_rejects_no_ty_tag() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("notris.ris");
        std::fs::write(&path, b"this is not an RIS file").expect("write");
        let err = validate_ris_nonempty(&path).unwrap_err();
        assert!(matches!(err, ScrapeError::NoData(_)));
        assert!(!path.exists(), "invalid RIS should be removed");
    }

    #[test]
    fn validate_ris_nonempty_accepts_valid_ris() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("valid.ris");
        let body = "TY  - JOUR\nTI  - Test\nER  -\n";
        std::fs::write(&path, body).expect("write");
        validate_ris_nonempty(&path).expect("valid RIS should pass");
        assert!(path.exists(), "valid RIS should be kept");
    }

    #[test]
    fn scrape_kind_helpers_are_consistent() {
        assert_eq!(ScrapeKind::References.find_btn_id(), "find_refs");
        assert_eq!(ScrapeKind::Citations.find_btn_id(), "find_cits");
        assert_eq!(ScrapeKind::References.download_link_id(), "refs_ris");
        assert_eq!(ScrapeKind::Citations.download_link_id(), "cits_ris");
        assert_eq!(ScrapeKind::References.filename_suffix(), "_references.ris");
        assert_eq!(ScrapeKind::Citations.filename_suffix(), "_citations.ris");
    }

    #[test]
    fn validation_error_when_both_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let options = ScrapeOptions { get_citations: false, get_references: false };
        let cancel = CancelToken::new();
        let result = scrape_citation_chaser("10.1234/anything", dir.path(), &options, &cancel);
        let err = result.unwrap_err();
        assert!(matches!(err, ScrapeError::Validation(_)));
    }
}
