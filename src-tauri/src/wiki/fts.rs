//! FTS5 full-text search over wiki pages.
//!
//! Creates and maintains a `wiki_pages_fts` virtual table (FTS5, bundled in
//! `rusqlite`'s `bundled` feature) for BM25-ranked retrieval. Used by:
//! - Phase 5 `wiki_chat` (token-budgeted RAG retrieval).
//! - Phase 3 `wiki_ingest` (the index is rebuilt after pages are generated).
//!
//! The index mirrors the frontmatter + body of every `.md` page under
//! `wiki/` (not `raw/` — raw sources are ingested into wiki pages first).

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::AppError;
use crate::wiki::frontmatter;

/// The FTS5 virtual table name.
pub const FTS_TABLE: &str = "wiki_pages_fts";

/// Ensure the FTS5 table exists. Idempotent.
pub fn ensure_table(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {FTS_TABLE} USING fts5(
            slug,
            title,
            summary,
            body,
            page_type,
            source_articles UNINDEXED,
            file_path UNINDEXED,
            tokenize = 'porter unicode61'
        );"
    ))?;
    Ok(())
}

/// Rebuild the index from scratch by scanning `wiki/` for `.md` files.
/// Drops and recreates the FTS table, then inserts every page.
pub fn rebuild_index(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    // Drop existing content and recreate.
    conn.execute_batch(&format!("DELETE FROM {FTS_TABLE};"))?;
    ensure_table(conn)?;

    let wiki_dir = root.join("wiki");
    let mut count = 0usize;
    if wiki_dir.exists() {
        index_directory(conn, &wiki_dir, &wiki_dir, &mut count)?;
    }
    Ok(count)
}

/// Recursively index all `.md` files under `dir`.
fn index_directory(
    conn: &Connection,
    dir: &Path,
    wiki_root: &Path,
    count: &mut usize,
) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            index_directory(conn, &path, wiki_root, count)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            index_single_file(conn, &path, wiki_root)?;
            *count += 1;
        }
    }
    Ok(())
}

/// Index a single wiki `.md` file into the FTS table.
fn index_single_file(conn: &Connection, path: &Path, wiki_root: &Path) -> Result<(), AppError> {
    let (fm, body) = frontmatter::read_file(path)?;
    let slug = fm.get("slug").unwrap_or("").to_string();
    let title = fm.get("title").unwrap_or("").to_string();
    let summary = fm.get("summary").unwrap_or("").to_string();
    let page_type = fm.get("type").unwrap_or("").to_string();
    let source_articles = fm.get("source_articles").unwrap_or("").to_string();
    // Store a relative path from the wiki dir for compactness.
    let file_path = path
        .strip_prefix(wiki_root.parent().unwrap_or(Path::new("")))
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    conn.execute(
        &format!(
            "INSERT INTO {FTS_TABLE} (slug, title, summary, body, page_type, source_articles, file_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);"
        ),
        rusqlite::params![slug, title, summary, body, page_type, source_articles, file_path],
    )?;
    Ok(())
}

/// A search hit returned by `search`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageHit {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub page_type: String,
    pub source_articles: String,
    pub file_path: String,
    pub rank: f64,
}

/// BM25-ranked search over the FTS5 index. Returns up to `limit` hits.
///
/// The query is escaped for FTS5 by quoting it as a phrase match. This is a
/// conservative approach that prevents syntax errors from user input.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<WikiPageHit>, AppError> {
    // Build a safe FTS5 MATCH query: phrase-quote the whole query.
    // Double any embedded double quotes first.
    let safe_query = format!("\"{}\"", query.replace('"', "\"\""));
    let limit_i = limit.max(1) as i64;

    let mut stmt = conn.prepare(
        &format!(
            "SELECT slug, title, summary, body, page_type, source_articles, file_path, bm25({FTS_TABLE}) AS rank
             FROM {FTS_TABLE}
             WHERE {FTS_TABLE} MATCH ?1
             ORDER BY rank
             LIMIT ?2;"
        )
    )?;

    let rows = stmt.query_map(rusqlite::params![safe_query, limit_i], |row| {
        Ok(WikiPageHit {
            slug: row.get(0)?,
            title: row.get(1)?,
            summary: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            body: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            page_type: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            source_articles: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            file_path: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            rank: row.get::<_, Option<f64>>(7)?.unwrap_or(0.0),
        })
    })?;

    let mut hits = Vec::new();
    for row in rows {
        hits.push(row?);
    }
    Ok(hits)
}

