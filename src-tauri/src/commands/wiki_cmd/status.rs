//! Wiki status / root / init commands + drift check + `ensure_initialized`.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use crate::db::app_settings_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{agents_contract, fts, storage, templates};

use serde::Serialize;
use tauri::Emitter;

/// Result of `wiki_check_for_updates`. Drives the frontend toast UX.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdatesResult {
    /// `true` when external edits were detected and the FTS5 index + manifest
    /// were rebuilt. `false` when the index was already in sync (or the wiki
    /// is empty / not initialized).
    pub rebuilt: bool,
    /// Number of pages currently in the index (useful for the toast message).
    pub pages_reindexed: usize,
}

/// On-demand drift check: detect external edits to `wiki/**/*.md` files and
/// re-index them transparently without re-running the LLM ingest.
///
/// **Never blocks the UI.** Runs as an async Tauri command on the tokio
/// runtime. All filesystem work (page reads + per-file hashing) happens
/// **lock-free**; the `DbState` mutex is held only for the millisecond-scale
/// SQLite writes (manifest read/write, FTS5 rebuild, dir-hash update).
///
/// Two-tier detection keeps the common case cheap:
/// 1. **Tier 1 (directory fingerprint):** stat-only SHA-256 over
///   `(rel_path, mtime, size)` for every page. Stored in `app_settings` under
///   `wiki_dir_hash`. Equal -> nothing changed, return immediately.
/// 2. **Tier 2 (per-file content hashes):** the `wiki_index_manifest` table
///   stores one SHA-256 per file. When tier-1 drifts, compare per-file hashes.
///   If content is identical (e.g. `touch`) -> update only the dir hash. If
///   any file's content hash differs (or the path set changed) -> rebuild
///   FTS5 + rewrite the manifest.
///
/// Triggered from:
/// - Wiki view `onMounted` (debounced 30s).
/// - Chat view `onMounted` (wiki mode available).
/// - "Check for Updates" button in the wiki toolbar Actions menu (manual,
///   bypasses the debounce).
#[tauri::command]
pub async fn wiki_check_for_updates(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
) -> Result<CheckUpdatesResult, AppError> {
    // Step 1: resolve the wiki root (microsecond lock).
    let root = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        storage::resolve_root(&conn)?
    };

    // Skip entirely if the wiki was never initialized (no AGENTS.md). Avoids
    // hashing an empty/nonexistent wiki/ tree on every chat-view mount.
    if !root.join("AGENTS.md").exists() {
        return Ok(CheckUpdatesResult { rebuilt: false, pages_reindexed: 0 });
    }

    // Step 2: LOCK-FREE filesystem work (the slow part).
    // Read every wiki page from disk + compute the tier-1 directory fingerprint
    // (stat-only) and the tier-2 per-file content hashes (file reads).
    let rows = fts::collect_page_rows(&root)?;
    let dir_hash = fts::compute_directory_fingerprint(&rows);
    let file_hashes = fts::compute_file_hashes(&rows)?;

    // Step 3: tier-1 fast path + tier-2 diff (microsecond lock for reads).
    let (stored_dir_hash, stored_manifest): (
        Option<String>,
        std::collections::HashMap<String, String>,
    ) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        (fts::get_dir_hash(&conn), fts::read_manifest(&conn)?)
    };

    // No pages on disk: clear any stale baseline and report nothing to do.
    if dir_hash.is_none() {
        if stored_dir_hash.is_some() || !stored_manifest.is_empty() {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            fts::set_dir_hash(&conn, None);
            fts::write_manifest(&conn, &[])?;
        }
        return Ok(CheckUpdatesResult { rebuilt: false, pages_reindexed: 0 });
    }

    // Tier 1: directory fingerprint matches -> nothing changed.
    if dir_hash.as_deref() == stored_dir_hash.as_deref() {
        return Ok(CheckUpdatesResult { rebuilt: false, pages_reindexed: rows.len() });
    }

    // Tier 2: per-file content hash diff.
    let drifted = fts::manifest_drifted(&stored_manifest, &file_hashes);
    if !drifted {
        // mtime/size changed but content is identical (e.g. `touch`). Update
        // only the dir hash so the next check is a fast-path hit.
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        fts::set_dir_hash(&conn, dir_hash.as_deref());
        return Ok(CheckUpdatesResult { rebuilt: false, pages_reindexed: rows.len() });
    }

    // Step 4: drift confirmed -> rebuild FTS5 + manifest + dir hash (brief lock).
    let pages_count = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        fts::ensure_table(&conn)?;
        conn.execute_batch(&format!("DELETE FROM {};", fts::FTS_TABLE))?;
        let wiki_dir = root.join("wiki");
        fts::insert_page_rows(&conn, &rows, &wiki_dir)?;
        fts::write_manifest(&conn, &file_hashes)?;
        fts::set_dir_hash(&conn, dir_hash.as_deref());
        rows.len()
    };

    // Step 5: notify the frontend so open wiki/chat views can refresh.
    let _ =
        app_handle.emit("wiki:files-changed", serde_json::json!({ "pagesReindexed": pages_count }));

    Ok(CheckUpdatesResult { rebuilt: true, pages_reindexed: pages_count })
}

/// Effective wiki root + status metadata returned by `wiki_get_status`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiStatus {
    /// Always true after `wiki_init` has scaffolded the tree.
    pub configured: bool,
    /// Absolute path to the effective `wiki-root/` directory.
    pub root_dir: String,
    /// Whether an explicit override is configured (vs derived default).
    pub is_custom: bool,
    /// Platform default path (derived from `storage_root`).
    pub default_path: String,
    /// Count of `.md` files in `/raw` (top-level).
    pub raw_count: usize,
    /// Count of `.md` files in `/wiki` (recursive).
    pub page_count: usize,
    /// Whether the included article corpus changed since the last ingest.
    pub needs_refresh: bool,
    /// Number of included articles (the raw input set for the wiki).
    pub included_article_count: usize,
    /// Whether the wiki root has been initialized (AGENTS.md present).
    pub initialized: bool,
}

