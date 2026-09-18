//! Obsidian vault export (single-step: stage + zip).
//!
//! Pre-processes the wiki Markdown tree into a UUID-free vault (see
//! `wiki::obsidian_export`), stages it to `wiki-root/obsidian-export/`
//! (cleared on each run, mirroring `wiki-export/`), and zips it to the
//! user-chosen path. Single-step: no browser preview, so no generate/zip
//! split.

use std::path::PathBuf;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::obsidian_export::{self, ArticleRow};
use crate::wiki::storage;

use super::emit_wiki_progress;
use super::site_export::{count_files_recursive, zip_directory};

/// Result of `wiki_export_obsidian`: file count, destination path, and the
/// vault rewrite stats (for the success toast).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianExportResult {
    /// Total files in the zip (pages + Home.md + `.obsidian/` config).
    pub file_count: usize,
    /// Absolute path the zip was written to.
    pub path: String,
    /// Vault rewrite stats.
    pub stats: obsidian_export::VaultStats,
}

/// Export the wiki as an Obsidian-ready vault zip.
///
/// The `path` comes from the frontend's `save()` dialog (matching the
/// `wiki_zip_export` / `export_ris_to_file` patterns). The DB mutex is held
/// only to resolve the wiki root + load the article rows; staging + zipping
/// run lock-free. Emits `wiki:progress` events at each step.
#[tauri::command]
pub async fn wiki_export_obsidian(
    db_state: tauri::State<'_, DbState>,
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<ObsidianExportResult, AppError> {
    // Brief lock: resolve root + load the article rows for the slug map.
    let (root, rows) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let root = storage::resolve_root(&conn)?;
        let rows = load_article_rows(&conn)?;
        (root, rows)
    };

    emit_wiki_progress(&app_handle, 10, "Building article slug map...");

    let map = obsidian_export::build_article_slug_map(&rows);

    let staging = root.join("obsidian-export");
    let stats = obsidian_export::write_vault(&root.join("wiki"), &staging, &map)?;

    emit_wiki_progress(&app_handle, 70, "Zipping Obsidian vault...");

    // Zip to a temp file next to the staging dir, then move (cross-volume safe).
    let temp_zip = staging.with_extension("zip.tmp");
    zip_directory(&staging, &temp_zip)?;

    let dest = PathBuf::from(&path);
    std::fs::rename(&temp_zip, &dest).or_else(|_| {
        std::fs::copy(&temp_zip, &dest)?;
        std::fs::remove_file(&temp_zip)
    })?;

    let file_count = count_files_recursive(&staging)?;

    emit_wiki_progress(
        &app_handle,
        100,
        &format!("Obsidian vault saved to {} ({file_count} files)", dest.display()),
    );

    Ok(ObsidianExportResult { file_count, path: dest.to_string_lossy().to_string(), stats })
}

/// Load `(id, title, year, authors)` for every article. Status-agnostic: the
/// wiki only ever references included articles, but a whole-corpus map is
/// simpler and safe. Ordered by `sequence_id` so suffix assignment on slug
/// collisions is stable across runs.
fn load_article_rows(conn: &rusqlite::Connection) -> Result<Vec<ArticleRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, publication_year, authors FROM articles ORDER BY sequence_id",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let year: Option<i32> = row.get(2)?;
        // Canonical Article deserialization (mirrors `article_repo`): the
        // column stores a JSON array of author strings.
        let authors_str: String = row.get(3)?;
        let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();
        Ok(ArticleRow { id, title, year, authors })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