/// Collect all wiki `.md` file paths (for listing in the UI).
pub fn collect_wiki_pages(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let wiki_dir = root.join("wiki");
    if !wiki_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_md_recursive(&wiki_dir, &mut out)?;
    // Sort by filename stem for stable display.
    out.sort_by(|a, b| {
        a.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .cmp(b.file_stem().and_then(|s| s.to_str()).unwrap_or(""))
    });
    Ok(out)
}

fn collect_md_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md_recursive(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_wiki_page(root: &Path, subdir: &str, slug: &str, title: &str, body: &str) {
        let dir = root.join("wiki").join(subdir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut fm = frontmatter::Frontmatter::default();
        fm.set("slug", slug);
        fm.set("title", title);
        fm.set("type", "concept");
        fm.set("summary", &format!("{title} summary"));
        fm.set("status", "draft");
        fm.set("source_articles", "[]");
        fm.set("links", "[]");
        frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
    }

    #[test]
    fn rebuild_indexes_all_wiki_pages() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(
            root,
            "concepts",
            "sugar-tax",
            "Sugar Tax",
            "# Sugar Tax\nA levy on sugary drinks.",
        );
        write_wiki_page(
            root,
            "concepts",
            "obesity",
            "Obesity",
            "# Obesity\nA major public health concern.",
        );
        write_wiki_page(
            root,
            "authors",
            "jane-doe",
            "Jane Doe",
            "# Jane Doe\nResearcher in nutrition.",
        );

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        let count = rebuild_index(&conn, root).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn search_returns_relevant_hits_by_bm25() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(
            root,
            "concepts",
            "sugar-tax",
            "Sugar Tax",
            "sugar tax beverage levy obesity",
        );
        write_wiki_page(
            root,
            "concepts",
            "obesity",
            "Obesity",
            "obesity public health body mass index",
        );
        write_wiki_page(
            root,
            "authors",
            "jane-doe",
            "Jane Doe",
            "nutrition researcher sugar consumption",
        );

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        let hits = search(&conn, "sugar", 5).unwrap();
        assert!(!hits.is_empty(), "should find results for 'sugar'");
        // Sugar Tax and Jane Doe should rank high (both contain "sugar").
        let slugs: Vec<&str> = hits.iter().map(|h| h.slug.as_str()).collect();
        assert!(slugs.contains(&"sugar-tax") || slugs.contains(&"jane-doe"));
    }

    #[test]
    fn search_handles_empty_results_gracefully() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        let hits = search(&conn, "zzznonexistent", 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_respects_limit() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        for i in 0..10 {
            write_wiki_page(
                root,
                "concepts",
                &format!("topic-{i}"),
                &format!("Topic {i}"),
                "common keyword shared",
            );
        }

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        let hits = search(&conn, "common", 3).unwrap();
        assert!(hits.len() <= 3);
    }

    #[test]
    fn search_escapes_quotes_in_query() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "test", "Test", "content with a quote here");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // A query with embedded double quotes should not error.
        let result = search(&conn, "\"quoted phrase\"", 5);
        assert!(result.is_ok());
    }

    #[test]
    fn rebuild_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        let c1 = rebuild_index(&conn, root).unwrap();
        let c2 = rebuild_index(&conn, root).unwrap();
        assert_eq!(c1, c2);
        // No duplicates.
        let hits = search(&conn, "alpha", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn collect_wiki_pages_lists_all_md_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
        write_wiki_page(root, "authors", "beta", "Beta", "b");

        let pages = collect_wiki_pages(root).unwrap();
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn collect_wiki_pages_handles_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let pages = collect_wiki_pages(tmp.path()).unwrap();
        assert!(pages.is_empty());
    }
}
