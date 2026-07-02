use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension};

/// Get a setting value by key. Returns None if not found or value is NULL.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    let result = conn
        .query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten();
    Ok(result)
}

/// Set a setting value. Inserts if not exists, updates if exists.
pub fn set_setting(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

/// Get the fulltext storage directory. Returns the configured path or the platform default.
/// Also ensures the directory exists.
pub fn get_fulltext_storage_dir(conn: &Connection) -> Result<String, AppError> {
    let configured = get_setting(conn, "fulltext_storage_dir")?;

    let path = if let Some(ref p) = configured {
        if !p.is_empty() {
            p.clone()
        } else {
            compute_default_storage_dir()
        }
    } else {
        compute_default_storage_dir()
    };

    // Ensure directory exists
    std::fs::create_dir_all(&path).map_err(|e| {
        AppError::Import(format!("Failed to create fulltext storage directory '{}': {}", path, e))
    })?;

    Ok(path)
}

/// Set the fulltext storage directory. Pass None to reset to default.
pub fn set_fulltext_storage_dir(conn: &Connection, path: Option<&str>) -> Result<(), AppError> {
    let value = path.and_then(|p| if p.is_empty() { None } else { Some(p) });
    set_setting(conn, "fulltext_storage_dir", value)?;

    // Ensure the new directory exists
    if let Some(p) = value {
        std::fs::create_dir_all(p).map_err(|e| {
            AppError::Import(format!("Failed to create fulltext storage directory '{}': {}", p, e))
        })?;
    }

    Ok(())
}

/// The `app_settings` key that records whether bibliometric normalized data
/// is stale and needs to be rebuilt on the next visit to the Bibliometrics
/// dashboard. Mutations that affect bibliometrics (imports, reference/citation
/// imports, tag/label edits, status changes, AI screening) set this to "true".
pub const BIBLIO_NEEDS_REFRESH_KEY: &str = "biblio_needs_refresh";

/// Mark bibliometric data as stale. Called by any mutation that changes the
/// underlying data bibliometrics depends on (articles, references, tags,
/// labels, screening decisions). Non-fatal: errors are logged to stderr.
pub fn mark_biblio_needs_refresh(conn: &Connection) {
    if let Err(e) = set_setting(conn, BIBLIO_NEEDS_REFRESH_KEY, Some("true")) {
        eprintln!("[biblio] failed to mark needs_refresh: {e}");
    }
}

/// Mark bibliometric data as fresh. Called after `biblio_normalize` commits.
pub fn clear_biblio_needs_refresh(conn: &Connection) {
    if let Err(e) = set_setting(conn, BIBLIO_NEEDS_REFRESH_KEY, Some("false")) {
        eprintln!("[biblio] failed to clear needs_refresh: {e}");
    }
}

/// Whether bibliometric data is stale and should be re-normalized.
/// Absent key is treated as not stale (fresh) so post-reset state (no
/// articles) does not trigger an unnecessary normalization.
pub fn get_biblio_needs_refresh(conn: &Connection) -> Result<bool, AppError> {
    Ok(get_setting(conn, BIBLIO_NEEDS_REFRESH_KEY)?.map(|v| v == "true").unwrap_or(false))
}

/// The `app_settings` key that records whether the LLM Wiki needs to be
/// re-ingested. Set by any mutation that changes the wiki's raw sources
/// (article import, status -> included, full-text attach, AI summary regen).
/// Cleared after a successful `wiki_ingest`.
pub const WIKI_NEEDS_REFRESH_KEY: &str = "wiki_needs_refresh";

/// Mark wiki data as stale. Called by any mutation that changes the wiki's
/// raw sources (article import, status -> included, full-text attach, AI
/// summary regen). Non-fatal: errors are logged to stderr.
pub fn mark_wiki_needs_refresh(conn: &Connection) {
    if let Err(e) = set_setting(conn, WIKI_NEEDS_REFRESH_KEY, Some("true")) {
        eprintln!("[wiki] failed to mark needs_refresh: {e}");
    }
}

/// Mark wiki data as fresh. Called after a successful `wiki_ingest`.
pub fn clear_wiki_needs_refresh(conn: &Connection) {
    if let Err(e) = set_setting(conn, WIKI_NEEDS_REFRESH_KEY, Some("false")) {
        eprintln!("[wiki] failed to clear needs_refresh: {e}");
    }
}

/// Whether the wiki is stale and should be re-ingested.
/// Absent key is treated as not stale (fresh) so post-reset state (no
/// included articles, no wiki) does not trigger an unnecessary ingest.
pub fn get_wiki_needs_refresh(conn: &Connection) -> Result<bool, AppError> {
    Ok(get_setting(conn, WIKI_NEEDS_REFRESH_KEY)?.map(|v| v == "true").unwrap_or(false))
}

/// Compute the platform-specific default storage directory:
/// ~/Documents/Bango/fulltext/
fn compute_default_storage_dir() -> String {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    docs.join("Bango").join("fulltext").to_string_lossy().to_string()
}

// ── Tier 3 screening-mode settings ──────────────────────────────────────────
//
// All keys are stored in the `app_settings` key/value table. Each has a stable
// default; absent keys fall back to the default. See `docs/bango-v4-spec.md`
// §4.3.1 (Screening Modes) and §8.1 (Configuration Settings).

/// `app_settings` key for the active screening mode.
/// Values: `"abstract"` (default) | `"enhanced"` | `"two_stage"`.
const SCREENING_MODE_KEY: &str = "screening_mode";

/// `app_settings` key for the enhanced-mode top-K chunk count.
const ENHANCED_TOP_K_KEY: &str = "enhanced_top_k";

/// `app_settings` key for the enhanced-mode section allow-list
/// (comma-separated). Default `"Methods,Results"`.
const ENHANCED_SCREENING_SECTIONS_KEY: &str = "enhanced_screening_sections";

/// `app_settings` keys for the two-stage borderline confidence band
/// `[two_stage_low, two_stage_high)`. Defaults 0.4 / 0.7.
const TWO_STAGE_LOW_KEY: &str = "two_stage_low";
const TWO_STAGE_HIGH_KEY: &str = "two_stage_high";

/// `app_settings` key for the per-article chunk budget (words).
const CHUNK_BUDGET_PER_ARTICLE_KEY: &str = "chunk_budget_per_article";

/// `app_settings` key for the expected fraction of articles that fall in the
/// two-stage borderline band `[two_stage_low, two_stage_high)` and therefore
/// receive a second full-text-aware pass. Used by the token-warning estimator
/// (§4.3 Readiness Check) to compute the Two-stage worst-case footprint as
/// `chunk_budget * borderline_fraction`. Advanced-only (no Settings UI);
/// power users edit via `app_settings` directly, matching `two_stage_low`/
/// `two_stage_high`. Default 0.15 per `docs/bango-v4-spec.md` §4.3.1.
const TWO_STAGE_EXPECTED_BORDERLINE_FRACTION_KEY: &str = "two_stage_expected_borderline_fraction";

/// The screening mode. Absent key = `Abstract` (default, preserving today's
/// behavior and cost exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScreeningMode {
    #[default]
    Abstract,
    Enhanced,
    TwoStage,
}

