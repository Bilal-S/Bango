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

/// Ensure the FTS5 table exists AND is in sync with the wiki pages on disk.
/// Self-heals two desync cases:
///
/// 1. **Empty index, pages on disk** - happens after a schema rebuild /
///    `rebuild_schema` / DB reset that drops the FTS table while leaving the
///    `wiki/*.md` files intact.
/// 2. **Count mismatch** - the index has a different number of rows than there
///    are `.md` pages on disk. Happens when pages were added/removed on disk
///    by an ingest run that predates the rebuild-on-mutation fix, or any other
///    path that wrote pages without touching the index. (Detects added OR
///    removed pages via a cheap count comparison, not a full content diff.)
///
/// Returns `true` if a rebuild was performed, `false` if the index was already
/// in sync (or there are no pages to index). Read paths (`wiki_chat`,
/// `wiki_search`) call this instead of `ensure_table` so the first access after
/// a desync transparently recovers with no user action.
pub fn ensure_index_populated(conn: &Connection, root: &Path) -> Result<bool, AppError> {
    ensure_table(conn)?;
    let row_count: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM {FTS_TABLE}"), [], |row| row.get(0))
        .unwrap_or(0);
    let pages = collect_wiki_pages(root)?;
    let disk_count = pages.len() as i64;

    if row_count == 0 {
        // Case 1: empty index. Rebuild only if there are pages to index.
        if !pages.is_empty() {
            rebuild_index(conn, root)?;
            return Ok(true);
        }
        // No pages on disk and empty index -> nothing to do.
        return Ok(false);
    }

    // Case 2: index non-empty but row count differs from disk count. Rebuild
    // to pick up added/removed pages. (When counts match we assume in-sync;
    // in-place edits of existing pages are handled by the rebuild-on-mutation
    // hooks in `wiki_update_page` / `wiki_delete_page`.)
    if row_count != disk_count {
        rebuild_index(conn, root)?;
        return Ok(true);
    }

    Ok(false)
}

