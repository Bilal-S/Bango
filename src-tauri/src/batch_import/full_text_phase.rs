//! Phase 1: Scan `fulltext/` and attach PDF/TXT files to articles by DOI
//! match. Files named `{normalized_doi}.pdf/.txt` via
//! [`crate::scraping::citation_chaser::clean_doi_filename`].
//!
//! # Lock scope (Concern 3)
//!
//! DB mutex held only for (1) brief discovery and (2) per-article write burst
//! via `attach_full_text_split`. CPU-bound PDF parse runs on `spawn_blocking`
//! via [`crate::commands::full_text::extract_full_text_data`] with no lock
//! held, so other IPC commands stay responsive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::commands::full_text::{attach_full_text_split, compute_storage_dir};
use crate::db::article_repo::{self, ArticleDoiInfo};
use crate::error::AppError;
use crate::scraping::citation_chaser::clean_doi_filename;

use super::{BatchImportPhaseResult, DoiMatchMap};

/// Secondary lookup index built during discovery: `article_id -> DOI`.
/// Enables O(1) DOI recovery per article in the attach loop (avoiding O(n²)
/// scans of the cleaned-DOI-keyed match map).
type IdToDoiMap = HashMap<String, String>;

/// The file extensions recognized as full-text attachments.
const FULLTEXT_EXTENSIONS: &[&str] = &["pdf", "txt"];

/// Build DOI match map from articles with DOIs. Each DOI normalized through
/// `clean_doi_filename`. Has-full-text articles are included but flagged for
/// skipping. Pure `#[must_use]`.
///
/// First article per cleaned-DOI key wins (avoids ambiguous matches).
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

/// Build secondary `article_id -> DOI` index from the same article list.
/// O(1) lookup in the attach loop. Pure `#[must_use]`.
#[must_use]
pub fn build_id_to_doi_map(articles: &[ArticleDoiInfo]) -> IdToDoiMap {
    articles
        .iter()
        .filter(|a| !a.doi.trim().is_empty())
        .map(|a| (a.id.clone(), a.doi.clone()))
        .collect()
}

/// Discover importable full-text files in `fulltext/`. Returns `(path,
/// article_id)` for files whose stem matches an article DOI and whose article
/// doesn't already have full text. Pure I/O + lookup; no DB writes.
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

/// Discovery payload from the brief initial lock burst. `id_to_doi` enables
/// O(1) DOI lookup per article during the attach loop.
pub(crate) struct FullTextDiscovery {
    pub storage_dir: PathBuf,
    pub importable: Vec<(PathBuf, String)>,
    pub id_to_doi: IdToDoiMap,
}

/// Brief lock burst: resolve storage dir, build maps, discover files.
/// Lock released before return; per-article attach runs without mutex.
pub(crate) fn discover(conn: &rusqlite::Connection) -> Result<FullTextDiscovery, AppError> {
    let storage_dir = compute_storage_dir(conn)?;
    let articles = article_repo::get_articles_with_doi_info(conn)?;
    let match_map = build_fulltext_match_map(&articles);
    let id_to_doi = build_id_to_doi_map(&articles);
    let importable = discover_importable_files(&storage_dir, &match_map);
    Ok(FullTextDiscovery { storage_dir, importable, id_to_doi })
}

/// Run Phase 1: attach importable full-text files via split pipeline.
/// DB mutex held for brief discovery + per-article write burst only;
/// CPU-bound PDF parse on `spawn_blocking` with no lock (Concern 3 fix).
/// Returns result + newly-attached article IDs (consumed by Phase 3).
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

        /* DOI lookup via secondary `id_to_doi` index (O(1) in-memory,
        Concern 2). Previous shape scanned `match_map.0.values()` per
        article (O(n²)). */
        let article_doi = id_to_doi.get(article_id).map(String::as_str);

        /* Split attach (Concern 3): CPU-bound PDF parse on `spawn_blocking`
        with no lock; only DB writes take a millisecond-scale burst. */
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
                /* Surface in Audit Timeline. Non-fatal: ignore audit-write
                errors so logging can't break the import loop. */
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
