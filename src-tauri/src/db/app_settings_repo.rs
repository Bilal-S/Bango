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

/// The `app_settings` key for the Bango documents root directory.
///
/// All on-disk project artifacts derive from this root as subdirectories:
/// - `fulltext/` - article PDFs + text extracts
/// - `ris/` - Citation Chaser output
/// - `wiki-root/` - LLM Wiki (Markdown)
///
/// If unconfigured, defaults to `~/Documents/Bango/`.
pub const STORAGE_ROOT_KEY: &str = "storage_root";

/// The legacy `app_settings` key (pre-reorg) that stored the full *fulltext*
/// path rather than the root. Read once during lazy migration
/// (see [`get_storage_root`]) and then superseded by [`STORAGE_ROOT_KEY`].
const LEGACY_FULLTEXT_STORAGE_DIR_KEY: &str = "fulltext_storage_dir";

/// Subdirectory name under the storage root for full-text attachments.
pub const FULLTEXT_DIR_NAME: &str = "fulltext";

/// Resolve the Bango documents root, performing a one-time lazy migration
/// from the legacy `fulltext_storage_dir` key to [`STORAGE_ROOT_KEY`].
///
/// Migration rules (only run when `storage_root` is absent):
/// 1. Legacy value ending in `fulltext` (e.g. `~/Documents/Bango/fulltext`)
///    -> root = parent (`~/Documents/Bango`).
/// 2. Legacy custom value *not* ending in `fulltext` -> root = the value as-is
///    (preserves the prior non-fulltext custom-dir behavior).
/// 3. Legacy absent/empty -> default `~/Documents/Bango/`.
///
/// After computing, the normalized root is persisted to `storage_root` so
/// subsequent reads are O(1) and the legacy key is never consulted again.
/// Ensures the directory exists.
pub fn get_storage_root(conn: &Connection) -> Result<String, AppError> {
    // Fast path: the new key is already set.
    if let Some(root) = get_setting(conn, STORAGE_ROOT_KEY)? {
        if !root.is_empty() {
            ensure_storage_root_exists(&root)?;
            return Ok(root);
        }
    }

    // Lazy migration: derive the root from the legacy fulltext key.
    let legacy = get_setting(conn, LEGACY_FULLTEXT_STORAGE_DIR_KEY)?;
    let default = compute_default_storage_root();
    let root = normalize_legacy_to_root(legacy.as_deref(), &default);

    // Persist so we never consult the legacy key again.
    set_setting(conn, STORAGE_ROOT_KEY, Some(&root))?;
    ensure_storage_root_exists(&root)?;
    Ok(root)
}

/// Set the Bango documents root. Pass `None` to reset to the platform default.
pub fn set_storage_root(conn: &Connection, path: Option<&str>) -> Result<(), AppError> {
    let value = path.and_then(|p| if p.is_empty() { None } else { Some(p) });
    let root = value.map(String::from).unwrap_or_else(compute_default_storage_root);
    set_setting(conn, STORAGE_ROOT_KEY, Some(&root))?;
    ensure_storage_root_exists(&root)?;
    Ok(())
}

/// The full-text subdirectory under the storage root.
pub fn get_fulltext_dir(conn: &Connection) -> Result<String, AppError> {
    let root = get_storage_root(conn)?;
    let fulltext = std::path::Path::new(&root).join(FULLTEXT_DIR_NAME);
    std::fs::create_dir_all(&fulltext).map_err(|e| {
        AppError::Import(format!(
            "Failed to create fulltext storage directory '{}': {}",
            fulltext.display(),
            e
        ))
    })?;
    Ok(fulltext.to_string_lossy().to_string())
}

/// Derive the storage root from a legacy `fulltext_storage_dir` value.
///
/// - Trailing `fulltext` segment stripped (parent becomes root).
/// - Non-fulltext custom path kept as-is.
/// - Empty/absent falls back to `default`.
#[must_use]
pub fn normalize_legacy_to_root(legacy: Option<&str>, default: &str) -> String {
    let Some(p) = legacy.filter(|s| !s.is_empty()) else {
        return default.to_string();
    };
    let path = std::path::Path::new(p);
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case(FULLTEXT_DIR_NAME))
        .unwrap_or(false)
    {
        path.parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .filter(|parent| !parent.is_empty())
            .unwrap_or_else(|| default.to_string())
    } else {
        p.to_string()
    }
}

/// Ensure the storage root directory exists.
fn ensure_storage_root_exists(root: &str) -> Result<(), AppError> {
    std::fs::create_dir_all(root).map_err(|e| {
        AppError::Import(format!("Failed to create storage root directory '{root}': {e}"))
    })?;
    Ok(())
}

