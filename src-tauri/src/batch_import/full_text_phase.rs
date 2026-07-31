//! Phase 1: Scan the `fulltext/` directory and attach PDF/TXT files to
//! articles by DOI match.
//!
//! Files must be named `{normalized_doi}.pdf` or `{normalized_doi}.txt`, where
//! `normalized_doi` is the article's DOI run through
//! [`crate::scraping::citation_chaser::clean_doi_filename`]. This mirrors the
//! Citation Chaser RIS naming convention (`{clean_doi}_references.ris`).
//!
//! # Lock scope (Concern 3)
//!
//! The DB mutex is held only for:
//! 1. The brief initial discovery (build the match map + resolve the storage
//!    dir) - one short burst at the start of the phase.
//! 2. The short per-article DB-write burst via the split pipeline
//!    `attach_full_text_split` (`update_full_text` + chunk insert + audit
//!    entries + staleness flags).
//!
//! The CPU-bound PDF parse + text extraction runs on `spawn_blocking` with
//! NO lock held (via [`crate::commands::full_text::extract_full_text_data`]),
//! so other IPC commands stay responsive during a large batch import. This
//! replaces the previous shape that locked the connection ONCE at the top of a
//! `spawn_blocking` and held the guard across every per-article PDF parse, and
//! the intermediate shape that released the lock between articles but still
//! held it across each per-article parse.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::commands::full_text::{attach_full_text_split, compute_storage_dir};
use crate::db::article_repo::{self, ArticleDoiInfo};
use crate::error::AppError;
use crate::scraping::citation_chaser::clean_doi_filename;

use super::{BatchImportPhaseResult, DoiMatchMap};

/// A secondary lookup index built once during discovery: `article_id -> DOI`.
/// The primary match map is keyed by cleaned-DOI (so the discovery scan can
/// match on-disk filename stems directly); this secondary index lets the
/// per-article attach loop recover the article's DOI in O(1) instead of
/// scanning the match map values per article (O(n²) overall). Built alongside
/// `DoiMatchMap` so the two stay consistent.
type IdToDoiMap = HashMap<String, String>;

/// The file extensions recognized as full-text attachments.
const FULLTEXT_EXTENSIONS: &[&str] = &["pdf", "txt"];

/// Build the DOI match map from articles with DOIs. Each article's DOI is
/// normalized through `clean_doi_filename` so it matches the on-disk filename
/// stem. Articles with `has_full_text = true` are included but flagged so the
/// phase 1 scanner skips them (the map serves all 3 phases).
///
/// Pure function (`#[must_use]`): no I/O. Extracted so tests can verify the
/// matching logic against a fixture list of articles.
#[must_use]
pub fn build_fulltext_match_map(articles: &[ArticleDoiInfo]) -> DoiMatchMap {
    let mut map: HashMap<String, ArticleDoiInfo> = HashMap::with_capacity(articles.len());
    for a in articles {
        let key = clean_doi_filename(&a.doi);
        if key.is_empty() {
            continue;
        }
        // Only insert the first article per cleaned-DOI key to avoid ambiguous
        // matches (two articles with DOIs that clean to the same string).
        map.entry(key).or_insert_with(|| a.clone());
    }
    DoiMatchMap(map)
}

/// Build the secondary `article_id -> DOI` lookup index from the same article
/// list as [`build_fulltext_match_map`]. This lets the per-article attach loop
/// recover each article's DOI in O(1) instead of scanning the match map values
/// (which is keyed by cleaned-DOI, not article-id). Pure `#[must_use]`.
#[must_use]
pub fn build_id_to_doi_map(articles: &[ArticleDoiInfo]) -> IdToDoiMap {
    articles
        .iter()
        .filter(|a| !a.doi.trim().is_empty())
        .map(|a| (a.id.clone(), a.doi.clone()))
        .collect()
}

