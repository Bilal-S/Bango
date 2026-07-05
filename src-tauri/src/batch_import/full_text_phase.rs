//! Phase 1: Scan the `fulltext/` directory and attach PDF/TXT files to
//! articles by DOI match.
//!
//! Files must be named `{normalized_doi}.pdf` or `{normalized_doi}.txt`, where
//! `normalized_doi` is the article's DOI run through
//! [`crate::scraping::citation_chaser::clean_doi_filename`]. This mirrors the
//! Citation Chaser RIS naming convention (`{clean_doi}_references.ris`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::commands::full_text::{attach_full_text_inner, compute_storage_dir};
use crate::db::article_repo::{self, ArticleDoiInfo};
use crate::error::AppError;
use crate::scraping::citation_chaser::clean_doi_filename;

use super::{BatchImportPhaseResult, DoiMatchMap};

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

/// Run Phase 1: attach each importable full-text file to its matched article.
///
/// Calls [`attach_full_text_inner`] per file under the caller's DB lock.
/// Text-extraction failures are handled inside `attach_full_text_inner` (the
/// file attaches with empty text and a general-error audit entry), so they are
/// reported as succeeded here; this `Err` arm only catches hard attach
/// failures (missing file, copy error, DB write error).
/// The caller's `is_cancelled` closure is checked before each file so the
/// runner can abort mid-phase.
///
/// Returns a [`BatchImportPhaseResult`] with counts and the list of
/// newly-attached article IDs (consumed by Phase 3).
pub fn run_full_text_phase<F, P>(
    conn: &rusqlite::Connection,
    is_cancelled: &F,
    on_progress: &mut P,
) -> Result<(BatchImportPhaseResult, Vec<String>), AppError>
where
    F: Fn() -> bool,
    P: FnMut(usize, usize, &str),
{
    let storage_dir = compute_storage_dir(conn)?;
    let articles = article_repo::get_articles_with_doi_info(conn)?;
    let match_map = build_fulltext_match_map(&articles);
    let importable = discover_importable_files(&storage_dir, &match_map);

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

        // The connection mutex is held for the whole phase, so the attach call
        // (text extraction + DB write) runs under the lock. Text-extraction
        // failures are handled inside `attach_full_text_inner`: the file still
        // attaches with empty text and a general-error audit entry, so the
        // article is correctly reported as succeeded here and is not retried
        // next run. This `Err` arm catches only hard attach failures (missing
        // file, copy error, DB write error) and surfaces them via the Audit
        // Timeline in addition to the transient progress UI.
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        on_progress(processed, total, &format!("Phase 1 - Full Text - found {total} files - attempting to attach {} of {total}: {filename}", processed + 1));

        processed += 1;
        match attach_full_text_inner(conn, article_id, path, &storage_dir) {
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
                let _ = crate::db::audit_repo::log_error(conn, &audit);
            }
        }
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
}
