//! Phase 2: Scan `ris/` and import Citation Chaser RIS / BibTeX files into
//! articles matched by DOI. Naming conventions (keyed on
//! [`clean_doi_filename`]):
//! - `{cleaned_doi}_references.ris` (skip if `has_reference_details`)
//! - `{cleaned_doi}_citations.ris` (skip if `has_citation_details`)
//! - `{cleaned_doi}.ris` / `.bib` (generic, skip if `has_reference_details`)
//!
//! Files parsed via [`crate::commands::references::import_references_inner`]
//! (auto-detects RIS vs BibTeX).
//!
//! # Lock scope (Concern 3)
//!
//! DB mutex held only for brief discovery + short per-file import burst.
//! RIS parsing is fast but the short-lock principle keeps other IPC commands
//! responsive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::commands::references::import_references_inner;
use crate::db::article_repo;
use crate::error::AppError;

use super::{full_text_phase::build_fulltext_match_map, BatchImportPhaseResult, DoiMatchMap};

/// Recognized RIS/BibTeX extensions.
const CITATION_EXTENSIONS: &[&str] = &["ris", "bib"];

/// Pending citation/reference file to import.
struct PendingImport {
    path: PathBuf,
    article_id: String,
    ref_type: &'static str,
}

/// Resolve `ris/` under storage root, creating if needed.
fn resolve_ris_dir(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let root = crate::db::app_settings_repo::get_storage_root(conn)?;
    let ris = PathBuf::from(root).join("ris");
    std::fs::create_dir_all(&ris).map_err(|e| {
        AppError::Import(format!("Failed to create ris storage directory '{}': {e}", ris.display()))
    })?;
    Ok(ris)
}

/// Build a one-pass lowercase filename index of `dir`: lowercased file name ->
/// path. When two files differ only in letter case (possible on Linux), the
/// exactly-lowercase-named file wins the slot so resolution is deterministic.
#[must_use]
fn build_lowercase_dir_index(dir: &Path) -> HashMap<String, PathBuf> {
    let mut index: HashMap<String, PathBuf> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return index;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let key = name.to_lowercase();
        let entry_is_lowercase = name == key;
        let incumbent_is_lowercase = index
            .get(&key)
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == key);
        if !index.contains_key(&key) || (entry_is_lowercase && !incumbent_is_lowercase) {
            index.insert(key, path);
        }
    }
    index
}

/// Discover importable citation files in `ris/`. For each article DOI, checks
/// naming patterns (resolved against the lowercase directory index, so legacy
/// mixed-case filenames still match) and skips if target already has the
/// detail flag.
fn discover_importable_files(ris_dir: &Path, match_map: &DoiMatchMap) -> Vec<PendingImport> {
    let mut importable: Vec<PendingImport> = Vec::new();
    let dir_index = build_lowercase_dir_index(ris_dir);

    // Build a set of (article_id, ref_type) already present so we can skip.
    // Iterate over the match map entries so we only look for files for DOIs we
    // actually have articles for. Match-map keys are lowercase (canonical DOI
    // form), so the formatted probes line up with the index keys.
    for (cleaned_doi, article) in &match_map.0 {
        // _references.ris - skip if article already has reference details
        if !article.has_reference_details {
            if let Some(refs_path) = dir_index.get(&format!("{cleaned_doi}_references.ris")) {
                importable.push(PendingImport {
                    path: refs_path.clone(),
                    article_id: article.id.clone(),
                    ref_type: "reference",
                });
            } else {
                // Generic fallback: {cleaned_doi}.ris or .bib
                for ext in CITATION_EXTENSIONS {
                    if let Some(generic) = dir_index.get(&format!("{cleaned_doi}.{ext}")) {
                        importable.push(PendingImport {
                            path: generic.clone(),
                            article_id: article.id.clone(),
                            ref_type: "reference",
                        });
                        break;
                    }
                }
            }
        }

        // _citations.ris - skip if article already has citation details
        if !article.has_citation_details {
            if let Some(cits_path) = dir_index.get(&format!("{cleaned_doi}_citations.ris")) {
                importable.push(PendingImport {
                    path: cits_path.clone(),
                    article_id: article.id.clone(),
                    ref_type: "citation",
                });
            }
        }
    }

    importable
}

/// Discovery payload from the brief initial lock burst.
pub(crate) struct CitationsDiscovery {
    importable: Vec<PendingImport>,
}

/// Brief lock burst: resolve `ris/`, build match map, discover files.
/// Lock released before return; per-file import runs without mutex.
pub(crate) fn discover(conn: &rusqlite::Connection) -> Result<CitationsDiscovery, AppError> {
    let ris_dir = resolve_ris_dir(conn)?;
    let articles = article_repo::get_articles_with_doi_info(conn)?;
    let match_map = build_fulltext_match_map(&articles);
    let importable = discover_importable_files(&ris_dir, &match_map);
    Ok(CitationsDiscovery { importable })
}

