//! Reference + Citation harvest for OpenAlex imports (both directions of citation graph).

use tauri::State;

use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::reference_repo;
use crate::openalex;
use crate::openalex::mapping;

/// Harvest references + citations for imported article-work pairs.
///
/// Non-fatal: per-article errors logged to audit trail. DB locks only for
/// millisecond-scale writes.
pub async fn harvest_references_and_citations(
    article_work_pairs: &[(String, String)],
    mailto: &str,
    api_key: Option<&str>,
    db_state: &State<'_, DbState>,
) {
    if article_work_pairs.is_empty() {
        return;
    }

    // Fetch full work data for all imported articles to get their
    // `referenced_works` arrays (search `select` excludes this field).
    let openalex_ids: Vec<String> = article_work_pairs.iter().map(|(_, wid)| wid.clone()).collect();

    let fetched_works =
        match openalex::client::fetch_works_by_ids(&openalex_ids, mailto, api_key).await {
            Ok(works) => works,
            Err(e) => {
                eprintln!("[openalex] reference harvest initial fetch failed: {e}");
                return;
            }
        };

    // Build lookup: openalex_id -> referenced_works
    let work_refs_map: std::collections::HashMap<&String, &Vec<String>> =
        fetched_works.iter().map(|w| (&w.id, &w.referenced_works)).collect();

    for (article_id, work_id) in article_work_pairs {
        // Outgoing references (bibliography)
        harvest_outgoing_references(article_id, work_id, &work_refs_map, mailto, api_key, db_state)
            .await;

        // Incoming citations
        harvest_incoming_citations(article_id, work_id, mailto, api_key, db_state).await;
    }
}

/// Write article-scoped audit error so harvest failures surface in Audit Timeline.
fn log_harvest_error(db_state: &State<'_, DbState>, article_id: &str, details: &str) {
    match crate::db::connection::lock_conn(&db_state.conn) {
        Ok(conn) => {
            let _ = audit_repo::create_entry(
                &conn,
                article_id,
                "error",
                None,
                None,
                Some(details),
                "system",
            );
        }
        Err(e) => {
            eprintln!("[openalex] harvest error audit write failed for article {article_id}: {e}");
        }
    }
}

/// Fetch and insert the article's outgoing references (bibliography).
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
            log_harvest_error(
                db_state,
                article_id,
                &format!("OpenAlex reference harvest failed: {e}"),
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
            log_harvest_error(
                db_state,
                article_id,
                &format!("OpenAlex citation harvest failed: {e}"),
            );
        }
    }
}
