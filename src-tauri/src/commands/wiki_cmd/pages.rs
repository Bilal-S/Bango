//! Page CRUD + page/source listings.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use crate::db::app_settings_repo::clear_wiki_needs_refresh;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{frontmatter, fts, raw_export, storage};

use serde::Serialize;

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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
            // cheap (dozens to low hundreds of local pages) and avoids fragile
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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

/// Delete the entire wiki: removes the `wiki/` subtree AND `AGENTS.md` so the
/// wiki is fully de-initialized (`status.initialized` becomes `false`).
///
/// Keeps `raw/` and `templates/` so the user's source documents are preserved
/// for a future rebuild. The user must explicitly re-initialize via the
/// "Initialize & Build Wiki" button (or Rebuild Wiki action), which
/// re-scaffolds the tree + writes `AGENTS.md` via the self-healing
/// `ensure_initialized` guard.
///
/// Also clears the `wiki_needs_refresh` staleness flag so the stale badge
/// does not appear after deletion. Auto-ingest on tab visit has been removed
/// entirely (replaced by an explicit Update button in the toolbar); the flag
/// clear is defense-in-depth.
#[tauri::command]
pub fn wiki_delete_wiki(db_state: tauri::State<'_, DbState>) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let wiki_dir = root.join("wiki");
    if wiki_dir.exists() {
        std::fs::remove_dir_all(&wiki_dir)?;
    }
    // Remove AGENTS.md to de-initialize the wiki. The status command reports
    // `initialized: false` based on AGENTS.md presence, so the wiki-view shows
    // the "Initialize Your Wiki" empty-state card after deletion.
    let agents_path = root.join("AGENTS.md");
    if agents_path.exists() {
        std::fs::remove_file(&agents_path)?;
    }
    // Clear the staleness flag so the stale badge does not show after delete.
    clear_wiki_needs_refresh(&conn);
    Ok(())
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
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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

/// A raw source article's metadata for reference resolution + static-site
/// article stub rendering.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiSourceInfo {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    /// Article abstract (copyright-safe metadata). Used by the static-site
    /// exporter to render article-stub pages without a second DB query.
    pub abstract_text: String,
    /// Journal name (metadata). Used by the static-site exporter.
    pub journal: Option<String>,
}

/// List all raw source articles (metadata for [^art-id] reference resolution).
#[tauri::command]
pub fn wiki_list_sources(
    db_state: tauri::State<'_, DbState>,
) -> Result<Vec<WikiSourceInfo>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
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
            let abstract_text = fm.get("abstract_text").unwrap_or("").to_string();
            let journal = fm.get("journal").map(str::to_string);
            Some(WikiSourceInfo { id, title, authors, year, doi, abstract_text, journal })
        })
        .collect();
    Ok(sources)
}
