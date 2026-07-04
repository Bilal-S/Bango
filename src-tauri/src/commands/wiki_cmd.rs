use std::path::PathBuf;
use std::sync::Arc;

use crate::db::app_settings_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{
    agents_contract, chat as wiki_chat_mod, engine, frontmatter, fts, ingest, raw_export, storage,
    templates,
};

use crate::commands::chat::ChatMessage;
use serde::Serialize;
use tauri::Emitter;

/// Lock a mutex holding `rusqlite::Connection`, mapping poison errors to
/// `AppError::Database`. Keeps command bodies free of the boilerplate.
fn lock_conn<'a>(
    db_state: &'a tauri::State<'a, DbState>,
) -> Result<std::sync::MutexGuard<'a, rusqlite::Connection>, AppError> {
    db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))
}

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
        let conn = lock_conn(&db_state)?;
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
        let conn = lock_conn(&db_state)?;
        (fts::get_dir_hash(&conn), fts::read_manifest(&conn)?)
    };

    // No pages on disk: clear any stale baseline and report nothing to do.
    if dir_hash.is_none() {
        if stored_dir_hash.is_some() || !stored_manifest.is_empty() {
            let conn = lock_conn(&db_state)?;
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
        let conn = lock_conn(&db_state)?;
        fts::set_dir_hash(&conn, dir_hash.as_deref());
        return Ok(CheckUpdatesResult { rebuilt: false, pages_reindexed: rows.len() });
    }

    // Step 4: drift confirmed -> rebuild FTS5 + manifest + dir hash (brief lock).
    let pages_count = {
        let conn = lock_conn(&db_state)?;
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
    let conn = lock_conn(&db_state)?;

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
    let conn = lock_conn(&db_state)?;
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
    let conn = lock_conn(&db_state)?;
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
    let conn = lock_conn(&db_state)?;
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

/// Prepare raw sources: export included articles AND process user-dropped files.
/// Runs both on-ramps in sequence. Idempotent.
#[tauri::command]
pub fn wiki_export_raw(
    db_state: tauri::State<'_, DbState>,
) -> Result<raw_export::RawExportReport, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let report = raw_export::prepare_all(&conn, &root)?;
    Ok(report)
}

/// Add a user-selected file to `raw/` and extract its companion `.md` immediately.
/// Returns the companion `.md` path.
#[tauri::command]
pub fn wiki_add_raw_file(
    db_state: tauri::State<'_, DbState>,
    file_path: String,
) -> Result<String, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let companion = raw_export::add_user_file(&root, std::path::Path::new(&file_path))?;
    Ok(companion.to_string_lossy().to_string())
}

/// Fetch a URL, extract its text content, and add it as a wiki raw source.
#[tauri::command]
pub async fn wiki_add_raw_url(
    db_state: tauri::State<'_, DbState>,
    url: String,
) -> Result<String, AppError> {
    // Derive a title from the last path segment or host.
    let title = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && !s.contains(':'))
        .unwrap_or("web-page")
        .to_string();

    // Fetch the page.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; BangoWiki/1.0)")
        .build()
        .map_err(|e| AppError::Import(format!("HTTP client error: {e}")))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("Failed to fetch '{url}': {e}")))?;
    let html = response
        .text()
        .await
        .map_err(|e| AppError::Import(format!("Failed to read response from '{url}': {e}")))?;

    // Resolve wiki root and add the content.
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let companion = raw_export::add_raw_content(&root, &title, &html, &url)?;
    Ok(companion.to_string_lossy().to_string())
}

/// A raw file entry for the listing command.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFileEntry {
    pub path: String,
    pub title: String,
    pub slug: String,
    pub source_kind: String,
    pub source_file: Option<String>,
    pub status: String,
}