/// Discover all importable full-text files in the `fulltext/` directory.
///
/// Returns `(matched, total_files)` where `matched` is the count of files
/// whose stem matches an article DOI in `match_map` AND the article does not
/// already have full text attached. Pure I/O + lookup; no DB writes.
///
/// Used by the runner to compute the phase total for progress reporting before
/// starting the attach loop.
pub fn discover_importable_files(
    fulltext_dir: &Path,
    match_map: &DoiMatchMap,
) -> Vec<(PathBuf, String)> {
    let mut importable: Vec<(PathBuf, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(fulltext_dir) else {
        return importable;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) else {
            continue;
        };
        if !FULLTEXT_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem = stem.to_string();
        // Match against the DOI map. Skip if article already has full text.
        if let Some(article) = match_map.get(&stem) {
            if !article.has_full_text {
                importable.push((path, article.id.clone()));
            }
        }
    }
    importable
}

/// Discovery payload returned by the brief initial lock burst. The
/// `id_to_doi` index is carried alongside the importable file list so
/// per-article DOI lookups during the attach loop are a pure in-memory O(1)
/// operation (no DB read). The primary `DoiMatchMap` (keyed by cleaned-DOI)
/// is used only during discovery to match on-disk filename stems and is not
/// returned - the attach loop resolves DOIs by article-id via `id_to_doi`.
pub(crate) struct FullTextDiscovery {
    pub storage_dir: PathBuf,
    pub importable: Vec<(PathBuf, String)>,
    pub id_to_doi: IdToDoiMap,
}

/// Brief initial lock burst: resolve the storage dir, build the DOI match map,
/// discover importable files. The lock is released before this function
/// returns, so the per-article attach loop runs WITHOUT holding the mutex.
///
/// Extracted from `run_full_text_phase` so the async wrapper and the
/// synchronous test harness can share the discovery step.
pub(crate) fn discover(conn: &rusqlite::Connection) -> Result<FullTextDiscovery, AppError> {
    let storage_dir = compute_storage_dir(conn)?;
    let articles = article_repo::get_articles_with_doi_info(conn)?;
    let match_map = build_fulltext_match_map(&articles);
    let id_to_doi = build_id_to_doi_map(&articles);
    let importable = discover_importable_files(&storage_dir, &match_map);
    Ok(FullTextDiscovery { storage_dir, importable, id_to_doi })
}