/// Rebuild the index from scratch by scanning `wiki/` for `.md` files.
/// Drops and recreates the FTS table, then inserts every page.
///
/// Uses `collect_wiki_pages` (which excludes top-level internal files like
/// `log.md` / `index.md`) so the FTS search index matches the page list shown
/// in the UI — internal infrastructure never surfaces in chat or search.
pub fn rebuild_index(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    // Drop existing content and recreate.
    conn.execute_batch(&format!("DELETE FROM {FTS_TABLE};"))?;
    ensure_table(conn)?;

    let wiki_dir = root.join("wiki");
    let pages = collect_wiki_pages(root)?;
    for path in &pages {
        index_single_file(conn, path, &wiki_dir)?;
    }
    Ok(pages.len())
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
/// The user query is converted to a safe FTS5 MATCH expression by
/// `build_match_query`: tokens are split on whitespace/punctuation, stop words
/// are dropped, and each remaining token is phrase-quoted (so FTS5 treats it
/// as a literal string, never as an operator) and OR-joined. This retrieves
/// any page containing any meaningful term, ranked by BM25 — the standard
/// RAG-over-FTS5 pattern. (The previous implementation phrase-quoted the whole
/// question, which only matched documents containing those exact words in that
/// exact sequence, so natural-language questions like "Who is J Adams?"
/// returned zero hits.)
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<WikiPageHit>, AppError> {
    let safe_query = build_match_query(query);
    // Empty query (e.g. only stop words / punctuation) -> no results, no error.
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
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

/// A small set of English stop words dropped from the MATCH query to avoid
/// surfacing only pages that happen to contain common particles. Tokens here
/// are matched case-insensitively against lowercased input.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "will", "with", "who", "what", "when", "where", "why", "how", "i",
    "you", "we", "he", "she", "they", "me", "him", "her", "us", "do", "does", "did", "can",
    "could", "would", "should", "my", "your", "our",
];

/// Build a safe, effective FTS5 MATCH expression from a natural-language query.
///
/// - Splits on any non-alphanumeric character (so punctuation/whitespace both
///   separate tokens).
/// - Lowercases for stop-word comparison (FTS5's `unicode61` tokenizer is
///   case-insensitive, so casing in the emitted query is irrelevant).
/// - Drops stop words (`who`, `is`, `the`, ...).
/// - Phrase-quotes each remaining token with embedded `"` doubled, so FTS5
///   treats the token as a literal string (never an operator like `AND`/`OR`/
///   `*`/`:`).
/// - OR-joins the quoted tokens so any page containing any meaningful term is
///   a candidate, BM25-ranked.
/// - If stop-word stripping removes everything, falls back to OR-joining all
///   literal tokens (so a query like "the and is" still searches rather than
///   silently returning nothing).
/// - Returns an empty `String` only when there are no alphanumeric tokens at
///   all (e.g. `"???"`); callers treat that as "no results".
#[must_use]
pub fn build_match_query(query: &str) -> String {
    // Split on any run of non-alphanumeric characters.
    let raw_tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect();
    if raw_tokens.is_empty() {
        return String::new();
    }

    // Prefer tokens that are not stop words; fall back to all tokens if that
    // would leave nothing.
    let stop = |t: &str| STOP_WORDS.contains(&t);
    let meaningful: Vec<&String> = raw_tokens.iter().filter(|t| !stop(t.as_str())).collect();
    let tokens: Vec<&String> =
        if meaningful.is_empty() { raw_tokens.iter().collect() } else { meaningful };

    tokens
        .iter()
        .map(|t| {
            // Phrase-quote each token, doubling any embedded double quote so
            // FTS5 reads it literally.
            let escaped = t.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Top-level wiki `.md` files that are internal infrastructure (not wiki
/// "pages"). These are excluded from the page list shown in the UI and from
/// the FTS search index so they don't surface as navigable pages. A same-named
/// file inside a subdirectory (e.g. `wiki/concepts/log.md`) is still listed —
/// only direct children of `wiki/` are filtered.
const INTERNAL_WIKI_FILES: &[&str] = &["log", "index"];

/// Collect all wiki `.md` file paths (for listing in the UI).
///
/// Excludes top-level internal infrastructure files (`log.md`, `index.md`) so
/// the audit trail and master catalog don't surface as navigable pages.
pub fn collect_wiki_pages(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let wiki_dir = root.join("wiki");
    if !wiki_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_md_recursive(&wiki_dir, &mut out)?;
    // Exclude top-level internal infrastructure files (log.md, index.md) —
    // they have no frontmatter slug/type and would surface as raw "log" /
    // "index" entries in the sidebar. Only direct children of wiki/ are
    // filtered; a wiki/concepts/log.md page is still listed.
    out.retain(|path| {
        let is_top_level =
            path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("wiki");
        if !is_top_level {
            return true;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        !INTERNAL_WIKI_FILES.contains(&stem)
    });
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
    fn ensure_index_populated_rebuilds_when_empty_but_pages_exist() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax levy");
        write_wiki_page(root, "authors", "jane-doe", "Jane Doe", "nutrition researcher");

        let conn = Connection::open_in_memory().unwrap();
        // Table exists but is empty (the desync state after a schema rebuild).
        ensure_table(&conn).unwrap();
        let hits_before = search(&conn, "sugar", 5).unwrap();
        assert!(hits_before.is_empty(), "index should be empty before self-heal");

        // Self-heal fires because there are pages on disk but 0 indexed rows.
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(rebuilt, "should report a rebuild was performed");
        let hits_after = search(&conn, "sugar", 5).unwrap();
        assert!(!hits_after.is_empty(), "search should find results after self-heal");
        let slugs: Vec<&str> = hits_after.iter().map(|h| h.slug.as_str()).collect();
        assert!(slugs.contains(&"sugar-tax"));
    }

    #[test]
    fn ensure_index_populated_returns_false_when_already_populated() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // Already populated -> no rebuild needed.
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(!rebuilt, "should not rebuild an already-populated index");
    }

    #[test]
    fn ensure_index_populated_returns_false_when_no_pages_exist() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Scaffold wiki/ dir but with no pages.
        std::fs::create_dir_all(root.join("wiki")).unwrap();

        let conn = Connection::open_in_memory().unwrap();
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(!rebuilt, "should not rebuild when there are no pages to index");
    }

    #[test]
    fn ensure_index_populated_rebuilds_when_index_count_mismatches_disk() {
        // Reproduces the "J Adams on disk but not in Chat" staleness: pages
        // exist on disk, the index is non-empty, but its row count differs
        // from the disk page count (e.g. a new page was added by an ingest run
        // that did not refresh the index).
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Start with 3 pages and a fully-built index.
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");
        write_wiki_page(root, "concepts", "beta", "Beta", "beta content");
        write_wiki_page(root, "authors", "jane-doe", "Jane Doe", "nutrition researcher");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();
        // Sanity: index has 3 rows, "jane-doe" is retrievable.
        assert!(search(&conn, "jane", 5).unwrap().iter().any(|h| h.slug == "jane-doe"));

        // Add a 4th page on disk WITHOUT rebuilding the index (simulates a
        // stale index after an ingest that pre-dates the mutation hooks).
        write_wiki_page(root, "authors", "j-adams", "J Adams", "adams sugar policy");
        // The new page is not yet retrievable.
        assert!(
            search(&conn, "adams", 5).unwrap().is_empty(),
            "new page should not be in the stale index"
        );

        // Self-heal detects the count mismatch (index=3, disk=4) and rebuilds.
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(rebuilt, "should rebuild on count mismatch");
        // Now the previously-missing page is retrievable.
        let hits = search(&conn, "adams", 5).unwrap();
        assert!(
            hits.iter().any(|h| h.slug == "j-adams"),
            "J Adams should be retrievable after self-heal"
        );
    }

    #[test]
    fn ensure_index_populated_rebuilds_when_index_has_more_rows_than_disk() {
        // Inverse case: a page was deleted on disk without updating the index.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");
        write_wiki_page(root, "concepts", "beta", "Beta", "beta content");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // Delete one page from disk (bypassing wiki_delete_page).
        std::fs::remove_file(root.join("wiki/concepts/beta.md")).unwrap();

        // Index still has 2 rows but disk has 1 -> mismatch -> rebuild.
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(rebuilt, "should rebuild when index has stale extra rows");
        // The deleted page is no longer retrievable.
        assert!(search(&conn, "beta", 5).unwrap().is_empty());
    }

    #[test]
    fn search_finds_pages_by_any_token_in_natural_language_question() {
        // Regression: the old phrase-query implementation returned 0 hits for
        // multi-word natural-language questions because no page contained the
        // exact phrase. The token-based OR query must find pages by any term.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(
            root,
            "authors",
            "author-adams-j",
            "J Adams",
            "J Adams appears across several SDIL studies on household purchases.",
        );
        write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "levy on sugary drinks");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // Natural-language question (the exact case the bug was reported on).
        let hits = search(&conn, "Who is J Adams?", 5).unwrap();
        assert!(!hits.is_empty(), "natural-language question must find pages");
        assert!(hits.iter().any(|h| h.slug == "author-adams-j"), "should find the J Adams page");
    }

    #[test]
    fn build_match_query_drops_stop_words_and_or_joins_meaningful_tokens() {
        // "Who is J Adams?" -> stop words (who, is) dropped -> "J" OR "Adams".
        let q = build_match_query("Who is J Adams?");
        assert_eq!(q, "\"j\" OR \"adams\"");
    }

    #[test]
    fn build_match_query_handles_punctuation_as_token_separator() {
        // Punctuation splits tokens; only meaningful tokens are emitted.
        let q = build_match_query("What is the sugar-tax?");
        assert_eq!(q, "\"sugar\" OR \"tax\"");
    }

    #[test]
    fn build_match_query_treats_fts_operators_as_literal_tokens() {
        // FTS5 keywords and wildcards must be phrase-quoted so they are treated
        // as literal strings, not operators.
        let q = build_match_query("sugar AND tax OR levy*");
        // "and"/"or" are stop words -> dropped. "sugar", "tax", "levy" quoted.
        assert_eq!(q, "\"sugar\" OR \"tax\" OR \"levy\"");
        // Ensure the literal operator strings are quoted (not bare).
        assert!(!q.contains(" OR AND") && !q.contains(" OR OR"));
    }

    #[test]
    fn build_match_query_falls_back_to_all_tokens_when_only_stop_words() {
        // If every token is a stop word, fall back to OR-joining all of them
        // rather than returning an empty query.
        let q = build_match_query("the and is");
        assert_eq!(q, "\"the\" OR \"and\" OR \"is\"");
    }

    #[test]
    fn build_match_query_returns_empty_when_no_alphanumeric_tokens() {
        // Pure punctuation / empty -> empty string -> caller returns no hits.
        assert_eq!(build_match_query("??? !!!"), "");
        assert_eq!(build_match_query(""), "");
    }

    #[test]
    fn build_match_query_single_token_no_or() {
        // A single meaningful token is emitted as one phrase-quoted term.
        assert_eq!(build_match_query("sugar"), "\"sugar\"");
        // "the sugar" -> stop word dropped -> single token.
        assert_eq!(build_match_query("the sugar"), "\"sugar\"");
    }

    #[test]
    fn search_returns_empty_for_pure_stopword_question_without_erroring() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // A question with only stop words falls back to searching them; they
        // aren't in the index, so 0 hits — but no error.
        let hits = search(&conn, "the and is", 5).unwrap();
        assert!(hits.is_empty());

        // Pure punctuation -> empty query -> empty hits, no error.
        let hits2 = search(&conn, "???", 5).unwrap();
        assert!(hits2.is_empty());
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

    #[test]
    fn collect_wiki_pages_excludes_top_level_log_and_index() {
        // Internal infrastructure files at the wiki/ root must not surface as
        // navigable pages in the sidebar or the FTS search index.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
        // Top-level internal files (no frontmatter; raw audit trail / catalog).
        let wiki_dir = root.join("wiki");
        std::fs::write(wiki_dir.join("log.md"), "# Wiki Audit Log\n\nentry.").unwrap();
        std::fs::write(wiki_dir.join("index.md"), "# Wiki Index\n\n- [alpha](concepts/alpha.md)")
            .unwrap();

        let pages = collect_wiki_pages(root).unwrap();
        let stems: Vec<&str> =
            pages.iter().filter_map(|p| p.file_stem().and_then(|s| s.to_str())).collect();
        assert!(stems.contains(&"alpha"), "concept page should be listed");
        assert!(!stems.contains(&"log"), "top-level log.md must be excluded");
        assert!(!stems.contains(&"index"), "top-level index.md must be excluded");
        assert_eq!(pages.len(), 1, "only the concept page should be listed");
    }

    #[test]
    fn collect_wiki_pages_keeps_subdirectory_log_or_index() {
        // A same-named page inside a subdir is a legitimate wiki page and must
        // still be listed — only direct children of wiki/ are filtered.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
        write_wiki_page(root, "concepts", "log", "Log Concept", "a logging concept page");
        write_wiki_page(root, "authors", "index", "Index Author", "an author named Index");

        let pages = collect_wiki_pages(root).unwrap();
        let stems: Vec<&str> =
            pages.iter().filter_map(|p| p.file_stem().and_then(|s| s.to_str())).collect();
        assert!(stems.contains(&"alpha"));
        assert!(stems.contains(&"log"), "wiki/concepts/log.md must be listed");
        assert!(stems.contains(&"index"), "wiki/authors/index.md must be listed");
        assert_eq!(pages.len(), 3);
    }

    #[test]
    fn rebuild_index_excludes_top_level_log_and_index() {
        // The FTS index must also exclude internal files so chat / search do
        // not surface them as retrievable pages.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax levy");
        std::fs::write(root.join("wiki/log.md"), "ingest run audit entry mentioning sugar")
            .unwrap();

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        let count = rebuild_index(&conn, root).unwrap();
        assert_eq!(count, 1, "log.md must not be indexed");
        // The log.md content mentions sugar but must NOT be retrievable.
        let hits = search(&conn, "sugar", 5).unwrap();
        assert!(hits.iter().all(|h| h.slug == "sugar-tax"), "log.md must not surface in search");
        assert_eq!(hits.len(), 1);
    }
}
