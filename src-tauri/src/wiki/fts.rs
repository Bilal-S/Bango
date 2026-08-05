//! FTS5 full-text search over wiki pages. Creates `wiki_pages_fts` virtual table for
//! BM25-ranked retrieval. Used by `wiki_chat` (RAG) and `wiki_ingest` (rebuilt after pages).
//! Index mirrors frontmatter + body of every `.md` under `wiki/` (not `raw/`).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::app_settings_repo;
use crate::error::AppError;
use crate::wiki::frontmatter;

/// The FTS5 virtual table name.
pub const FTS_TABLE: &str = "wiki_pages_fts";

/// `app_settings` key for the tier-1 directory fingerprint (stat-based hash over wiki pages).
/// Absent = "no baseline" → first check populates it.
pub const WIKI_DIR_HASH_KEY: &str = "wiki_dir_hash";

/// Ensure the FTS5 table exists with chunk-aware schema (T1.2). Idempotent.
/// Three `UNINDEXED` metadata columns for passage-level chunks with section provenance:
/// `chunk_index` (0-based, NULL = legacy whole-page), `section`, `parent_slug`.
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
            chunk_index UNINDEXED,
            section UNINDEXED,
            parent_slug UNINDEXED,
            tokenize = 'porter unicode61'
        );"
    ))?;
    Ok(())
}

/// Ensure FTS5 table exists AND is in sync with wiki pages on disk. Self-heals:
/// 1. Empty index with pages on disk (e.g. after schema rebuild).
/// 2. Count mismatch: `DISTINCT parent_slug` rows ≠ disk `.md` count (pages added/removed).
///
/// Returns `true` if rebuild performed, `false` if already in sync.
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

    /* Case 2: index non-empty but distinct page count ≠ disk count.
    After T1.2 the index is row-per-chunk, so raw COUNT would false-positive.
    Compare COUNT(DISTINCT parent_slug) instead so chunk rows don't desync self-heal. */
    let distinct_pages: i64 = conn
        .query_row(
            &format!("SELECT COUNT(DISTINCT COALESCE(parent_slug, slug)) FROM {FTS_TABLE}"),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if distinct_pages != disk_count {
        rebuild_index(conn, root)?;
        return Ok(true);
    }

    Ok(false)
}

/// Rebuild index from scratch by scanning `wiki/` for `.md` files. Drops + recreates FTS table,
/// inserts every page. Excludes internal files (log.md, index.md) via `collect_wiki_pages`.
/// Thin wrapper; callers needing smaller DB lock bursts should use `collect_page_rows` + `insert_page_rows`.
pub fn rebuild_index(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    // Drop existing content and recreate.
    conn.execute_batch(&format!("DELETE FROM {FTS_TABLE};"))?;
    ensure_table(conn)?;

    let wiki_dir = root.join("wiki");
    /* Collect whole-page rows, expand to chunk rows for FTS insertion only.
    Manifest helpers need whole-page set; here we only insert into FTS. */
    let rows = collect_page_rows(root)?;
    let chunked_rows = chunk_page_rows(rows);
    insert_page_rows(conn, &chunked_rows, &wiki_dir)?;
    Ok(chunked_rows.len())
}

/// One wiki page parsed from disk, ready for FTS5 insert. Built by `collect_page_rows`
/// (filesystem-only, no DB) so drift check can do file I/O without DB lock.
#[derive(Debug, Clone)]
pub struct PageRow {
    /// Absolute path to the `.md` file on disk.
    pub abs_path: PathBuf,
    /// Relative path from wiki-root (stable across moves; manifest PK).
    pub rel_path: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub page_type: String,
    pub source_articles: String,
    // ── Chunk metadata (T1.2). `None` for legacy whole-page rows. ──
    /// 0-based chunk ordinal within the page. `None` = whole-page row (short pages, concept/author hubs).
    pub chunk_index: Option<i32>,
    /// Section label, e.g. `"Methods"`. `None` for unstructured text.
    pub section: Option<String>,
    /// Slug of the page this chunk belongs to (== slug for whole-page rows).
    pub parent_slug: Option<String>,
}