impl ScreeningMode {
    /// Parse from the stored string value. Unknown/absent → `Abstract`.
    #[must_use]
    pub fn from_str_lossy(s: Option<&str>) -> Self {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("enhanced") => Self::Enhanced,
            Some("two_stage") | Some("two-stage") | Some("twostage") => Self::TwoStage,
            _ => Self::Abstract,
        }
    }

    /// Serialize to the stored string value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abstract => "abstract",
            Self::Enhanced => "enhanced",
            Self::TwoStage => "two_stage",
        }
    }
}

pub fn get_screening_mode(conn: &Connection) -> Result<ScreeningMode, AppError> {
    Ok(ScreeningMode::from_str_lossy(get_setting(conn, SCREENING_MODE_KEY)?.as_deref()))
}

pub fn set_screening_mode(conn: &Connection, mode: ScreeningMode) -> Result<(), AppError> {
    set_setting(conn, SCREENING_MODE_KEY, Some(mode.as_str()))
}

pub fn get_enhanced_top_k(conn: &Connection) -> Result<usize, AppError> {
    Ok(get_setting(conn, ENHANCED_TOP_K_KEY)?.and_then(|v| v.parse::<usize>().ok()).unwrap_or(2))
}

pub fn set_enhanced_top_k(conn: &Connection, value: usize) -> Result<(), AppError> {
    set_setting(conn, ENHANCED_TOP_K_KEY, Some(&value.to_string()))
}