/// Run Phase 1: attach each importable full-text file to its matched article.
///
/// The DB mutex is held only for the brief initial discovery and for the
/// short per-article DB-write burst inside [`attach_full_text_split`]. The
/// CPU-bound PDF parse + text extraction runs on `spawn_blocking` with NO
/// lock held (Concern 3 gap fix), so other IPC commands stay responsive
/// during a large import.
///
/// The caller's `is_cancelled` closure is checked before each file so the
/// runner can abort mid-phase.
///
/// Returns a [`BatchImportPhaseResult`] with counts and the list of
/// newly-attached article IDs (consumed by Phase 3).
pub async fn run_full_text_phase<F, P>(
    conn_mutex: &Mutex<rusqlite::Connection>,
    is_cancelled: &F,
    on_progress: &mut P,
) -> Result<(BatchImportPhaseResult, Vec<String>), AppError>
where
    F: Fn() -> bool,
    P: FnMut(usize, usize, &str),
{
    // Brief initial lock: discover + build the match map, then release.
    let discovery = {
        let conn = crate::db::connection::lock_conn(conn_mutex)?;
        discover(&conn)?
    };
    let FullTextDiscovery { storage_dir, importable, id_to_doi } = discovery;

    let total = importable.len();
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut newly_attached_ids: Vec<String> = Vec::new();

    for (path, article_id) in &importable {
        if is_cancelled() {
            break;
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        on_progress(processed, total, &format!("Phase 1 - Full Text - found {total} files - attempting to attach {} of {total}: {filename}", processed + 1));
        processed += 1;

        // Look up the article's DOI via the secondary `id_to_doi` index so
        // the destination filename uses the DOI-aware naming convention
        // (Concern 2). The index is built once during discovery, so this is a
        // pure in-memory O(1) lookup - no DB round-trip. (The previous shape
        // scanned `match_map.0.values()` per article, which was O(n²)
        // overall because the primary map is keyed by cleaned-DOI, not by
        // article-id.)
        let article_doi = id_to_doi.get(article_id).map(String::as_str);

        // Split attach pipeline (Concern 3 gap fix): the CPU-bound PDF parse
        // + text extraction runs on `spawn_blocking` with NO DB lock held,
        // then a short lock burst handles only the DB writes. This keeps the
        // DbState mutex held for millisecond-scale bursts instead of the
        // 1-5s per-PDF parse, so other IPC commands stay responsive.
        let attach_result =
            attach_full_text_split(conn_mutex, article_id, article_doi, path, &storage_dir).await;

        match attach_result {
            Ok(_) => {
                succeeded += 1;
                newly_attached_ids.push(article_id.clone());
            }
            Err(e) => {
                failed += 1;
                let audit = format!(
                    "Batch import Phase 1 (article {article_id}): Failed to attach '{filename}': {e}"
                );
                errors.push(format!("Failed to attach '{filename}': {e}"));
                // Surface in the Audit Timeline (general error) so failures
                // aren't only visible in the transient progress UI. Non-fatal:
                // ignore audit-write errors so a logging failure can't break
                // the import loop.
                let conn = crate::db::connection::lock_conn(conn_mutex)?;
                let _ = crate::db::audit_repo::log_error(&conn, &audit);
            }
        }

        // Yield between articles so the async runtime can flush progress
        // events and other IPC commands get a turn.
        tokio::task::yield_now().await;
    }

    Ok((BatchImportPhaseResult { total, processed, succeeded, failed, errors }, newly_attached_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(id: &str, doi: &str, has_ft: bool) -> ArticleDoiInfo {
        ArticleDoiInfo {
            id: id.to_string(),
            doi: doi.to_string(),
            has_full_text: has_ft,
            has_reference_details: false,
            has_citation_details: false,
            has_ai_summary: false,
        }
    }

    #[test]
    fn build_match_map_normalizes_dois() {
        let articles =
            vec![art("a1", "10.1016/j.jand.2021.06.013", false), art("a2", "10.1001/foo", false)];
        let map = build_fulltext_match_map(&articles);
        // Slashes are replaced with underscores by clean_doi_filename.
        assert!(map.contains_key("10.1016_j.jand.2021.06.013"));
        assert!(map.contains_key("10.1001_foo"));
    }

    #[test]
    fn build_match_map_first_doi_wins_on_collision() {
        // Two DOIs that clean to the same filename: first article wins.
        let articles = vec![art("a1", "10.1/abc", false), art("a2", "10.1_abc", false)];
        let map = build_fulltext_match_map(&articles);
        // Only one key exists (both clean to "10.1_abc"), and it maps to a1.
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("10.1_abc").unwrap().id, "a1");
    }

    #[test]
    fn build_match_map_skips_empty_dois() {
        let articles = vec![art("a1", "", false)];
        let map = build_fulltext_match_map(&articles);
        assert!(map.0.is_empty());
    }

    #[test]
    fn build_id_to_doi_map_indexes_articles_with_dois() {
        let articles = vec![
            art("a1", "10.1001/foo", false),
            art("a2", "10.1001/bar", false),
            art("a3", "", false), // no DOI -> excluded
        ];
        let id_to_doi = build_id_to_doi_map(&articles);
        assert_eq!(id_to_doi.len(), 2, "only articles with DOIs are indexed");
        assert_eq!(id_to_doi.get("a1").map(String::as_str), Some("10.1001/foo"));
        assert_eq!(id_to_doi.get("a2").map(String::as_str), Some("10.1001/bar"));
        assert!(!id_to_doi.contains_key("a3"), "empty-DOI article is excluded");
    }

    #[test]
    fn build_id_to_doi_map_empty_input() {
        let id_to_doi = build_id_to_doi_map(&[]);
        assert!(id_to_doi.is_empty());
    }
}