/// Read every wiki page from disk into whole-page `Vec<PageRow>` (one per file).
/// **Filesystem-only - no DB.** Drift check does all file I/O lock-free, then takes
/// DB lock only for `insert_page_rows` + manifest. Returns whole-page rows
/// (`chunk_index = None`); manifest helpers need one-row-per-file. Call
/// `chunk_page_rows` before FTS insertion only.
pub fn collect_page_rows(root: &Path) -> Result<Vec<PageRow>, AppError> {
    let pages = collect_wiki_pages(root)?;
    let mut rows = Vec::with_capacity(pages.len());
    for path in pages {
        let (fm, body) = frontmatter::read_file(&path)?;
        let rel_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
        rows.push(PageRow {
            abs_path: path,
            rel_path,
            slug: fm.get("slug").unwrap_or("").to_string(),
            title: fm.get("title").unwrap_or("").to_string(),
            summary: fm.get("summary").unwrap_or("").to_string(),
            body,
            page_type: fm.get("type").unwrap_or("").to_string(),
            source_articles: fm.get("source_articles").unwrap_or("").to_string(),
            chunk_index: None,
            section: None,
            parent_slug: None,
        });
    }
    Ok(rows)
}

/// Expand whole-page rows into chunk rows for FTS5 insertion (T1.2). Long pages produce
/// multiple chunk rows sharing `parent_slug`; short pages produce single row (`chunk_index = None`).
/// Apply ONLY before `insert_page_rows`, never before manifest helpers (which need one-row-per-file).
#[must_use]
pub fn chunk_page_rows(rows: Vec<PageRow>) -> Vec<PageRow> {
    let mut out = Vec::new();
    for row in rows {
        /* Use table-aware composer so GFM tables are detected as SectionKind::Table
        (emitted atomically by chunk_sections) instead of line-split as generic Text. */
        let sections = crate::utils::sections::extract_sections_with_tables(&row.body);
        let chunks = crate::utils::chunking::chunk_sections(
            &sections,
            crate::utils::chunking::DEFAULT_CHUNK_WORDS,
        );

        if chunks.len() <= 1 {
            /* Short page: keep as whole-page row. Strip TABLE:N placeholder comments
            from body so markers don't pollute the FTS index. */
            let mut short = row;
            short.body = strip_table_placeholders(&short.body);
            out.push(short);
        } else {
            /* Long page: emit one chunk row per chunk, sharing parent_slug so
            build_context can dedupe and self-heal distinct-count stays correct.
            Strip placeholder comments from each chunk body. */
            let parent_slug = row.slug.clone();
            for chunk in &chunks {
                out.push(PageRow {
                    body: strip_table_placeholders(&chunk.text),
                    chunk_index: Some(chunk.chunk_index as i32),
                    section: chunk.section.clone(),
                    parent_slug: Some(parent_slug.clone()),
                    ..row.clone()
                });
            }
        }
    }
    out
}

/// Insert pre-collected page rows into FTS5. DB-only (no file I/O). Pairs with
/// `collect_page_rows` so callers split lock-free filesystem work from DB work.
/// Caller must `DELETE FROM {FTS_TABLE}` first.
pub fn insert_page_rows(conn: &Connection, rows: &[PageRow], root: &Path) -> Result<(), AppError> {
    for row in rows {
        /* Store relative path from wiki dir for compactness, matching original
        index_single_file behavior for back-compat with file_path column. */
        let file_path = row
            .abs_path
            .strip_prefix(root.parent().unwrap_or(Path::new("")))
            .unwrap_or(&row.abs_path)
            .to_string_lossy()
            .to_string();
        conn.execute(
            &format!(
                "INSERT INTO {FTS_TABLE} (slug, title, summary, body, page_type, source_articles, file_path, chunk_index, section, parent_slug)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);"
            ),
            rusqlite::params![
                row.slug,
                row.title,
                row.summary,
                row.body,
                row.page_type,
                row.source_articles,
                file_path,
                row.chunk_index,
                row.section,
                row.parent_slug,
            ],
        )?;
    }
    Ok(())
}

// ─── Two-tier drift detection (manifest + directory fingerprint) ───────────
// wiki_check_for_updates detects external edits to wiki/**.md without re-ingesting.
// Tier 1 (cheap): stat-only dir fingerprint (SHA-256 over sorted (rel_path, mtime, size)).
// Tier 2 (expensive): per-file content hash via wiki_index_manifest when tier-1 drifts.

/// Compute tier-1 directory fingerprint: SHA-256 over sorted (rel_path, mtime, size) tuples.
/// **Stat-only - no file reads.** Returns `None` when no pages (caller clears stored hash).
#[must_use]
pub fn compute_directory_fingerprint(pages: &[PageRow]) -> Option<String> {
    if pages.is_empty() {
        return None;
    }
    // Sort by rel_path for a stable, order-independent digest.
    let mut entries: Vec<(&str, std::time::SystemTime, u64)> = pages
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(&p.abs_path).ok()?;
            let mtime = meta.modified().ok()?;
            let size = meta.len();
            Some((p.rel_path.as_str(), mtime, size))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut hasher = Sha256::new();
    for (rel, mtime, size) in &entries {
        hasher.update(rel.as_bytes());
        // Durations since UNIX epoch; fall back to 0 if mtime before epoch
        // (shouldn't happen for real files but keeps the hash total).
        let dur = mtime.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        hasher.update(dur.as_secs().to_string().as_bytes());
        hasher.update(dur.subsec_nanos().to_string().as_bytes());
        hasher.update(size.to_string().as_bytes());
    }
    Some(hex_encode(&hasher.finalize()))
}