/// Compute the platform-specific default storage root: `~/Documents/Bango/`.
fn compute_default_storage_root() -> String {
    let docs = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    docs.join("Bango").to_string_lossy().to_string()
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

// ── Custom Screening Instructions ────────────────────────────────────────────

/// The `app_settings` key for the optional custom screening-instructions text.
///
/// Free-text combinatorial rules the LLM applies during screening (AND/OR
/// gates, hard exclusions, conditional inclusion). References criteria by
/// their globally unique number (inclusion `1..N`, exclusion continues
/// `N+1..N+M`, matching the Criteria screen). Empty/absent = today's
/// priority-only behavior (backward-compatible). Stored verbatim (only
/// trimmed of surrounding whitespace on read).
pub const SCREENING_CUSTOM_LOGIC_KEY: &str = "screening_custom_logic";

/// Read the custom screening-instructions text. Returns `None` when the key
/// is absent or the stored value trims to empty.
pub fn get_screening_custom_logic(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(get_setting(conn, SCREENING_CUSTOM_LOGIC_KEY)?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty()))
}

/// Persist the custom screening-instructions text. The value is trimmed of
/// surrounding whitespace before storage; an empty value is allowed and
/// effectively disables the feature.
pub fn set_screening_custom_logic(conn: &Connection, value: &str) -> Result<(), AppError> {
    set_setting(conn, SCREENING_CUSTOM_LOGIC_KEY, Some(value.trim()))
}

// ── Embedding settings (triple-state capability flag) ───────────────────────

/// The `app_settings` key recording the embedding capability state.
/// Values: `"unknown"` (default) | `"enabled"` | `"disabled"`. Set by the
/// `probe_embedding_support` flow during `Test Connection` or the first
/// embedding call. Reset to `"unknown"` whenever LLM config changes.
pub const EMBEDDING_STATUS_KEY: &str = "embedding_status";
/// The `app_settings` key recording the working embedding model name once the
/// probe succeeds (e.g. `"text-embedding-3-small"`).
pub const EMBEDDING_MODEL_KEY: &str = "embedding_model";
/// The `app_settings` key recording the embedding vector dimensionality once
/// the probe succeeds (e.g. `1536`). Used by recall to filter rows whose
/// dimensions don't match the current model.
pub const EMBEDDING_DIMENSIONS_KEY: &str = "embedding_dimensions";

/// The triple-state embedding capability. `Unknown` (default) means the probe
/// has not run yet; `Enabled` means a working model was found; `Disabled` means
/// the provider has no embedding-capable model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingStatus {
    #[default]
    Unknown,
    Enabled,
    Disabled,
}

impl EmbeddingStatus {
    /// Parse from the stored string value. Unknown/absent/garbage -> `Unknown`.
    #[must_use]
    pub fn from_str_lossy(s: Option<&str>) -> Self {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("enabled") => Self::Enabled,
            Some("disabled") => Self::Disabled,
            _ => Self::Unknown,
        }
    }

    /// Serialize to the stored string value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// Read the embedding capability state. Absent key = `Unknown` (default).
pub fn get_embedding_status(conn: &Connection) -> Result<EmbeddingStatus, AppError> {
    Ok(EmbeddingStatus::from_str_lossy(get_setting(conn, EMBEDDING_STATUS_KEY)?.as_deref()))
}

/// Persist the embedding capability state + the working model + dimensions.
pub fn set_embedding_status(
    conn: &Connection,
    status: EmbeddingStatus,
    model: &str,
    dimensions: i32,
) -> Result<(), AppError> {
    set_setting(conn, EMBEDDING_STATUS_KEY, Some(status.as_str()))?;
    set_setting(conn, EMBEDDING_MODEL_KEY, if model.is_empty() { None } else { Some(model) })?;
    set_setting(conn, EMBEDDING_DIMENSIONS_KEY, Some(&dimensions.to_string()))?;
    Ok(())
}

