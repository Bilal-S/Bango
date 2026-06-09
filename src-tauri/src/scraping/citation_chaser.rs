//! Scraping integration with [Citation Chaser](https://estech.shinyapps.io/citationchaser/).
//!
//! Given a DOI, automates the Citation Chaser Shiny app to download RIS files
//! containing the article's **references** and/or **citations**.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Errors that can occur during citation-chaser scraping.
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

    #[error("Download timeout: {0}")]
    DownloadTimeout(String),

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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Sanitise a DOI string so it can be used as a cross-platform file name.
///
/// Replaces characters that are invalid in filenames (`/`, `\`, `:`, `*`, `?`,
/// `"`, `<`, `>`, `|`) with `_`.
///
/// ```
/// use bango_lib::scraping::citation_chaser::clean_doi_filename;
/// assert_eq!(clean_doi_filename("10.1016/j.jaad.2023.01.013"), "10.1016_j.jaad.2023.01.013");
/// ```
pub fn clean_doi_filename(doi: &str) -> String {
    const INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    doi.chars().map(|c| if INVALID.contains(&c) { '_' } else { c }).collect()
}

/// Randomized delay between 500 ms and 2500 ms to mimic human behaviour.
fn human_delay() {
    let mut rng = rand::rng();
    let delay_ms = rng.random_range(500..=2500);
    eprintln!("  ⏳ Waiting {delay_ms}ms (human-like delay)…");
    thread::sleep(Duration::from_millis(delay_ms));
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

/// Download a file from `url` to `output_dir` using `curl`.
fn download_with_curl(
    url: &str,
    output_dir: &Path,
    filename: &str,
) -> Result<PathBuf, ScrapeError> {
    let output_path = output_dir.join(filename);
    eprintln!("  📥 Downloading via curl: {url}");
    eprintln!("  📁 Saving to: {}", output_path.display());

    let status = std::process::Command::new("curl")
        .args([
            "-sL", // silent, follow redirects
            "-o",
        ])
        .arg(&output_path)
        .arg(url)
        .status()
        .map_err(|e| ScrapeError::Io(std::io::Error::other(format!("Failed to run curl: {e}"))))?;

    if !status.success() {
        return Err(ScrapeError::DownloadTimeout(format!(
            "curl exited with status {:?} for {url}",
            status.code()
        )));
    }

    let size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    eprintln!("  ✅ Downloaded: {} ({} bytes)", output_path.display(), size);

    Ok(output_path)
}

/// Poll until an XPath element is found, clicking it once it appears.
fn click_xpath_with_retry(tab: &Tab, xpath: &str, description: &str) -> Result<(), ScrapeError> {
    eprintln!("  🔍 Looking for element: {description}");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;

    while Instant::now() < deadline {
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
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }
    }

    Err(ScrapeError::ElementNotFound(format!(
        "{description} (xpath={xpath}) not found within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

/// Wait for an element to appear (without clicking it).
fn wait_for_element(tab: &Tab, xpath: &str, description: &str) -> Result<(), ScrapeError> {
    eprintln!("  🔍 Waiting for element: {description}");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;

    while Instant::now() < deadline {
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
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }
    }

    Err(ScrapeError::ElementNotFound(format!(
        "{description} (xpath={xpath}) not found within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

/// Wait for a Shiny download link to become enabled (i.e. the `disabled` class is removed).
///
/// Citation Chaser uses `<a id="refs_ris">` and `<a id="cits_ris">` download links that
/// start with a `disabled` class. Once data is fetched, the class is removed and the
/// `href` is updated with a real download URL.
fn wait_for_download_enabled(tab: &Tab, element_id: &str) -> Result<(), ScrapeError> {
    eprintln!("  📊 Waiting for download link #{element_id} to become enabled…");
    let deadline = Instant::now() + Duration::from_secs(ELEMENT_TIMEOUT_SECS);
    let mut logged_at: u64 = 0;
    let xpath = format!("//*[@id='{element_id}']");

    while Instant::now() < deadline {
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
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }

    Err(ScrapeError::ElementNotFound(format!(
        "Download link #{element_id} did not become enabled within {ELEMENT_TIMEOUT_SECS}s"
    )))
}

// ---------------------------------------------------------------------------
// Core scraping flow
// ---------------------------------------------------------------------------

/// Scrape Citation Chaser for the given DOI.
///
/// ```text
/// let result = scrape_citation_chaser("10.1234/example", &output_dir, &ScrapeOptions::default())?;
/// println!("References: {:?}", result.references_ris);
/// println!("Citations:  {:?}", result.citations_ris);
/// ```
///
/// # Errors
///
/// Returns [`ScrapeError::Browser`] if Chrome/Chromium is not installed.
/// Returns [`ScrapeError::Validation`] if both `get_citations` and `get_references` are `false`.
pub fn scrape_citation_chaser(
    doi: &str,
    output_dir: &Path,
    options: &ScrapeOptions,
) -> Result<ScrapeResult, ScrapeError> {
    // ── Validate options ───────────────────────────────────────────────
    if !options.get_citations && !options.get_references {
        return Err(ScrapeError::Validation(
            "At least one of get_citations or get_references must be true".to_string(),
        ));
    }

    // ── Detect browser ─────────────────────────────────────────────────
    eprintln!("🔍 Detecting browser…");
    let browser_info = detect_browser()?;
    eprintln!("✅ Found browser: {}", browser_info.executable.display());

    // ── Ensure output directory exists ─────────────────────────────────
    fs::create_dir_all(output_dir)?;
    eprintln!("📁 Output directory: {}", output_dir.display());

    // ── Launch headless Chrome ─────────────────────────────────────────
    eprintln!("🚀 Launching headless browser…");
    let browser = Browser::new(LaunchOptions {
        headless: true,
        sandbox: false,
        path: Some(browser_info.executable),
        args: vec![OsStr::new("--disable-gpu"), OsStr::new("--disable-dev-shm-usage")],
        ..Default::default()
    })
    .map_err(|e| ScrapeError::Launch(format!("Failed to launch browser: {e}")))?;

    let tab =
        browser.new_tab().map_err(|e| ScrapeError::Launch(format!("Failed to create tab: {e}")))?;
    eprintln!("✅ Browser tab created");

    // Configure downloads to go to our output directory.
    tab.set_default_timeout(Duration::from_secs(ELEMENT_TIMEOUT_SECS));

    eprintln!("⚙️  Configuring downloads to: {}", output_dir.display());
    tab.call_method(SetDownloadBehavior {
        behavior: SetDownloadBehaviorBehaviorOption::Allow,
        download_path: Some(output_dir.to_string_lossy().to_string()),
        browser_context_id: None,
        events_enabled: Some(true),
    })
    .map_err(|e| ScrapeError::Launch(format!("Failed to set download behavior: {e}")))?;

    // ── Navigate to Citation Chaser ────────────────────────────────────
    let url = format!("{BASE_URL}?dois={doi}");
    eprintln!("🌐 Navigating to: {url}");
    tab.navigate_to(&url)
        .map_err(|e| ScrapeError::Navigation(format!("Failed to navigate to {url}: {e}")))?;
    eprintln!("✅ Page loaded");

    // Wait for the Shiny app to fully render (watch for nav tabs).
    wait_for_element(&tab, "//a[contains(text(), 'References')]", "Shiny app nav tabs")?;
    eprintln!("✅ Shiny app is ready");

    let mut references_ris: Option<PathBuf> = None;
    let mut citations_ris: Option<PathBuf> = None;

    // ── References flow ────────────────────────────────────────────────
    if options.get_references {
        eprintln!("═══════════════════════════════════════");
        eprintln!("📥 Starting REFERENCES flow");
        eprintln!("═══════════════════════════════════════");
        references_ris = Some(scrape_references(&tab, doi, output_dir)?);
        if let Some(ref path) = references_ris {
            eprintln!("✅ References RIS saved: {}", path.display());
        }
        human_delay();
    }

    // ── Citations flow ─────────────────────────────────────────────────
    if options.get_citations {
        eprintln!("═══════════════════════════════════════");
        eprintln!("📥 Starting CITATIONS flow");
        eprintln!("═══════════════════════════════════════");
        citations_ris = Some(scrape_citations(&tab, doi, output_dir)?);
        if let Some(ref path) = citations_ris {
            eprintln!("✅ Citations RIS saved: {}", path.display());
        }
    }

    // ── Clean up browser ───────────────────────────────────────────────
    eprintln!("🧹 Closing browser…");
    let _ = tab.close(true);
    eprintln!("✅ Done!");

    Ok(ScrapeResult { references_ris, citations_ris })
}

/// Run the "References" flow: click tab → search → wait for download enabled → fetch via curl.
fn scrape_references(tab: &Tab, doi: &str, output_dir: &Path) -> Result<PathBuf, ScrapeError> {
    // 1. Click the "References" tab.
    click_xpath_with_retry(tab, "//a[contains(text(), 'References')]", "Click References tab")?;
    human_delay();

    // 2. Click the search trigger button (id="find_refs").
    click_xpath_with_retry(
        tab,
        "//*[@id='find_refs']",
        "Click 'Search for all referenced articles in Lens.org' button",
    )?;

    // 3. Wait for the download link (#refs_ris) to become enabled (disabled class removed).
    wait_for_download_enabled(tab, "refs_ris")?;

    // 4. Extract the download URL from the href attribute.
    let href = get_element_href(tab, "refs_ris")?;
    eprintln!("  🔗 Download URL: {href}");

    let filename = format!("{}_references.ris", clean_doi_filename(doi));
    // 5. Download the file via curl.
    download_with_curl(&href, output_dir, &filename)
}

/// Run the "Citations" flow: click tab → search → wait for download enabled → fetch via curl.
fn scrape_citations(tab: &Tab, doi: &str, output_dir: &Path) -> Result<PathBuf, ScrapeError> {
    // 1. Click the "Citations" tab.
    click_xpath_with_retry(tab, "//a[contains(text(), 'Citations')]", "Click Citations tab")?;
    human_delay();

    // 2. Click the search trigger button (id="find_cits").
    click_xpath_with_retry(
        tab,
        "//*[@id='find_cits']",
        "Click 'Search for all citing articles in Lens.org' button",
    )?;

    // 3. Wait for the download link (#cits_ris) to become enabled (disabled class removed).
    wait_for_download_enabled(tab, "cits_ris")?;

    // 4. Extract the download URL from the href attribute.
    let href = get_element_href(tab, "cits_ris")?;
    eprintln!("  🔗 Download URL: {href}");

    let filename = format!("{}_citations.ris", clean_doi_filename(doi));
    // 5. Download the file via curl.
    download_with_curl(&href, output_dir, &filename)
}