/// List all `.md` files in `raw/` with parsed metadata.
#[tauri::command]
pub fn wiki_list_raw_files(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<RawFileEntry>, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let files = raw_export::list_raw_files(&root)?;
    let entries = files
        .into_iter()
        .map(|(path, fm)| RawFileEntry {
            path: path.to_string_lossy().to_string(),
            title: fm.get("title").unwrap_or("Untitled").to_string(),
            slug: fm.get("slug").unwrap_or("").to_string(),
            source_kind: fm.get("source_kind").unwrap_or("").to_string(),
            source_file: fm.get("source_file").map(str::to_string),
            status: fm.get("status").unwrap_or("draft").to_string(),
        })
        .collect();
    Ok(entries)
}

/// Search the wiki FTS5 index. Returns BM25-ranked hits.
#[tauri::command]
pub fn wiki_search(
    db_state: tauri::State<'_, DbState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<fts::WikiPageHit>, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    // Self-heal: rebuild the index if it is empty but pages exist on disk
    // (e.g. after a schema rebuild / DB reset that dropped the FTS table).
    fts::ensure_index_populated(&conn, &root)?;
    fts::search(&conn, &query, limit.unwrap_or(10))
}

// Allow frontmatter import to be referenced (used by tests + future lint command).
#[allow(dead_code)]
fn _ensure_frontmatter_linked() {
    let _ = frontmatter::Frontmatter::default();
}

/// Lint the wiki: detect broken links, orphans, duplicates, missing frontmatter.
#[tauri::command]
pub fn wiki_lint(db_state: tauri::State<'_, DbState>) -> Result<engine::LintReport, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let report = engine::lint(&root)?;
    Ok(report)
}

/// A wiki page returned by `wiki_get_page`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPage {
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub status: String,
    pub summary: String,
    pub body: String,
    pub file_path: String,
    pub source_articles: Option<String>,
}

/// Read a single wiki page by slug. Searches `wiki/**/*.md` for a matching
/// frontmatter `slug` field.
#[tauri::command]
pub fn wiki_get_page(
    db_state: tauri::State<'_, DbState>,
    slug: String,
) -> Result<Option<WikiPage>, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let pages = fts::collect_wiki_pages(&root)?;
    for path in pages {
        let (fm, body) = frontmatter::read_file(&path)?;
        if fm.get("slug") == Some(slug.as_str()) {
            return Ok(Some(WikiPage {
                slug: fm.get("slug").unwrap_or("").to_string(),
                title: fm.get("title").unwrap_or("").to_string(),
                page_type: fm.get("type").unwrap_or("").to_string(),
                status: fm.get("status").unwrap_or("draft").to_string(),
                summary: fm.get("summary").unwrap_or("").to_string(),
                body,
                file_path: path.to_string_lossy().to_string(),
                source_articles: fm.get("source_articles").map(str::to_string),
            }));
        }
    }
    Ok(None)
}

/// Update a wiki page's body + title + summary (preserves other frontmatter).
/// Writes atomically (write to temp, rename).
#[tauri::command]
pub fn wiki_update_page(
    db_state: tauri::State<'_, DbState>,
    slug: String,
    title: String,
    summary: String,
    body: String,
) -> Result<WikiPage, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let pages = fts::collect_wiki_pages(&root)?;
    for path in pages {
        let (fm, _old_body) = frontmatter::read_file(&path)?;
        if fm.get("slug") == Some(slug.as_str()) {
            let mut new_fm = fm.clone();
            new_fm.set("title", &title);
            new_fm.set("summary", &summary);
            // Bump the updated timestamp.
            let now = chrono::Utc::now().to_rfc3339();
            new_fm.set("updated", &now);
            // status: reviewed is protected from LLM overwrites, but a human
            // edit via this command is intentional - demote to 'draft' so the
            // next lint picks it up if links changed.
            if fm.get("status") == Some("reviewed") {
                new_fm.set("status", "draft");
            }
            frontmatter::write_file(&path, &new_fm, &body)?;
            // Keep the FTS5 index in sync so wiki_chat / wiki_search reflect
            // the edited title/summary/body immediately. A full rebuild is
            // cheap (dozens to low-hundreds of local pages) and avoids fragile
            // per-row sync logic. Uses `rebuild_index_with_manifest` so the
            // drift-detection manifest stays in sync too (otherwise the next
            // `wiki_check_for_updates` would false-positive a rebuild).
            fts::rebuild_index_with_manifest(&conn, &root)?;
            return Ok(WikiPage {
                slug: new_fm.get("slug").unwrap_or("").to_string(),
                title,
                page_type: new_fm.get("type").unwrap_or("").to_string(),
                status: new_fm.get("status").unwrap_or("draft").to_string(),
                summary,
                body,
                file_path: path.to_string_lossy().to_string(),
                source_articles: new_fm.get("source_articles").map(str::to_string),
            });
        }
    }
    Err(AppError::Import(format!("Wiki page '{slug}' not found")))
}

