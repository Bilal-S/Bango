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