/// SHA-256 of a file's bytes. Used for the tier-2 per-file content hash.
fn hash_file_contents(path: &Path) -> Result<String, AppError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex_encode(&hasher.finalize()))
}

/// Compute tier-2 per-file content hashes: (rel_path, sha256) for every page.
/// **Reads file contents** - only called after tier-1 detects drift.
pub fn compute_file_hashes(pages: &[PageRow]) -> Result<Vec<(String, String)>, AppError> {
    let mut out = Vec::with_capacity(pages.len());
    for p in pages {
        out.push((p.rel_path.clone(), hash_file_contents(&p.abs_path)?));
    }
    Ok(out)
}

/// Read `wiki_index_manifest` into `{rel_path → hash}` map. Empty (not error) if table missing.
pub fn read_manifest(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let mut map = std::collections::HashMap::new();
    let mut stmt = match conn.prepare("SELECT file_path, content_hash FROM wiki_index_manifest") {
        Ok(s) => s,
        Err(_) => return Ok(map), // table missing - treat as empty.
    };
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    for row in rows {
        let (path, hash) = row?;
        map.insert(path, hash);
    }
    Ok(map)
}

/// Atomically replace the `wiki_index_manifest` table contents with the given
/// `(rel_path, hash)` rows. `DELETE` + batch `INSERT` in one call.
pub fn write_manifest(conn: &Connection, rows: &[(String, String)]) -> Result<(), AppError> {
    conn.execute_batch("DELETE FROM wiki_index_manifest;")?;
    let mut stmt =
        conn.prepare("INSERT INTO wiki_index_manifest (file_path, content_hash) VALUES (?1, ?2);")?;
    for (path, hash) in rows {
        stmt.execute(rusqlite::params![path, hash])?;
    }
    Ok(())
}

/// Whether FTS index needs rebuild given disk hashes vs stored manifest.
/// `true` when content changed or path set changed. `false` when identical (e.g. only `touch`).
#[must_use]
pub fn manifest_drifted(
    stored: &std::collections::HashMap<String, String>,
    disk: &[(String, String)],
) -> bool {
    if stored.len() != disk.len() {
        return true;
    }
    disk.iter().any(|(path, hash)| stored.get(path).map(String::as_str) != Some(hash.as_str()))
}

/// Read stored tier-1 directory fingerprint from `app_settings`. `None` = no baseline yet.
#[must_use]
pub fn get_dir_hash(conn: &Connection) -> Option<String> {
    app_settings_repo::get_setting(conn, WIKI_DIR_HASH_KEY).ok().flatten()
}

/// Write tier-1 directory fingerprint to `app_settings`. Non-fatal: errors logged to stderr.
pub fn set_dir_hash(conn: &Connection, hash: Option<&str>) {
    if let Err(e) = app_settings_repo::set_setting(conn, WIKI_DIR_HASH_KEY, hash) {
        eprintln!("[wiki] warning: failed to persist wiki_dir_hash: {e}");
    }
}

/// Full rebuild of FTS5 + manifest + dir hash. Call from every internal mutation path
/// (`wiki_update_page`, `wiki_delete_page`, `finalize_ingest`) so the on-demand drift
/// check doesn't false-positive after an internal edit.
pub fn rebuild_index_with_manifest(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    /* Filesystem: collect whole-page rows + compute per-file hashes + dir hash.
    Manifest helpers run on whole-page set (one row per file) so
    wiki_index_manifest PRIMARY KEY (file_path) is not violated by
    duplicate chunk rows sharing the same file_path. */
    let rows = collect_page_rows(root)?;
    let file_hashes = compute_file_hashes(&rows)?;
    let dir_hash = compute_directory_fingerprint(&rows);

    /* Expand to chunk rows for FTS insertion only (not for manifest). */
    let chunked_rows = chunk_page_rows(rows);

    /* DB: wipe + rebuild FTS5, rewrite manifest, update dir hash. */
    conn.execute_batch(&format!("DELETE FROM {FTS_TABLE};"))?;
    ensure_table(conn)?;
    let wiki_dir = root.join("wiki");
    insert_page_rows(conn, &chunked_rows, &wiki_dir)?;
    write_manifest(conn, &file_hashes)?;
    set_dir_hash(conn, dir_hash.as_deref());
    Ok(chunked_rows.len())
}