/// Delete a single wiki page by slug.
#[tauri::command]
pub fn wiki_delete_page(
    db_state: tauri::State<'_, DbState>,
    slug: String,
) -> Result<bool, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let pages = fts::collect_wiki_pages(&root)?;
    for path in pages {
        let (fm, _body) = frontmatter::read_file(&path)?;
        if fm.get("slug") == Some(slug.as_str()) {
            std::fs::remove_file(&path)?;
            // Keep the FTS5 index in sync so the deleted page is no longer
            // returned by wiki_chat / wiki_search. Uses
            // `rebuild_index_with_manifest` so the drift-detection manifest
            // stays in sync too.
            fts::rebuild_index_with_manifest(&conn, &root)?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Delete the entire wiki output (the `wiki/` subtree). Keeps `raw/`,
/// `templates/`, and `AGENTS.md`.
///
/// Does NOT touch the `wiki_needs_refresh` staleness flag. That flag signals
/// "the article corpus changed since the last ingest" and is set by corpus-
/// mutation sites (imports, status changes, full-text attachments). Setting it
/// here would make `wiki-view.vue`'s `autoIngestIfStale` immediately rebuild
/// what the user just deleted. The user can manually rebuild via the toolbar.
#[tauri::command]
pub fn wiki_delete_wiki(db_state: tauri::State<'_, DbState>) -> Result<(), AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let wiki_dir = root.join("wiki");
    if wiki_dir.exists() {
        std::fs::remove_dir_all(&wiki_dir)?;
    }
    // Re-scaffold the empty wiki tree + log.md.
    storage::scaffold_tree(&root)?;
    Ok(())
}

/// Send a wiki-grounded chat message (FTS5 RAG). Returns the assistant response.
#[tauri::command]
pub async fn wiki_chat(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    question: String,
    history: Vec<ChatMessage>,
) -> Result<String, AppError> {
    wiki_chat_mod::wiki_chat(db_state, orchestrator, &question, &history).await
}

/// Get the wiki link graph (nodes + edges) for visualization.
#[tauri::command]
pub fn wiki_get_graph(db_state: tauri::State<'_, DbState>) -> Result<engine::WikiGraph, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let graph = engine::build_graph(&root)?;
    Ok(graph)
}

/// Run the LLM wiki ingest: build prompt batches from raw sources, dispatch
/// them to the LLM in parallel (bounded by the orchestrator's concurrency
/// limit), write the generated pages, rebuild FTS5, and clear staleness.
#[tauri::command]
pub async fn wiki_ingest(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
) -> Result<ingest::IngestReport, AppError> {
    let (root, config) = {
        let conn = lock_conn(&db_state)?;
        let root = storage::resolve_root(&conn)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        raw_export::process_user_files(&root)?;
        // Self-heal: ensure AGENTS.md exists so the wiki-view UI does not gate
        // the generated pages behind the "Initialize" empty-state.
        let _ = ensure_initialized(&root);
        (root, config)
    };

    // Build batches (with author manifest if multi-batch) inside a DB scope.
    let batches = {
        let mut conn = lock_conn(&db_state)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    let mut report = ingest::run_chunked_ingest(&root, batches, sender, None, (25, 95)).await?;

    let conn = lock_conn(&db_state)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;
    Ok(report)
}

/// Build ingest batches with the deterministic pre-seed foundation:
///
/// 1. Run the full 8-step bibliometric normalization so `biblio_authors`
///    (with metrics), `biblio_terms`, and `biblio_article_terms` are populated.
/// 2. **Pre-seed author pages** (`wiki/authors/`) from `biblio_authors`.
/// 3. **Pre-seed synthesis pages** (`wiki/synthesis/`) from each included
///    article's `full_text_ai_summary` JSON - no LLM call needed.
/// 4. **Pre-seed concept hubs** (`wiki/concepts/`) from the top-N terms in
///    `biblio_terms` - no LLM call needed.
/// 5. Inject the author manifest into every batch prompt (so the LLM links to
///    canonical author slugs instead of inventing its own).
///
/// This deterministic foundation runs unconditionally (both single-batch and
/// multi-batch runs), guaranteeing author + synthesis + concept pages exist
/// regardless of which LLM model is used or how many batches the corpus splits
/// into. The LLM's role becomes cross-cutting thematic synthesis only.
///
/// Reviewed (user-edited) pages of each type are preserved by the pre-seeders.
fn build_batches_with_manifest(
    conn: &mut rusqlite::Connection,
    root: &std::path::Path,
    config: &crate::models::llm_config::LlmConfig,
) -> Result<Vec<ingest::IngestBatch>, AppError> {
    // Run the full 8-step bibliometric normalization pipeline so
    // `biblio_authors` (with metrics), `biblio_terms`, `biblio_article_terms`,
    // and the co-author/citation networks are all populated. This is the
    // single source of truth for all three pre-seed layers.
    //
    // Non-fatal: if normalization fails (e.g. empty corpus), we still proceed
    // so the LLM can operate on the raw sources alone. The pre-seeders will
    // simply find no rows and write nothing.
    let _ = crate::db::biblio_repo::run_full_normalization(conn);

    // Phase 1: Pre-seed author pages from `biblio_authors`.
    let manifest = ingest::build_author_manifest(conn)?;
    if !manifest.entries.is_empty() {
        // Errors are non-fatal: the LLM can still produce author pages itself,
        // and the consolidation pass will dedup them.
        let _ = ingest::preseed_authors(root, &manifest);
    }

    // Phase 2: Pre-seed synthesis pages from AI summaries.
    // Each included article with a `full_text_ai_summary` gets a synthesis page
    // whose slug = the article UUID (so [[uuid]] links resolve automatically).
    let _ = ingest::preseed_synthesis_from_ai_summaries(conn, root);

    // Phase 3: Pre-seed concept hubs from `biblio_terms`.
    // Caps at 25 terms so the concept layer stays curated + high-signal.
    let _ = ingest::preseed_concept_hubs(conn, root, 25);

    // Layer 1 (External Documents): Pre-seed source pages for user-uploaded
    // documents (Add Documents). Each external doc in `raw/` with a
    // `source_kind: user_*` gets a first-class wiki node at
    // `wiki/sources/{slug}.md` so `[[user-slug]]` wikilinks and
    // `[^art-user-slug]` footnote refs resolve to a navigable page.
    let _ = ingest::preseed_document_source_pages(root);

    // Rebuild batches with the manifest injected (when non-empty). The
    // manifest's `to_prompt_section()` directive tells the LLM NOT to create
    // author pages and to link to the canonical slugs instead.
    if manifest.entries.is_empty() {
        ingest::build_ingest_prompt_batches(root, config.context_window_tokens, None)
    } else {
        ingest::build_ingest_prompt_batches(root, config.context_window_tokens, Some(&manifest))
    }
}

/// Lightweight page summary for the sidebar list (no body).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageSummary {
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub status: String,
    pub summary: String,
}

/// List all wiki pages (metadata only, no body). Sorted by type then title.
#[tauri::command]
pub fn wiki_list_pages(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<WikiPageSummary>, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let pages = fts::collect_wiki_pages(&root)?;
    let mut summaries: Vec<WikiPageSummary> = pages
        .iter()
        .filter_map(|path| {
            let (fm, _body) = frontmatter::read_file(path).ok()?;
            let slug = fm.get("slug")?.to_string();
            Some(WikiPageSummary {
                slug: slug.clone(),
                title: fm.get("title").unwrap_or(&slug).to_string(),
                page_type: fm.get("type").unwrap_or("concept").to_string(),
                status: fm.get("status").unwrap_or("draft").to_string(),
                summary: fm.get("summary").unwrap_or("").to_string(),
            })
        })
        .collect();
    summaries.sort_by(|a, b| {
        a.page_type
            .cmp(&b.page_type)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Ok(summaries)
}

/// A raw source article's metadata for reference resolution.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiSourceInfo {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
}

/// List all raw source articles (metadata for [^art-id] reference resolution).
#[tauri::command]
pub fn wiki_list_sources(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<WikiSourceInfo>, AppError> {
    let conn = lock_conn(&db_state)?;
    let root = storage::resolve_root(&conn)?;
    let raw_files = raw_export::list_raw_files(&root)?;
    let sources: Vec<WikiSourceInfo> = raw_files
        .iter()
        .filter_map(|(_path, fm)| {
            let id = fm.get("id")?.to_string();
            // Only include source-type files (skip user-added non-article files).
            if fm.get("type") != Some("source") {
                return None;
            }
            let title = fm.get("title").unwrap_or(&id).to_string();
            let authors: Vec<String> = fm
                .get("authors")
                .map(|a| {
                    a.trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let year = fm.get("year").and_then(|y| y.parse::<i32>().ok());
            let doi = fm.get("doi").map(str::to_string);
            Some(WikiSourceInfo { id, title, authors, year, doi })
        })
        .collect();
    Ok(sources)
}

/// Total steps in the wiki rebuild pipeline (for the progress bar).
pub const WIKI_PIPELINE_TOTAL_STEPS: usize = 100;

/// Progress payload emitted via the `wiki:progress` event.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WikiProgress {
    pub step: usize,
    pub total_steps: usize,
    pub message: String,
}

/// Emit a `wiki:progress` event.
fn emit_wiki_progress(app_handle: &tauri::AppHandle, step: usize, message: &str) {
    let _ = app_handle.emit(
        "wiki:progress",
        WikiProgress { step, total_steps: WIKI_PIPELINE_TOTAL_STEPS, message: message.to_string() },
    );
}

/// Log a wiki ingest error to the audit table for later lookup.
fn log_wiki_error(conn: &rusqlite::Connection, error_msg: &str) {
    let _ = audit_repo::create_entry(
        conn,
        "", // no specific article - wiki-level error
        "wiki_ingest_error",
        None,
        None,
        Some(error_msg),
        "system",
    );
}

/// Full rebuild: scaffold (if needed) + export included articles + process user files
/// + LLM ingest + FTS5 rebuild. Emits `wiki:progress` at each step.
/// This is the one-click "Re-scaffold" action.
#[tauri::command]
pub async fn wiki_rebuild(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    let result = wiki_rebuild_inner(&db_state, &orchestrator, &app_handle).await;
    if let Err(ref e) = result {
        if let Ok(conn) = lock_conn(&db_state) {
            log_wiki_error(&conn, &e.to_string());
        }
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    result
}

/// Inner implementation of wiki_rebuild (without error logging wrapper).
async fn wiki_rebuild_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    emit_wiki_progress(app_handle, 0, "Starting wiki rebuild...");

    // Step 0: Scaffold (ensure wiki-root exists) + self-heal AGENTS.md.
    {
        let conn = lock_conn(db_state)?;
        let root = storage::resolve_root(&conn)?;
        storage::scaffold_tree(&root)?;
        let _ = ensure_initialized(&root);
    }
    emit_wiki_progress(app_handle, 10, "Wiki directory ready");

    // Step 1: Export included articles + process user files.
    let (root, config) = {
        let conn = lock_conn(db_state)?;
        let root = storage::resolve_root(&conn)?;
        raw_export::prepare_all(&conn, &root)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        (root, config)
    };
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Step 2: Build prompt batches + dispatch them to the LLM in parallel.
    // Each batch carries the full source index, so batches are independent and
    // safe to run concurrently. Progress emits as each batch completes. When
    // the corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let batches = {
        let mut conn = lock_conn(db_state)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
    let mut report =
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95)).await?;

    // Step 3: Finalize (FTS5 rebuild + log + clear staleness).
    emit_wiki_progress(app_handle, 95, "Indexing pages...");
    let conn = lock_conn(db_state)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;

    emit_wiki_progress(app_handle, 100, &format!("Done: {} pages written", report.pages_written));
    Ok(report)
}

/// Export raw + ingest in one call (used after "Add Documents").
/// Emits `wiki:progress` at each step.
#[tauri::command]
pub async fn wiki_export_and_ingest(
    db_state: tauri::State<'_, DbState>,
    orchestrator: tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    let result = wiki_export_and_ingest_inner(&db_state, &orchestrator, &app_handle).await;
    if let Err(ref e) = result {
        if let Ok(conn) = lock_conn(&db_state) {
            log_wiki_error(&conn, &e.to_string());
        }
        emit_wiki_progress(&app_handle, WIKI_PIPELINE_TOTAL_STEPS, &format!("Error: {}", e));
    }
    result
}

/// Inner implementation of wiki_export_and_ingest (without error logging wrapper).
async fn wiki_export_and_ingest_inner(
    db_state: &tauri::State<'_, DbState>,
    orchestrator: &tauri::State<'_, std::sync::Arc<crate::llm::orchestrator::LlmOrchestrator>>,
    app_handle: &tauri::AppHandle,
) -> Result<ingest::IngestReport, AppError> {
    emit_wiki_progress(app_handle, 0, "Preparing raw sources...");

    let (root, config) = {
        let conn = lock_conn(db_state)?;
        let root = storage::resolve_root(&conn)?;
        raw_export::prepare_all(&conn, &root)?;
        let config = crate::db::llm_config_repo::get_config(&conn)?.ok_or_else(|| {
            AppError::Validation(
                "LLM not configured. Please set up LLM configuration first.".to_string(),
            )
        })?;
        // Self-heal: ensure AGENTS.md exists so the wiki-view UI does not gate
        // the generated pages behind the "Initialize" empty-state.
        let _ = ensure_initialized(&root);
        (root, config)
    };
    emit_wiki_progress(app_handle, 15, "Raw sources prepared");

    // Build prompt batches + dispatch them to the LLM in parallel. When the
    // corpus splits into multiple batches, the author manifest + pre-seed
    // optimization is applied to prevent cross-batch duplication.
    let batches = {
        let mut conn = lock_conn(db_state)?;
        build_batches_with_manifest(&mut conn, &root, &config)?
    };
    let sender: Arc<dyn ingest::IngestLlmSender> =
        Arc::new(ingest::OrchestratorIngestSender::new(orchestrator.inner().clone(), config));
    emit_wiki_progress(app_handle, 25, "Generating wiki pages via LLM...");
    let mut report =
        ingest::run_chunked_ingest(&root, batches, sender, Some(app_handle), (25, 95)).await?;

    // Finalize (FTS5 rebuild + log + clear staleness).
    emit_wiki_progress(app_handle, 95, "Indexing pages...");
    let conn = lock_conn(db_state)?;
    ingest::finalize_ingest(&conn, &root, &mut report)?;

    emit_wiki_progress(app_handle, 100, &format!("Done: {} pages written", report.pages_written));
    Ok(report)
}

/// Helper used by tests and (later) other commands to resolve the root without
/// going through Tauri state. Not a `#[tauri::command]`.
#[allow(dead_code)]
pub(crate) fn root_for_conn(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    storage::resolve_root(conn)
}
