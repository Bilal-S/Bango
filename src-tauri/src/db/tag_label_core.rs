//! Shared helpers for the parallel `tags`/`labels` repos.
//!
//! The spec treats tags and labels as distinct domain concepts, so repos,
//! models, and commands stay separate; only the mechanical single-row SQL
//! shapes live here (lookup, dedupe-exists). The 5-line rename/color/merge/
//! delete mutations stay in each repo. Table names are compile-time
//! constants from the wrappers, never user input; names always stay bound
//! parameters.

use rusqlite::{params, Connection};

/// One raw `id, name, source, color` row from `tags` or `labels`.
pub struct NamedEntityRow {
    pub id: String,
    pub name: String,
    pub source: String,
    pub color: Option<String>,
}

/// Read one `id, name, source, color` tuple from the current result row.
pub fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NamedEntityRow> {
    Ok(NamedEntityRow {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        color: row.get(3)?,
    })
}

/// Case-insensitive single-row lookup by name (`LOWER(name) = LOWER(?1)`).
///
/// Mirrors the historical `.ok()` semantics: any lookup miss (including a
/// transient error) returns `None` so the caller proceeds to INSERT. This is
/// a deliberate behavior-preserving choice for the refactor (callers rely on
/// the fall-through dedupe flow), not an endorsed error-handling pattern.
pub fn find_by_normalized_name(
    conn: &Connection,
    table: &'static str,
    name: &str,
) -> Option<NamedEntityRow> {
    conn.query_row(
        &format!("SELECT id, name, source, color FROM {table} WHERE LOWER(name) = LOWER(?1)"),
        params![name],
        read_row,
    )
    .ok()
}

/// Whether a case-insensitive name match already exists.
///
/// Mirrors the historical `.unwrap_or(false)` semantics: a transient DB error
/// reads as "does not exist" so the caller proceeds to INSERT. Deliberate
/// behavior preservation, not an endorsed error-handling pattern.
pub fn exists_normalized(conn: &Connection, table: &'static str, name: &str) -> bool {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE LOWER(name) = LOWER(?1)"),
        params![name],
        |row| row.get::<_, usize>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}