/// Run Phase 2: import each discovered citation/reference file with short
/// DB lock bursts. `is_cancelled` checked before each file.
pub async fn run_citations_phase<F, P>(
    conn_mutex: &Mutex<rusqlite::Connection>,
    is_cancelled: &F,
    on_progress: &mut P,
) -> Result<BatchImportPhaseResult, AppError>
where
    F: Fn() -> bool,
    P: FnMut(usize, usize, &str),
{
    // Brief initial lock: discover + build the match map, then release.
    let discovery = {
        let conn = crate::db::connection::lock_conn(conn_mutex)?;
        discover(&conn)?
    };
    let CitationsDiscovery { importable } = discovery;

    let total = importable.len();
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for item in &importable {
        if is_cancelled() {
            break;
        }
        let fname = item.path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        on_progress(
            processed,
            total,
            &format!(
                "Phase 2 - Citations - found {total} files - importing {} of {total}: {fname}",
                processed + 1
            ),
        );

        processed += 1;
        let path_str = item.path.to_string_lossy().to_string();
        // Short lock burst per file: parse + insert. `import_references_inner`
        // is synchronous; the lock is released immediately after.
        let import_outcome = {
            let conn = crate::db::connection::lock_conn(conn_mutex)?;
            import_references_inner(&conn, &item.article_id, &path_str, item.ref_type)
        };

        match import_outcome {
            Ok(result) => {
                // Consider it succeeded if at least one link was created.
                if result.links_created > 0 || result.papers_created > 0 {
                    succeeded += 1;
                }
                // Surface non-fatal parse/insert errors from the inner function.
                for e in &result.errors {
                    errors.push(format!("{fname}: {e}"));
                }
            }
            Err(e) => {
                failed += 1;
                errors.push(format!("Failed to import '{fname}': {e}"));
            }
        }

        // Yield between files so the async runtime can flush progress events
        // and other IPC commands get a turn.
        tokio::task::yield_now().await;
    }

    Ok(BatchImportPhaseResult { total, processed, succeeded, failed, errors })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::article_repo::ArticleDoiInfo;
    use crate::scraping::citation_chaser::clean_doi_filename;
    use std::io::Write;

    fn art(id: &str, doi: &str, has_refs: bool, has_cits: bool) -> ArticleDoiInfo {
        ArticleDoiInfo {
            id: id.to_string(),
            doi: doi.to_string(),
            has_full_text: false,
            has_reference_details: has_refs,
            has_citation_details: has_cits,
            has_ai_summary: false,
        }
    }

    /// Build a `DoiMatchMap` from the given articles (thin wrapper so the test
    /// helper stays consistent with the production builder).
    fn map_of(articles: &[ArticleDoiInfo]) -> DoiMatchMap {
        build_fulltext_match_map(articles)
    }

    #[test]
    fn discover_skips_when_article_already_has_reference_details() {
        let tmp = tempfile::tempdir().unwrap();
        let doi_key = clean_doi_filename("10.1001/foo");
        // Create the references file.
        let refs_path = tmp.path().join(format!("{doi_key}_references.ris"));
        let mut f = std::fs::File::create(&refs_path).unwrap();
        writeln!(f, "TY  - JOUR\nTI  - Test\nER  -").unwrap();

        // Article already has reference details -> should be skipped.
        let articles = vec![art("a1", "10.1001/foo", true, false)];
        let map = map_of(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        assert!(importable.is_empty(), "should skip when has_reference_details is true");
    }

    #[test]
    fn discover_finds_references_file_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let doi_key = clean_doi_filename("10.1001/foo");
        let refs_path = tmp.path().join(format!("{doi_key}_references.ris"));
        let mut f = std::fs::File::create(&refs_path).unwrap();
        writeln!(f, "TY  - JOUR\nTI  - Test\nER  -").unwrap();

        let articles = vec![art("a1", "10.1001/foo", false, false)];
        let map = map_of(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        assert_eq!(importable.len(), 1);
        assert_eq!(importable[0].ref_type, "reference");
        assert_eq!(importable[0].article_id, "a1");
    }

    #[test]
    fn discover_finds_citations_file_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let doi_key = clean_doi_filename("10.1001/foo");
        let cits_path = tmp.path().join(format!("{doi_key}_citations.ris"));
        let mut f = std::fs::File::create(&cits_path).unwrap();
        writeln!(f, "TY  - JOUR\nTI  - Citing\nER  -").unwrap();

        let articles = vec![art("a1", "10.1001/foo", true, false)];
        let map = map_of(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        // references are skipped (has_reference_details=true), but citations found.
        assert_eq!(importable.len(), 1);
        assert_eq!(importable[0].ref_type, "citation");
    }

    #[test]
    fn discover_generic_ris_fallback_when_no_refs_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let doi_key = clean_doi_filename("10.1001/foo");
        let generic_path = tmp.path().join(format!("{doi_key}.ris"));
        let mut f = std::fs::File::create(&generic_path).unwrap();
        writeln!(f, "TY  - JOUR\nTI  - Generic\nER  -").unwrap();

        let articles = vec![art("a1", "10.1001/foo", false, false)];
        let map = map_of(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        assert_eq!(importable.len(), 1);
        assert_eq!(importable[0].ref_type, "reference");
    }

    #[test]
    fn discover_generic_bib_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let doi_key = clean_doi_filename("10.1001/foo");
        let generic_path = tmp.path().join(format!("{doi_key}.bib"));
        let mut f = std::fs::File::create(&generic_path).unwrap();
        writeln!(f, "@article{{foo, title={{Generic}}}}").unwrap();

        let articles = vec![art("a1", "10.1001/foo", false, false)];
        let map = map_of(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        assert_eq!(importable.len(), 1);
    }
}
