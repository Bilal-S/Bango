//! FTS5 full-text search over wiki pages.
//!
//! Creates and maintains a `wiki_pages_fts` virtual table (FTS5, bundled in
//! `rusqlite`'s `bundled` feature) for BM25-ranked retrieval. Used by:
//! - Phase 5 `wiki_chat` (token-budgeted RAG retrieval).
//! - Phase 3 `wiki_ingest` (the index is rebuilt after pages are generated).
//!
//! The index mirrors the frontmatter + body of every `.md` page under
//! `wiki/` (not `raw/` - raw sources are ingested into wiki pages first).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::app_settings_repo;
use crate::error::AppError;
use crate::wiki::frontmatter;

/// The FTS5 virtual table name.
pub const FTS_TABLE: &str = "wiki_pages_fts";

/// The `app_settings` key holding the tier-1 directory fingerprint (stat-based
/// hash over all wiki pages). Absent means "no baseline" → first check
/// populates it.
pub const WIKI_DIR_HASH_KEY: &str = "wiki_dir_hash";

/// Ensure the FTS5 table exists with the chunk-aware schema. Idempotent.
///
/// The schema includes three `UNINDEXED` metadata columns added in migration
/// v002 (T1.2) so BM25 retrieval can return passage-level chunks with section
/// provenance:
/// - `chunk_index` - 0-based chunk ordinal within the page (NULL = legacy
///   whole-page row).
/// - `section` - `"Methods"` / `"Results"` / NULL.
/// - `parent_slug` - slug of the page this chunk belongs to (== slug for
///   whole-page rows).
///
/// Migration v002 `DROP`s the old `wiki_pages_fts` so this `CREATE` runs on the
/// first read after upgrade. On a fresh DB it creates the new shape directly.
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

    // Case 2: index non-empty but the number of distinct pages differs from
    // disk count. After T1.2 the index is row-per-chunk (N rows per long page),
    // so a raw row-count comparison would false-positive on every call. We
    // compare `COUNT(DISTINCT COALESCE(parent_slug, slug))` against the disk
    // page count instead, so chunk rows do not desync the self-heal.
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

/// Rebuild the index from scratch by scanning `wiki/` for `.md` files.
/// Drops and recreates the FTS table, then inserts every page.
///
/// Uses `collect_wiki_pages` (which excludes top-level internal files like
/// `log.md` / `index.md`) so the FTS search index matches the page list shown
/// in the UI - internal infrastructure never surfaces in chat or search.
///
/// This is the thin wrapper that does both the file reads (filesystem) and the
/// FTS5 inserts (DB) in one call. Callers that need to keep the DB lock window
/// small (e.g. the on-demand drift check) should call `collect_page_rows`
/// (no DB) followed by `insert_page_rows` (DB only) directly, then
/// `write_manifest` + `set_dir_hash`.
pub fn rebuild_index(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    // Drop existing content and recreate.
    conn.execute_batch(&format!("DELETE FROM {FTS_TABLE};"))?;
    ensure_table(conn)?;

    let wiki_dir = root.join("wiki");
    // Collect whole-page rows, then expand to chunk rows for insertion only.
    // The manifest helpers (in `rebuild_index_with_manifest`) need the
    // whole-page set; here we only insert into FTS so chunking is safe.
    let rows = collect_page_rows(root)?;
    let chunked_rows = chunk_page_rows(rows);
    insert_page_rows(conn, &chunked_rows, &wiki_dir)?;
    Ok(chunked_rows.len())
}

/// One wiki page parsed from disk, ready to insert into FTS5. Built by
/// `collect_page_rows` (filesystem-only, no DB) so the drift check can do all
/// file I/O without holding the DB lock.
#[derive(Debug, Clone)]
pub struct PageRow {
    /// Absolute path to the `.md` file on disk.
    pub abs_path: PathBuf,
    /// Relative path from the wiki-root (stable across wiki-root moves;
    /// used as the manifest PK).
    pub rel_path: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub page_type: String,
    pub source_articles: String,
    // ── Chunk metadata (T1.2). `None` for legacy whole-page rows. ──
    /// 0-based chunk ordinal within the page. `None` = whole-page row (short
    /// pages, concept/author hubs).
    pub chunk_index: Option<i32>,
    /// Section label, e.g. `"Methods"`. `None` for unstructured text.
    pub section: Option<String>,
    /// Slug of the page this chunk belongs to (== slug for whole-page rows).
    pub parent_slug: Option<String>,
}

