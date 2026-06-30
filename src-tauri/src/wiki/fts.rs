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
/// v003 (T1.2) so BM25 retrieval can return passage-level chunks with section
/// provenance:
/// - `chunk_index` - 0-based chunk ordinal within the page (NULL = legacy
///   whole-page row).
/// - `section` - `"Methods"` / `"Results"` / NULL.
/// - `parent_slug` - slug of the page this chunk belongs to (== slug for
///   whole-page rows).
///
/// Migration v003 `DROP`s the old `wiki_pages_fts` so this `CREATE` runs on the
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
/// in the UI — internal infrastructure never surfaces in chat or search.
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
/// **Filesystem-only — does not touch the DB.** This lets the on-demand drift
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
        let sections = crate::utils::sections::classify_sections(&row.body);
        let chunks = crate::utils::chunking::chunk_sections(
            &sections,
            crate::utils::chunking::DEFAULT_CHUNK_WORDS,
        );

        if chunks.len() <= 1 {
            // Short page (or no headings): keep as a whole-page row.
            out.push(row);
        } else {
            // Long page: emit one chunk row per chunk, sharing parent_slug so
            // `build_context` can dedupe and the self-heal distinct-count
            // stays correct.
            let parent_slug = row.slug.clone();
            for chunk in &chunks {
                out.push(PageRow {
                    body: chunk.text.clone(),
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
/// **DB-only — does no file I/O.** Pairs with `collect_page_rows` so callers
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
/// **Stat-only — does not read file contents.** This is the cheap fast path
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
/// map. Empty (not error) if the table is missing — `ensure_table` handles
/// creation lazily.
pub fn read_manifest(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, String>, AppError> {
    let mut map = std::collections::HashMap::new();
    let mut stmt = match conn.prepare("SELECT file_path, content_hash FROM wiki_index_manifest") {
        Ok(s) => s,
        Err(_) => return Ok(map), // table missing — treat as empty.
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

    // ── Two-tier drift detection helpers ───────────────────────────────

    #[test]
    fn compute_directory_fingerprint_none_for_empty() {
        let rows: Vec<PageRow> = Vec::new();
        assert!(compute_directory_fingerprint(&rows).is_none());
    }

    #[test]
    fn compute_directory_fingerprint_changes_on_body_edit() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body v1");

        let rows1 = collect_page_rows(root).unwrap();
        let hash1 = compute_directory_fingerprint(&rows1).unwrap();

        // Edit the body; the stat-based fingerprint changes (mtime + size).
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body v2 with more words");

        let rows2 = collect_page_rows(root).unwrap();
        let hash2 = compute_directory_fingerprint(&rows2).unwrap();
        assert_ne!(hash1, hash2, "fingerprint must change when a file is edited");
    }

    #[test]
    fn compute_directory_fingerprint_is_order_independent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "zebra", "Zebra", "z");
        write_wiki_page(root, "concepts", "alpha", "Alpha", "a");

        let mut rows = collect_page_rows(root).unwrap();
        let hash1 = compute_directory_fingerprint(&rows).unwrap();
        rows.reverse();
        let hash2 = compute_directory_fingerprint(&rows).unwrap();
        assert_eq!(hash1, hash2, "fingerprint must be order-independent (sorted by rel_path)");
    }

    #[test]
    fn manifest_drifted_detects_content_change() {
        let mut stored = std::collections::HashMap::new();
        stored.insert("wiki/concepts/alpha.md".to_string(), "hash-a-v1".to_string());
        stored.insert("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string());

        let disk_same = vec![
            ("wiki/concepts/alpha.md".to_string(), "hash-a-v1".to_string()),
            ("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string()),
        ];
        assert!(!manifest_drifted(&stored, &disk_same), "identical hashes -> no drift");

        let disk_changed = vec![
            ("wiki/concepts/alpha.md".to_string(), "hash-a-v2".to_string()),
            ("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string()),
        ];
        assert!(manifest_drifted(&stored, &disk_changed), "changed hash -> drift");
    }

    #[test]
    fn manifest_drifted_detects_path_set_change() {
        let mut stored = std::collections::HashMap::new();
        stored.insert("wiki/concepts/alpha.md".to_string(), "hash-a".to_string());

        // Added file.
        let disk_added = vec![
            ("wiki/concepts/alpha.md".to_string(), "hash-a".to_string()),
            ("wiki/concepts/beta.md".to_string(), "hash-b".to_string()),
        ];
        assert!(manifest_drifted(&stored, &disk_added), "added file -> drift");

        // Removed file.
        let disk_removed: Vec<(String, String)> = Vec::new();
        assert!(manifest_drifted(&stored, &disk_removed), "removed file -> drift");
    }

    #[test]
    fn rebuild_index_with_manifest_writes_manifest_and_dir_hash() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body");
        write_wiki_page(root, "authors", "jane", "Jane", "jane body");

        let conn = Connection::open_in_memory().unwrap();
        // Note: no run_migrations here; the manifest table is created by the
        // v002 migration. Use an empty in-memory DB + create the table manually
        // so this test doesn't depend on the full migration suite.
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE wiki_index_manifest (
                 file_path TEXT PRIMARY KEY,
                 content_hash TEXT NOT NULL
             );",
        )
        .unwrap();
        ensure_table(&conn).unwrap();

        let count = rebuild_index_with_manifest(&conn, root).unwrap();
        assert_eq!(count, 2);

        // Manifest has 2 rows.
        let manifest = read_manifest(&conn).unwrap();
        assert_eq!(manifest.len(), 2);

        // Dir hash is populated.
        assert!(get_dir_hash(&conn).is_some());

        // A second call is idempotent: same manifest + dir hash, no duplicates.
        let count2 = rebuild_index_with_manifest(&conn, root).unwrap();
        assert_eq!(count2, 2);
        let manifest2 = read_manifest(&conn).unwrap();
        assert_eq!(manifest2.len(), 2);
    }

    // ── T1.2 vertical-slice tests (would have caught the chunk-emission gap) ──

    /// Helper: write a wiki page with a long, section-structured body so
    /// `collect_page_rows` -> `chunk_sections` produces multiple chunk rows.
    fn write_long_sectioned_page(root: &Path, slug: &str, title: &str) {
        // Build a body with a Methods heading + ~1400 words of Methods text
        // (enough to split into >= 3 chunks at target=512).
        let methods_sentence = "This study employed a randomised controlled trial design \
            across multiple sites to evaluate the primary outcome measure with covariate \
            adjustment for baseline characteristics and sensitivity analyses."; // ~28 words
        let methods_body = methods_sentence.repeat(50); // ~1400 words
        let body = format!(
            "# {title}\n\n## Introduction\nIntro text here about the study context.\n\n\
             ## Methods\n{methods_body}\n\n## Results\nThe results showed a significant effect.\n\n\
             ## Discussion\nThese findings have important policy implications."
        );
        write_wiki_page(root, "sources", slug, title, &body);
    }

    /// Test A: a long, section-structured page must produce chunk rows in FTS5
    /// with `chunk_index IS NOT NULL` and `section = 'Methods'`.
    ///
    /// This is the vertical-slice test that would have caught the
    /// chunk-emission gap (`collect_page_rows` not calling `chunk_sections`).
    #[test]
    fn rebuild_index_emits_chunk_rows_for_long_sectioned_pages() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_long_sectioned_page(root, "smith-2023", "Smith 2023");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // Query the raw FTS rows for chunk metadata.
        let mut stmt = conn
            .prepare(
                "SELECT chunk_index, section, parent_slug FROM wiki_pages_fts \
                 WHERE chunk_index IS NOT NULL",
            )
            .unwrap();
        let chunk_rows: Vec<(Option<i32>, Option<String>, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        assert!(
            !chunk_rows.is_empty(),
            "long sectioned page must emit at least one chunk row with chunk_index IS NOT NULL"
        );
        // At least one chunk carries the Methods section label.
        assert!(
            chunk_rows.iter().any(|(_, section, _)| section.as_deref() == Some("Methods")),
            "at least one chunk must have section='Methods': {chunk_rows:?}"
        );
        // All chunk rows share the same parent_slug.
        assert!(
            chunk_rows.iter().all(|(_, _, ps)| ps.as_deref() == Some("smith-2023")),
            "all chunk rows must share parent_slug='smith-2023': {chunk_rows:?}"
        );
    }

    /// Test B: `ensure_index_populated` must NOT rebuild when the FTS has more
    /// rows than disk pages, as long as the *distinct* parent-slug count
    /// matches the disk page count. This catches the self-heal regression
    /// where a raw row-count comparison would false-positive after chunking.
    #[test]
    fn ensure_index_populated_no_rebuild_when_chunk_rows_match_distinct_pages() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // One long page on disk that splits into multiple chunks.
        write_long_sectioned_page(root, "smith-2023", "Smith 2023");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // The index now has multiple chunk rows for one disk page.
        let fts_row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM wiki_pages_fts", [], |r| r.get(0)).unwrap();
        assert!(fts_row_count > 1, "expected multiple chunk rows, got {fts_row_count}");

        // Self-heal should return false (no rebuild): distinct parent-slug
        // count (1) == disk page count (1), even though raw row count > 1.
        let rebuilt = ensure_index_populated(&conn, root).unwrap();
        assert!(!rebuilt, "must not rebuild when chunk rows are consistent with disk pages");
    }

    /// Test C: `search` must return hits with `section == Some("Methods")` for
    /// a long page that was chunked. This catches the gap between the schema
    /// existing and the schema being populated (the new columns exist but
    /// `collect_page_rows` didn't fill them).
    #[test]
    fn search_returns_section_label_for_chunk_rows() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_long_sectioned_page(root, "smith-2023", "Smith 2023");

        let conn = Connection::open_in_memory().unwrap();
        ensure_table(&conn).unwrap();
        rebuild_index(&conn, root).unwrap();

        // Search for a term that appears in the Methods body.
        let hits = search(&conn, "randomised", 10).unwrap();
        assert!(!hits.is_empty(), "should find the Methods chunk");

        // At least one hit must carry the Methods section label and a chunk_index.
        let has_methods_chunk =
            hits.iter().any(|h| h.section.as_deref() == Some("Methods") && h.chunk_index.is_some());
        assert!(
            has_methods_chunk,
            "at least one hit must have section=Some(\"Methods\") + chunk_index set: \
             {:?}",
            hits.iter().map(|h| (&h.slug, &h.section, h.chunk_index)).collect::<Vec<_>>()
        );
    }

    /// Regression: `rebuild_index_with_manifest` must not crash on long pages
    /// that split into multiple chunks. Before the `chunk_page_rows` separation,
    /// `collect_page_rows` inlined chunking and the manifest helpers received
    /// multiple rows with the same `rel_path`, hitting the
    /// `wiki_index_manifest` PRIMARY KEY constraint.
    ///
    /// This test writes a long sectioned page, rebuilds with manifest, and
    /// asserts: (1) no error, (2) the manifest has exactly one row per file
    /// (not per chunk), (3) the FTS index has multiple chunk rows.
    #[test]
    fn rebuild_index_with_manifest_does_not_crash_on_long_chunked_pages() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_long_sectioned_page(root, "smith-2023", "Smith 2023");

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE wiki_index_manifest (
                 file_path TEXT PRIMARY KEY,
                 content_hash TEXT NOT NULL
             );",
        )
        .unwrap();
        ensure_table(&conn).unwrap();

        // Must not return an error (the bug manifested as a UNIQUE constraint
        // violation on wiki_index_manifest.file_path).
        let count = rebuild_index_with_manifest(&conn, root).unwrap();
        assert!(count > 1, "chunked page should produce multiple FTS rows: {count}");

        // Manifest has exactly ONE row (one file on disk), not one-per-chunk.
        let manifest = read_manifest(&conn).unwrap();
        assert_eq!(manifest.len(), 1, "manifest must have one row per file, not per chunk");
        assert!(
            manifest.keys().all(|k| k.contains("smith-2023")),
            "manifest key must be the file path: {manifest:?}"
        );

        // FTS has multiple chunk rows sharing the parent_slug.
        let fts_rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM wiki_pages_fts", [], |r| r.get(0)).unwrap();
        assert!(fts_rows > 1, "FTS should have multiple chunk rows: {fts_rows}");
        let distinct_parents: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT COALESCE(parent_slug, slug)) FROM wiki_pages_fts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(distinct_parents, 1, "all chunk rows share one parent slug");
    }
}
