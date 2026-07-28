//! Search, lint, and graph commands.
//!
//! Extracted from the pre-split `wiki_cmd.rs` (refactor v6). Bodies moved
//! VERBATIM; no behavioral change.

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::wiki::{engine, frontmatter, fts, storage};

// Allow frontmatter import to be referenced (used by tests + future lint command).
#[allow(dead_code)]
fn _ensure_frontmatter_linked() {
    let _ = frontmatter::Frontmatter::default();
}

/// Search the wiki FTS5 index. Returns BM25-ranked hits.
#[tauri::command]
pub fn wiki_search(
    db_state: tauri::State<'_, DbState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<fts::WikiPageHit>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    // Self-heal: rebuild the index if it is empty but pages exist on disk
    // (e.g. after a schema rebuild / DB reset that dropped the FTS table).
    fts::ensure_index_populated(&conn, &root)?;
    fts::search(&conn, &query, limit.unwrap_or(10))
}

/// Lint the wiki: detect broken links, orphans, duplicates, missing frontmatter.
#[tauri::command]
pub fn wiki_lint(db_state: tauri::State<'_, DbState>) -> Result<engine::LintReport, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let report = engine::lint(&root)?;
    Ok(report)
}

/// Get the wiki link graph (nodes + edges) for visualization.
#[tauri::command]
pub fn wiki_get_graph(db_state: tauri::State<'_, DbState>) -> Result<engine::WikiGraph, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let root = storage::resolve_root(&conn)?;
    let graph = engine::build_graph(&root)?;
    Ok(graph)
}
