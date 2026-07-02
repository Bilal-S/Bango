//! Phase 2: Scan the `ris/` directory and import Citation Chaser RIS files
//! (and BibTeX files) into the articles that match by DOI.
//!
//! Recognized naming conventions (all keyed on `clean_doi_filename(doi)`):
//! - `{cleaned_doi}_references.ris` - backward references (skip if
//!   `has_reference_details`)
//! - `{cleaned_doi}_citations.ris` - forward citations (skip if
//!   `has_citation_details`)
//! - `{cleaned_doi}.ris` / `{cleaned_doi}.bib` - generic reference file (skip
//!   if `has_reference_details`)
//!
//! Files are parsed via [`crate::commands::references::import_references_inner`]
//! which auto-detects RIS vs BibTeX by extension.

use std::path::{Path, PathBuf};

use crate::commands::references::import_references_inner;
use crate::db::article_repo;
use crate::error::AppError;

use super::{full_text_phase::build_fulltext_match_map, BatchImportPhaseResult, DoiMatchMap};

/// Recognized RIS-like extensions (BibTeX `.bib` is also accepted by
/// `import_references_inner`).
const CITATION_EXTENSIONS: &[&str] = &["ris", "bib"];

/// A discovered citation/reference file pending import, paired with the
/// matched article ID and the reference-type label (`"reference"` or
/// `"citation"`).
struct PendingImport {
    path: PathBuf,
    article_id: String,
    ref_type: &'static str,
}

/// Resolve the `ris/` directory under the storage root, creating it if needed.
fn resolve_ris_dir(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let root = crate::db::app_settings_repo::get_storage_root(conn)?;
    let ris = PathBuf::from(root).join("ris");
    std::fs::create_dir_all(&ris).map_err(|e| {
        AppError::Import(format!("Failed to create ris storage directory '{}': {e}", ris.display()))
    })?;
    Ok(ris)
}

/// Discover all importable citation/reference files in `ris/`.
///
/// For each article DOI, checks for the three naming patterns and skips files
/// whose target article already has the corresponding detail flag set.
fn discover_importable_files(ris_dir: &Path, match_map: &DoiMatchMap) -> Vec<PendingImport> {
    let mut importable: Vec<PendingImport> = Vec::new();

    // Build a set of (article_id, ref_type) already present so we can skip.
    // Iterate over the match map entries so we only look for files for DOIs we
    // actually have articles for.
    for (cleaned_doi, article) in &match_map.0 {
        // _references.ris - skip if article already has reference details
        if !article.has_reference_details {
            let refs_path = ris_dir.join(format!("{cleaned_doi}_references.ris"));
            if refs_path.exists() {
                importable.push(PendingImport {
                    path: refs_path,
                    article_id: article.id.clone(),
                    ref_type: "reference",
                });
            } else {
                // Generic fallback: {cleaned_doi}.ris or .bib
                for ext in CITATION_EXTENSIONS {
                    let generic = ris_dir.join(format!("{cleaned_doi}.{ext}"));
                    if generic.exists() {
                        importable.push(PendingImport {
                            path: generic,
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
            let cits_path = ris_dir.join(format!("{cleaned_doi}_citations.ris"));
            if cits_path.exists() {
                importable.push(PendingImport {
                    path: cits_path,
                    article_id: article.id.clone(),
                    ref_type: "citation",
                });
            }
        }
    }

    importable
}

/// Run Phase 2: import each discovered citation/reference file.
///
/// Calls [`import_references_inner`] per file. The caller's `is_cancelled`
/// closure is checked before each file so the runner can abort mid-phase.
pub fn run_citations_phase<F, P>(
    conn: &rusqlite::Connection,
    is_cancelled: &F,
    on_progress: &mut P,
) -> Result<BatchImportPhaseResult, AppError>
where
    F: Fn() -> bool,
    P: FnMut(usize, usize, &str),
{
    let ris_dir = resolve_ris_dir(conn)?;
    let articles = article_repo::get_articles_with_doi_info(conn)?;
    let match_map = build_fulltext_match_map(&articles);
    let importable = discover_importable_files(&ris_dir, &match_map);

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
        match import_references_inner(conn, &item.article_id, &path_str, item.ref_type) {
            Ok(result) => {
                // Consider it succeeded if at least one link was created.
                if result.links_created > 0 || result.papers_created > 0 {
                    succeeded += 1;
                }
                // Surface non-fatal parse/insert errors from the inner function.
                for e in &result.errors {
                    errors.push(format!(
                        "{}: {e}",
                        item.path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                    ));
                }
            }
            Err(e) => {
                failed += 1;
                errors.push(format!(
                    "Failed to import '{}': {e}",
                    item.path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ));
            }
        }
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
        let map = build_fulltext_match_map(&articles);
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
        let map = build_fulltext_match_map(&articles);
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
        let map = build_fulltext_match_map(&articles);
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
        let map = build_fulltext_match_map(&articles);
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
        let map = build_fulltext_match_map(&articles);
        let importable = discover_importable_files(tmp.path(), &map);
        assert_eq!(importable.len(), 1);
    }
}