/// Read the working embedding model name (set by the probe).
pub fn get_embedding_model(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(get_setting(conn, EMBEDDING_MODEL_KEY)?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Read the embedding dimensions (set by the probe). 0 when unknown.
pub fn get_embedding_dimensions(conn: &Connection) -> Result<i32, AppError> {
    Ok(get_setting(conn, EMBEDDING_DIMENSIONS_KEY)?
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(0))
}

/// Reset the embedding capability state to `unknown` (e.g. when the LLM config
/// changes, the probe must re-evaluate against the new provider/endpoint/model).
/// Keeps the model/dimensions so the user can see what was last working.
pub fn reset_embedding_status(conn: &Connection) -> Result<(), AppError> {
    set_setting(conn, EMBEDDING_STATUS_KEY, Some(EmbeddingStatus::Unknown.as_str()))
}

// ── Embedding model override (premium) ───────────────────────────────────────

/// The `app_settings` key for the optional embedding-model override (premium).
///
/// When set to a non-empty model name, `probe_embedding_support` tries this
/// model FIRST, ahead of the provider-default and the configured chat model.
/// This lets premium users pin a specific embedding model (e.g.
/// `text-embedding-3-large`, a specific Ollama embedding model) instead of
/// relying on auto-detection. When the override fails (404/405/auth error),
/// the probe falls back to the standard auto-detection order so a bad override
/// never hard-disables embeddings.
///
/// Machine-local (excluded from `PROJECT_PORTABLE_SETTINGS`): the choice is
/// tied to the provider configuration on this machine, which does not travel
/// with a project backup.
pub const EMBEDDING_MODEL_OVERRIDE_KEY: &str = "embedding_model_override";

/// Read the embedding-model override. Returns `None` when the key is absent or
/// the stored value trims to empty (so an empty input = "let the probe pick").
pub fn get_embedding_model_override(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(get_setting(conn, EMBEDDING_MODEL_OVERRIDE_KEY)?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Persist the embedding-model override. Pass `None` or an empty/whitespace
/// string to clear the override (restore auto-detection).
pub fn set_embedding_model_override(
    conn: &Connection,
    value: Option<&str>,
) -> Result<(), AppError> {
    let trimmed = value.map(str::trim).filter(|s| !s.is_empty());
    set_setting(conn, EMBEDDING_MODEL_OVERRIDE_KEY, trimmed)
}

// ── Project name (editable dashboard title) ─────────────────────────────────

/// The `app_settings` key for the user-editable project name shown in the
/// Dashboard header (replaces the "Project Dashboard" placeholder once set).
///
/// Free-text, up to [`PROJECT_NAME_MAX_LEN`] characters. Stored verbatim
/// (only trimmed of surrounding whitespace on read). Empty/absent = the
/// dashboard shows the "Project Dashboard" fallback. Portable: travels with
/// a project backup so restoring on a new machine keeps the user's title.
pub const PROJECT_NAME_KEY: &str = "project_name";

/// Maximum character length enforced for the project name. The frontend
/// `<input maxlength>` is the primary gate; the backend `set_project_name`
/// hard-caps as defense-in-depth so a stale frontend (or a direct DB write)
/// cannot store an overlong value.
pub const PROJECT_NAME_MAX_LEN: usize = 50;

/// Read the project name. Returns `None` when the key is absent or the stored
/// value trims to empty (the dashboard renders its "Project Dashboard"
/// fallback in that case).
pub fn get_project_name(conn: &Connection) -> Result<Option<String>, AppError> {
    Ok(get_setting(conn, PROJECT_NAME_KEY)?.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

/// Persist the project name. The value is trimmed of surrounding whitespace
/// and hard-capped to [`PROJECT_NAME_MAX_LEN`] chars (counted by `char::count`,
/// not byte length, so multi-byte CJK/emoji counts as one char per code point).
/// An empty/whitespace-only value is stored as `NULL` (effectively a clear),
/// keeping the table clean and matching [`get_project_name`]'s `None`-on-empty
/// contract.
pub fn set_project_name(conn: &Connection, value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    let capped: String = trimmed.chars().take(PROJECT_NAME_MAX_LEN).collect();
    let to_store = if capped.is_empty() { None } else { Some(capped.as_str()) };
    set_setting(conn, PROJECT_NAME_KEY, to_store)
}

// ── Project-portable settings (export/import) ───────────────────────────────
//
// `app_settings` mixes project-level intent (screening rules, summary mode,
// auto-translate, project name) with machine-local state (storage root,
// premium flag, staleness flags, embedding model override). Only the
// project-level subset travels with a backup so restoring a project on a new
// machine preserves the user's screening configuration + project title
// without leaking secrets or clobbering local state.

/// The subset of `app_settings` keys that travel with a project backup.
///
/// - `screening_custom_logic` - combinatorial screening rules (the new feature)
/// - `summary_evidence_mode` - literature-review evidence enrichment
/// - `auto_translate` - experimental non-English → English translation toggle
/// - `screening_mode` + enhanced/two-stage params - per-run screening behavior
/// - `project_name` - user-editable dashboard title
///
/// Explicitly **excluded** (stay machine-local, never exported):
/// `storage_root`, `flag_premium`, `biblio_needs_refresh`, `wiki_needs_refresh`,
/// `wiki_dir_hash`, `fulltext_storage_dir` (legacy), `embedding_model_override`.
pub const PROJECT_PORTABLE_SETTINGS: &[&str] = &[
    SCREENING_CUSTOM_LOGIC_KEY,
    AUTO_TRANSLATE_KEY,
    "summary_evidence_mode",
    SCREENING_MODE_KEY,
    ENHANCED_TOP_K_KEY,
    ENHANCED_SCREENING_SECTIONS_KEY,
    TWO_STAGE_LOW_KEY,
    TWO_STAGE_HIGH_KEY,
    CHUNK_BUDGET_PER_ARTICLE_KEY,
    TWO_STAGE_EXPECTED_BORDERLINE_FRACTION_KEY,
    "openalex_mailto",
    "openalex_retrieve_references",
    PROJECT_NAME_KEY,
];

/// Whether a given `app_settings` key should travel with a project backup.
#[must_use]
pub fn is_project_portable(key: &str) -> bool {
    PROJECT_PORTABLE_SETTINGS.contains(&key)
}

/// Export the project-portable `app_settings` rows as `(key, value)` pairs.
/// Used by `export::project::export_project` so a backup → restore cycle
/// preserves the user's screening configuration across machines. Rows with
/// NULL or empty values are omitted (they'd be no-ops on import anyway).
pub fn export_project_portable_settings(
    conn: &Connection,
) -> Result<Vec<(String, String)>, AppError> {
    let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (key, value) = row?;
        if !is_project_portable(&key) {
            continue;
        }
        if let Some(v) = value {
            if !v.is_empty() {
                out.push((key, v));
            }
        }
    }
    Ok(out)
}

// ── Auto Translate setting ──────────────────────────────────────────────────

/// The `app_settings` key for the experimental auto-translate toggle.
///
/// When enabled, articles written in other languages are translated to English
/// during AI processing (import trigger, full-text attach trigger, batch-import
/// Phase 3, and the screening pre-step). Default is `false` (opt-in): the user
/// must enable it explicitly in Settings so imports do not silently trigger
/// background translation + LLM cost. Unlike the sibling AI Summary toggles
/// (which live in `localStorage`), this is persisted in the database so it can
/// be read by backend processing stages.
pub const AUTO_TRANSLATE_KEY: &str = "auto_translate";

/// Whether auto-translate is enabled. Absent key = `false` (opt-in default).
/// Any value other than the exact strings `"true"` / `"false"` falls back to
/// the default so a corrupted row never silently enables the feature.
pub fn get_auto_translate(conn: &Connection) -> Result<bool, AppError> {
    Ok(match get_setting(conn, AUTO_TRANSLATE_KEY)?.as_deref() {
        Some("true") => true,
        Some("false") => false,
        // Absent key or unrecognized value: default disabled (opt-in).
        _ => false,
    })
}

/// Persist the auto-translate toggle.
pub fn set_auto_translate(conn: &Connection, enabled: bool) -> Result<(), AppError> {
    set_setting(conn, AUTO_TRANSLATE_KEY, Some(if enabled { "true" } else { "false" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "/home/user/Documents/Bango";

    #[test]
    fn normalize_legacy_none_returns_default() {
        assert_eq!(normalize_legacy_to_root(None, DEFAULT), DEFAULT);
    }

    #[test]
    fn normalize_legacy_empty_returns_default() {
        assert_eq!(normalize_legacy_to_root(Some(""), DEFAULT), DEFAULT);
    }

    #[test]
    fn normalize_legacy_fulltext_suffix_strips_to_parent() {
        let legacy = "/home/user/Documents/Bango/fulltext";
        assert_eq!(normalize_legacy_to_root(Some(legacy), DEFAULT), DEFAULT);
    }

    #[test]
    fn normalize_legacy_fulltext_case_insensitive() {
        let legacy = "/home/user/Documents/Bango/FullText";
        assert_eq!(normalize_legacy_to_root(Some(legacy), DEFAULT), DEFAULT);
    }

    #[test]
    fn normalize_legacy_custom_non_fulltext_kept_as_is() {
        let legacy = "/data/my-bango-store";
        assert_eq!(normalize_legacy_to_root(Some(legacy), DEFAULT), "/data/my-bango-store");
    }

    #[test]
    fn normalize_legacy_bare_fulltext_falls_back_to_default() {
        // Edge case: path is exactly `fulltext` -> parent is empty -> default.
        assert_eq!(normalize_legacy_to_root(Some("fulltext"), DEFAULT), DEFAULT);
    }
}
