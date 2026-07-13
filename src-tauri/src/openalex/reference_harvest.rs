//! Reference + Citation harvest for OpenAlex imports.
//!
//! When `openalex_retrieve_references` is enabled, this module fetches both
//! directions of the citation graph for each imported article:
//! - **Outgoing references** (`referenced_works`): the article's bibliography
//! - **Incoming citations** (`cites:` filter): works that cite this article
//!
//! Both are inserted as `reference_papers` + `article_reference_links` with
//! the appropriate `ReferenceType` (`Reference` or `Citation`).

use tauri::State;

use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::reference_repo;
use crate::openalex;
use crate::openalex::mapping;

/// Harvest both outgoing references and incoming citations for a set of
/// imported articles. Each entry in `article_work_pairs` is an
/// `(article_id, openalex_work_id)` pair.
///
/// This function is called from `import_openalex_articles` Phase 2 and runs
/// entirely on the tokio runtime. DB locks are held only for millisecond-scale
/// SQLite writes (insert reference papers + create links + audit entries).
///
/// Non-fatal: per-article errors are logged to the audit trail via
/// `audit_repo::log_error_best_effort` and do not block the import.
pub async fn harvest_references_and_citations(
    article_work_pairs: &[(String, String)],
    mailto: &str,
    api_key: Option<&str>,
    db_state: &State<'_, DbState>,
) {
    if article_work_pairs.is_empty() {
        return;
    }

    // Step 1: Fetch full work data for all imported articles to get their
    // `referenced_works` arrays. The search select excludes this field to
    // keep search payloads small, so we re-fetch here.
    let openalex_ids: Vec<String> = article_work_pairs.iter().map(|(_, wid)| wid.clone()).collect();

    let fetched_works =
        match openalex::client::fetch_works_by_ids(&openalex_ids, mailto, api_key).await {
            Ok(works) => works,
            Err(e) => {
                eprintln!("[openalex] reference harvest initial fetch failed: {e}");
                return;
            }
        };

    // Build a lookup: openalex_id -> referenced_works
    let work_refs_map: std::collections::HashMap<&String, &Vec<String>> =
        fetched_works.iter().map(|w| (&w.id, &w.referenced_works)).collect();

    for (article_id, work_id) in article_work_pairs {
        // --- Outgoing references (the article's bibliography) ---
        harvest_outgoing_references(article_id, work_id, &work_refs_map, mailto, api_key, db_state)
            .await;

        // --- Incoming citations (works that cite this article) ---
        harvest_incoming_citations(article_id, work_id, mailto, api_key, db_state).await;
    }
}

/// Fetch and insert the article's outgoing references (its bibliography).
async fn harvest_outgoing_references(
    article_id: &str,
    work_id: &str,
    work_refs_map: &std::collections::HashMap<&String, &Vec<String>>,
    mailto: &str,
    api_key: Option<&str>,
    db_state: &State<'_, DbState>,
) {
    let Some(ref_ids) = work_refs_map.get(&work_id.to_string()) else {
        return;
    };
    if ref_ids.is_empty() {
        return;
    }

    match openalex::client::fetch_works_by_ids(ref_ids, mailto, api_key).await {
        Ok(ref_works) => {
            let conn = match crate::db::connection::lock_conn(&db_state.conn) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[openalex] reference harvest DB lock failed: {e}");
                    return;
                }
            };
            for ref_work in &ref_works {
                let new_paper = mapping::map_work_to_reference_paper(ref_work);
                match reference_repo::insert_or_find_paper(&conn, &new_paper) {
                    Ok((paper, _)) => {
                        let _ = reference_repo::create_link(
                            &conn,
                            article_id,
                            &paper.id,
                            &crate::models::reference::ReferenceType::Reference,
                        );
                    }
                    Err(e) => {
                        eprintln!("[openalex] reference harvest insert error: {e}");
                    }
                }
            }
            let _ = audit_repo::create_entry(
                &conn,
                article_id,
                "reference_import",
                None,
                None,
                Some(&format!("Harvested {} OpenAlex references", ref_works.len())),
                "system",
            );
        }
        Err(e) => {
            eprintln!("[openalex] reference harvest fetch failed for article {article_id}: {e}");
            audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("OpenAlex reference harvest failed for article {article_id}: {e}"),
            );
        }
    }
}

/// Fetch and insert works that cite this article (incoming citations).
async fn harvest_incoming_citations(
    article_id: &str,
    work_id: &str,
    mailto: &str,
    api_key: Option<&str>,
    db_state: &State<'_, DbState>,
) {
    match openalex::client::fetch_citing_works(work_id, mailto, api_key).await {
        Ok(citing_works) => {
            if citing_works.is_empty() {
                return;
            }
            let conn = match crate::db::connection::lock_conn(&db_state.conn) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[openalex] citation harvest DB lock failed: {e}");
                    return;
                }
            };
            for cite_work in &citing_works {
                let new_paper = mapping::map_work_to_reference_paper(cite_work);
                match reference_repo::insert_or_find_paper(&conn, &new_paper) {
                    Ok((paper, _)) => {
                        let _ = reference_repo::create_link(
                            &conn,
                            article_id,
                            &paper.id,
                            &crate::models::reference::ReferenceType::Citation,
                        );
                    }
                    Err(e) => {
                        eprintln!("[openalex] citation harvest insert error: {e}");
                    }
                }
            }
            let _ = audit_repo::create_entry(
                &conn,
                article_id,
                "reference_import",
                None,
                None,
                Some(&format!("Harvested {} OpenAlex citations", citing_works.len())),
                "system",
            );
        }
        Err(e) => {
            eprintln!("[openalex] citation harvest fetch failed for article {article_id}: {e}");
            audit_repo::log_error_best_effort(
                &db_state.conn,
                &format!("OpenAlex citation harvest failed for article {article_id}: {e}"),
            );
        }
    }
}