/// Lowercase hex encoding (no external dep). Mirrors `raw_export::hex_encode`.
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Strip `<!-- TABLE:N -->` placeholder comments from text before FTS index insertion.
/// These structural markers would pollute BM25 rows + chat context.
#[must_use]
pub fn strip_table_placeholders(text: &str) -> String {
    use crate::utils::sections::compile_static_regex;
    use std::sync::OnceLock;
    static PLACEHOLDER_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = PLACEHOLDER_RE.get_or_init(|| {
        // Match `<!-- TABLE:N -->` on its own, optionally followed by a newline.
        compile_static_regex(r"(?m)^<!-- TABLE:\d+ -->\s*$\n?")
    });
    re.replace_all(text, "").to_string()
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
    /// Chunk ordinal (0-based) within the page. `None` for whole-page rows.
    pub chunk_index: Option<i32>,
    /// Section label, e.g. `"Methods"`. `None` for unstructured text.
    pub section: Option<String>,
    /// Slug of the page this chunk belongs to (== slug for whole-page rows).
    pub parent_slug: Option<String>,
}

/// BM25-ranked search over FTS5 index. Splits query into tokens, drops stop words,
/// phrase-quotes each, OR-joins. Any page containing any meaningful term is a candidate.
/// Empty query (only stop words/punctuation) → no results, no error.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<WikiPageHit>, AppError> {
    let safe_query = build_match_query(query);
    // Empty query (e.g. only stop words / punctuation) -> no results, no error.
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
    let limit_i = limit.max(1) as i64;

    let mut stmt = conn.prepare(
        &format!(
            "SELECT slug, title, summary, body, page_type, source_articles, file_path, bm25({FTS_TABLE}) AS rank,
                    chunk_index, section, parent_slug
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
            chunk_index: row.get::<_, Option<i32>>(8)?,
            section: row.get::<_, Option<String>>(9)?,
            parent_slug: row.get::<_, Option<String>>(10)?,
        })
    })?;

    let mut hits = Vec::new();
    for row in rows {
        hits.push(row?);
    }
    Ok(hits)
}

/// English stop words dropped from the MATCH query and from screening chunk
/// scoring. The canonical list lives in `utils::text_tokens` so the Wiki FTS5
/// BM25 index and the Tier 3 screening TF scorer share one source of truth.
/// Re-exported here so existing references (`STOP_WORDS.contains(...)`) keep
/// working without touching each call site.
pub use crate::utils::text_tokens::STOP_WORDS;

/// Build a safe FTS5 MATCH expression: tokenize on non-alphanumeric, drop stop words,
/// phrase-quote each token (literal, never operator), OR-join. Falls back to all tokens
/// if stop-word stripping removes everything. Empty string on no alphanumeric input.
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

    /* Prefer non-stop-word tokens; fall back to all tokens if that leaves nothing. */
    let stop = |t: &str| STOP_WORDS.contains(&t);
    let meaningful: Vec<&String> = raw_tokens.iter().filter(|t| !stop(t.as_str())).collect();
    let tokens: Vec<&String> =
        if meaningful.is_empty() { raw_tokens.iter().collect() } else { meaningful };

    tokens
        .iter()
        .map(|t| {
            /* Phrase-quote each token, doubling embedded " so FTS5 reads it literally. */
            let escaped = t.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Top-level wiki `.md` infrastructure files excluded from page list + FTS index.
/// Only direct children of `wiki/` filtered; subdirectory files of same name are listed.
const INTERNAL_WIKI_FILES: &[&str] = &["log", "index"];

/// Collect all wiki `.md` file paths for listing in UI. Excludes top-level internal
/// infrastructure files (log.md, index.md). Sorted by filename stem.
pub fn collect_wiki_pages(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let wiki_dir = root.join("wiki");
    if !wiki_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_md_recursive(&wiki_dir, &mut out)?;
    /* Exclude top-level internal files (log.md, index.md) - they lack
    frontmatter slug/type and would surface as raw "log"/"index" entries.
    Only direct children of wiki/ filtered; wiki/concepts/log.md still listed. */
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

/* All unit tests live in `src-tauri/tests/wiki_fts_test.rs` per
`docs/CLAUDE.md` §Testing: move large inline tests to standalone integration tests. */