/// Read every wiki page from disk into a `Vec<PageRow>` (one row per file).
///
/// **Filesystem-only - does not touch the DB.** This lets the on-demand drift
/// check (`wiki_check_for_updates`) do all file I/O lock-free, then take the
/// DB lock only for the fast SQLite writes (`insert_page_rows` + manifest).
///
/// Returns **whole-page rows** (`chunk_index = None`). This is intentional: the
/// manifest helpers (`compute_file_hashes`, `compute_directory_fingerprint`)
/// require one row per file so the `wiki_index_manifest` PRIMARY KEY
/// (`file_path`) is not violated by duplicate chunk rows. Call
/// `chunk_page_rows` to expand whole-page rows into chunk rows immediately
/// before FTS insertion only (see `rebuild_index` /
/// `rebuild_index_with_manifest`).
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

/// Expand whole-page rows into chunk rows for FTS5 insertion (T1.2).
///
/// Each whole-page `PageRow` is classified into sections and chunked; long
/// pages produce multiple chunk rows (all sharing `parent_slug`), short pages
/// produce a single row with `chunk_index = None`. This keeps the manifest at
/// one-row-per-file (correct for the `wiki_index_manifest` PRIMARY KEY) while
/// the FTS index gets passage-level granularity for BM25 retrieval + chat
/// section labels.
///
/// **Apply this ONLY before `insert_page_rows`**, never before
/// `compute_file_hashes` / `compute_directory_fingerprint` (those need the
/// original one-row-per-file set to avoid PRIMARY KEY violations and
/// redundant drift checks).
#[must_use]
pub fn chunk_page_rows(rows: Vec<PageRow>) -> Vec<PageRow> {
    let mut out = Vec::new();
    for row in rows {
        // Use the table-aware composer so GFM tables are detected as
        // `SectionKind::Table` (emitted atomically by `chunk_sections`)
        // rather than being line-split as generic `Text`. The previous call
        // to `classify_sections` missed tables entirely, defeating the
        // atomic Table/Figure arm and chopping tables across chunks.
        let sections = crate::utils::sections::extract_sections_with_tables(&row.body);
        let chunks = crate::utils::chunking::chunk_sections(
            &sections,
            crate::utils::chunking::DEFAULT_CHUNK_WORDS,
        );

        if chunks.len() <= 1 {
            // Short page (or no headings): keep as a whole-page row. Strip
            // any `<!-- TABLE:N -->` placeholder comments left by
            // `detect_markdown_tables` so they don't pollute the FTS index.
            let mut short = row;
            short.body = strip_table_placeholders(&short.body);
            out.push(short);
        } else {
            // Long page: emit one chunk row per chunk, sharing parent_slug so
            // `build_context` can dedupe and the self-heal distinct-count
            // stays correct. Strip placeholder comments from each chunk body.
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

/// Insert a batch of pre-collected page rows into the FTS5 table.
///
/// **DB-only - does no file I/O.** Pairs with `collect_page_rows` so callers
/// can split the lock-free filesystem work from the DB work. The caller is
/// responsible for `DELETE FROM {FTS_TABLE}` first (see `rebuild_index` /
/// `rebuild_index_with_manifest`).
pub fn insert_page_rows(conn: &Connection, rows: &[PageRow], root: &Path) -> Result<(), AppError> {
    for row in rows {
        // Store a relative path from the wiki dir for compactness (matches
        // the original `index_single_file` behavior for back-compat with the
        // `file_path` column the frontend already reads).
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
//
// `wiki_check_for_updates` uses these to detect when external programs edit
// `wiki/**/*.md` files without re-ingesting. Two tiers keep the common case
// cheap:
//
// - **Tier 1 (directory fingerprint):** one SHA-256 over the sorted set of
//   `(rel_path, mtime_sec, mtime_nsec, size)` tuples for every page. Stored
//   in `app_settings` under `WIKI_DIR_HASH_KEY`. Equal -> nothing changed,
//   return immediately (one stat walk, zero file reads).
// - **Tier 2 (per-file content hashes):** the `wiki_index_manifest` table
//   stores one SHA-256 per file. When tier-1 drifts, compare per-file hashes.
//   If content is identical (e.g. `touch`) -> update only the dir hash. If
//   any file's content hash differs (or the path set changed) -> rebuild
//   FTS5 + rewrite the manifest.

/// Compute the tier-1 directory fingerprint: SHA-256 over the sorted
/// `(rel_path, mtime_sec, mtime_nsec, size)` tuples for every page.
///
/// **Stat-only - does not read file contents.** This is the cheap fast path
/// that lets `wiki_check_for_updates` skip tier-2 entirely when nothing
/// changed on disk. The sort by `rel_path` makes the hash stable regardless
/// of filesystem readdir order.
///
/// Returns `None` when there are no pages (so the caller can clear any stale
/// stored hash and skip the check).
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
        // Durations since UNIX epoch; falls back to 0 if the mtime is before
        // the epoch (shouldn't happen for real files, but keeps the hash total).
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

/// Compute the tier-2 per-file content hash list: `(rel_path, sha256)` for
/// every page. **Reads file contents** (the slower path); only called when the
/// tier-1 directory fingerprint has already detected drift.
pub fn compute_file_hashes(pages: &[PageRow]) -> Result<Vec<(String, String)>, AppError> {
    let mut out = Vec::with_capacity(pages.len());
    for p in pages {
        out.push((p.rel_path.clone(), hash_file_contents(&p.abs_path)?));
    }
    Ok(out)
}

/// Read the entire `wiki_index_manifest` table into a `{ rel_path -> hash }`
/// map. Empty (not error) if the table is missing - `ensure_table` handles
/// creation lazily.
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

/// Decide whether the FTS5 index needs a rebuild given the on-disk per-file
/// hashes vs the stored manifest.
///
/// - Returns `true` if any file's content hash changed OR the path set
///   changed (file added/removed).
/// - Returns `false` if every path has the same content hash AND the path
///   sets are identical (e.g. only `touch` changed mtimes).
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

/// Read the stored tier-1 directory fingerprint from `app_settings`.
/// `None` means "no baseline yet" (first run, or the setting was cleared by a
/// schema rebuild/reset).
#[must_use]
pub fn get_dir_hash(conn: &Connection) -> Option<String> {
    app_settings_repo::get_setting(conn, WIKI_DIR_HASH_KEY).ok().flatten()
}

/// Write the tier-1 directory fingerprint to `app_settings`. Non-fatal: errors
/// are logged to stderr but do not fail the caller (the next check re-derives
/// it from disk).
pub fn set_dir_hash(conn: &Connection, hash: Option<&str>) {
    if let Err(e) = app_settings_repo::set_setting(conn, WIKI_DIR_HASH_KEY, hash) {
        eprintln!("[wiki] warning: failed to persist wiki_dir_hash: {e}");
    }
}

/// Full rebuild of FTS5 + manifest + dir hash in one shot.
///
/// Use this from every internal mutation path (`wiki_update_page`,
/// `wiki_delete_page`, `finalize_ingest`) so the manifest stays in sync with
/// the index. Without it, the on-demand check would false-positive a drift on
/// the next run after an internal edit.
///
/// Does both the file reads and the DB writes; safe to call from synchronous
/// command handlers (the existing mutation paths already do this).
pub fn rebuild_index_with_manifest(conn: &Connection, root: &Path) -> Result<usize, AppError> {
    // Filesystem: collect whole-page rows + compute per-file hashes + dir hash.
    // Manifest helpers MUST run on the whole-page set (one row per file) so the
    // `wiki_index_manifest` PRIMARY KEY (`file_path`) is not violated by
    // duplicate chunk rows sharing the same file_path.
    let rows = collect_page_rows(root)?;
    let file_hashes = compute_file_hashes(&rows)?;
    let dir_hash = compute_directory_fingerprint(&rows);

    // Expand to chunk rows for FTS insertion ONLY (not for the manifest).
    let chunked_rows = chunk_page_rows(rows);

    // DB: wipe + rebuild FTS5, rewrite manifest, update dir hash.
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

/// Strip `<!-- TABLE:N -->` placeholder comments from text before it enters
/// the FTS index.
///
/// `detect_markdown_tables` replaces GFM table blocks with these placeholders
/// in the linear text, then appends the actual table sections at the end. The
/// placeholders are structural markers, not content; leaving them in the FTS
/// body pollutes BM25 rows + chat context with markup that's useless for
/// retrieval. This strips them (and the surrounding blank line) so only real
/// prose + the appended table body are indexed.
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

/// BM25-ranked search over the FTS5 index. Returns up to `limit` hits.
///
/// The user query is converted to a safe FTS5 MATCH expression by
/// `build_match_query`: tokens are split on whitespace/punctuation, stop words
/// are dropped, and each remaining token is phrase-quoted (so FTS5 treats it
/// as a literal string, never as an operator) and OR-joined. This retrieves
/// any page containing any meaningful term, ranked by BM25 - the standard
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
/// file inside a subdirectory (e.g. `wiki/concepts/log.md`) is still listed -
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
    // Exclude top-level internal infrastructure files (log.md, index.md) -
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

// All unit tests for this module live in `src-tauri/tests/wiki_fts_test.rs` per
// `docs/CLAUDE.md` §Testing: "Avoid large inline unit tests in library source
// files ... move them into standalone integration test files under
// `src-tauri/tests/`."