/// Information returned by `wiki_init`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiInitResult {
    pub root_dir: String,
    pub created: bool, // true if AGENTS.md did not exist before this call
}

/// Information returned by `wiki_get_root_dir` / `wiki_set_root_dir`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiRootInfo {
    pub effective_path: String,
    pub is_custom: bool,
    pub default_path: String,
}

/// Get the current wiki status: root path, page/raw counts, staleness, and
/// readiness gates (LLM config is checked separately by the frontend via the
/// existing `has_llm_config` command).
#[tauri::command]
pub fn wiki_get_status(db_state: tauri::State<'_, DbState>) -> Result<WikiStatus, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;

    let root = storage::resolve_root(&conn)?;
    let is_custom = storage::has_explicit_override(&conn)?;

    // Default path for display: derived from the storage root.
    let default_path = {
        let storage_str = app_settings_repo::get_storage_root(&conn)?;
        storage::compute_default_root(std::path::Path::new(&storage_str))
            .to_string_lossy()
            .to_string()
    };

    let raw_count = storage::count_markdown(&root, "raw", false);
    let page_count = storage::count_markdown(&root, "wiki", true);
    let needs_refresh = app_settings_repo::get_wiki_needs_refresh(&conn)?;

    let included_article_count: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |row| {
            row.get::<_, i64>(0).map(|v| v as usize)
        })
        .unwrap_or(0);

    let initialized = root.join("AGENTS.md").exists();

    Ok(WikiStatus {
        configured: true,
        root_dir: root.to_string_lossy().to_string(),
        is_custom,
        default_path,
        raw_count,
        page_count,
        needs_refresh,
        included_article_count,
        initialized,
    })
}

/// Get the effective wiki-root directory info.
#[tauri::command]
pub fn wiki_get_root_dir(db_state: tauri::State<'_, DbState>) -> Result<WikiRootInfo, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let is_custom = storage::has_explicit_override(&conn)?;
    let default_path = {
        let storage_str = app_settings_repo::get_storage_root(&conn)?;
        storage::compute_default_root(std::path::Path::new(&storage_str))
            .to_string_lossy()
            .to_string()
    };
    Ok(WikiRootInfo { effective_path: root.to_string_lossy().to_string(), is_custom, default_path })
}

/// Set an explicit wiki-root override. Pass empty/None to reset to the derived
/// default (`{storage_root}/wiki-root`).
#[tauri::command]
pub fn wiki_set_root_dir(
    db_state: tauri::State<'_, DbState>,
    path: Option<String>,
) -> Result<WikiRootInfo, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let value = path.as_deref().and_then(|p| if p.is_empty() { None } else { Some(p) });
    app_settings_repo::set_setting(&conn, storage::WIKI_ROOT_DIR_KEY, value)?;

    // Ensure the (possibly new) root exists.
    let root = storage::resolve_root(&conn)?;
    let is_custom = storage::has_explicit_override(&conn)?;
    let default_path = {
        let storage_str = app_settings_repo::get_storage_root(&conn)?;
        storage::compute_default_root(std::path::Path::new(&storage_str))
            .to_string_lossy()
            .to_string()
    };
    Ok(WikiRootInfo { effective_path: root.to_string_lossy().to_string(), is_custom, default_path })
}

/// Initialize the wiki: scaffold the directory tree, write `AGENTS.md`, and
/// seed `templates/`. Idempotent. Does NOT ingest (that is `wiki_ingest`).
#[tauri::command]
pub fn wiki_init(db_state: tauri::State<'_, DbState>) -> Result<WikiInitResult, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    storage::scaffold_tree(&root)?;

    let created = ensure_initialized(&root)?;
    templates::write_all(&root.join("templates"))?;

    // Initialize wiki/log.md if missing (lives inside wiki/ output dir).
    let log_path = root.join("wiki").join("log.md");
    if !log_path.exists() {
        std::fs::write(
            &log_path,
            "# Wiki Audit Log\n\nAppend-only record of ingest and lint runs.\n",
        )
        .map_err(|e| AppError::Import(format!("Failed to write log.md: {}", e)))?;
    }

    Ok(WikiInitResult { root_dir: root.to_string_lossy().to_string(), created })
}

/// Ensure the wiki is initialized by writing `AGENTS.md` if it is missing.
///
/// Self-healing guard: `wiki_ingest` and `wiki_rebuild` call this at the top
/// of their pipelines so an uninitialized wiki is transparently initialized
/// before any LLM work begins. This prevents the "pages on disk but invisible
/// in the UI" state that occurred when `AGENTS.md` was deleted (the status
/// command reports `initialized: false` based on `AGENTS.md` presence, which
/// gated the entire wiki-view UI even though generated pages existed).
///
/// Returns `true` if `AGENTS.md` was newly created, `false` if it already
/// existed. Idempotent: never overwrites an existing `AGENTS.md`.
pub fn ensure_initialized(root: &std::path::Path) -> Result<bool, AppError> {
    let agents_path = root.join("AGENTS.md");
    if agents_path.exists() {
        return Ok(false);
    }
    std::fs::write(&agents_path, agents_contract::agents_md_content())
        .map_err(|e| AppError::Import(format!("Failed to write AGENTS.md: {}", e)))?;
    Ok(true)
}