/// The sections eligible for enhanced-screening evidence chunks.
/// Default `["Methods", "Results"]`. Discussion/Limitations excluded (lower
/// screening signal). The allow-list is fixed at this default in the UI; power
/// users edit via `app_settings` only.
pub fn get_enhanced_screening_sections(conn: &Connection) -> Result<Vec<String>, AppError> {
    Ok(get_setting(conn, ENHANCED_SCREENING_SECTIONS_KEY)?
        .map(|v| {
            v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["Methods".to_string(), "Results".to_string()]))
}

pub fn set_enhanced_screening_sections(
    conn: &Connection,
    sections: &[String],
) -> Result<(), AppError> {
    set_setting(conn, ENHANCED_SCREENING_SECTIONS_KEY, Some(&sections.join(",")))
}

/// The lower bound of the two-stage borderline band (inclusive). Default 0.4.
pub fn get_two_stage_low(conn: &Connection) -> Result<f64, AppError> {
    Ok(get_setting(conn, TWO_STAGE_LOW_KEY)?
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(0.4))
}

pub fn set_two_stage_low(conn: &Connection, value: f64) -> Result<(), AppError> {
    set_setting(conn, TWO_STAGE_LOW_KEY, Some(&value.to_string()))
}

/// The upper bound of the two-stage borderline band (exclusive). Default 0.7.
pub fn get_two_stage_high(conn: &Connection) -> Result<f64, AppError> {
    Ok(get_setting(conn, TWO_STAGE_HIGH_KEY)?
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(0.7))
}

pub fn set_two_stage_high(conn: &Connection, value: f64) -> Result<(), AppError> {
    set_setting(conn, TWO_STAGE_HIGH_KEY, Some(&value.to_string()))
}

/// Per-article chunk budget (words) for enhanced / two-stage screening.
/// Default 2400 (~600 tokens). Caps per-article cost so no single article can
/// blow the screening context window.
pub fn get_chunk_budget_per_article(conn: &Connection) -> Result<usize, AppError> {
    Ok(get_setting(conn, CHUNK_BUDGET_PER_ARTICLE_KEY)?
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(2_400))
}

pub fn set_chunk_budget_per_article(conn: &Connection, value: usize) -> Result<(), AppError> {
    set_setting(conn, CHUNK_BUDGET_PER_ARTICLE_KEY, Some(&value.to_string()))
}

/// Expected fraction of articles that fall in the two-stage borderline band
/// `[two_stage_low, two_stage_high)` and receive a second full-text-aware
/// pass. Used by the §4.3 token-warning estimator. Default 0.15. Clamped to
/// `[0.0, 1.0]` on read; absent/garbage values fall back to the default.
pub fn get_two_stage_expected_borderline_fraction(conn: &Connection) -> Result<f64, AppError> {
    Ok(get_setting(conn, TWO_STAGE_EXPECTED_BORDERLINE_FRACTION_KEY)?
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(0.15))
}

pub fn set_two_stage_expected_borderline_fraction(
    conn: &Connection,
    value: f64,
) -> Result<(), AppError> {
    set_setting(conn, TWO_STAGE_EXPECTED_BORDERLINE_FRACTION_KEY, Some(&value.to_string()))
}
