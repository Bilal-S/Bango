//! Tauri commands for the OpenAlex search integration.
//!
//! - `search_openalex`: search the OpenAlex catalog and return results with
//!   reconstructed abstracts + 200-char snippets + `already_in_library` flags.
//! - `import_openalex_articles`: map + insert + dedup + audit selected works.
//!   Reuses the exact same pipeline as `import_ris_file`.
//! - `check_dois_in_library`: batch-check which DOIs already exist in the library.
//! - `get_openalex_settings` / `set_openalex_settings`: read/write the API key +
//!   mailto + retrieve-references toggle.

use std::collections::HashSet;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::llm_config_repo;
use crate::db::reference_repo;
use crate::error::AppError;
use crate::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use crate::openalex;
use crate::openalex::mapping;
use crate::openalex::smart_search::{self, SmartSearchQuery};
use crate::openalex::OpenAlexResultItem;
use crate::openalex::OpenAlexSearchParams;
use crate::openalex::OpenAlexSearchResponse;
use crate::openalex::OpenAlexWork;

use super::import::ImportResult;

/// Search OpenAlex works. Calls the OpenAlex API, reconstructs abstracts,
/// truncates snippets, and checks which DOIs already exist in the library.
#[tauri::command]
pub async fn search_openalex(
    db_state: State<'_, DbState>,
    params: OpenAlexSearchParams,
) -> Result<OpenAlexSearchResponse, AppError> {
    // Read mailto + api_key in a short DB lock, then release for the HTTP call.
    let (mailto, api_key) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let mailto = openalex::get_mailto(&conn)?;
        let api_key = openalex::get_api_key(&conn)?;
        (mailto, api_key)
    };

    // Call the OpenAlex API.
    let api_response = openalex::client::search_works(
        &params.query,
        &params.filters,
        &params.sort,
        params.per_page,
        params.page,
        &mailto,
        api_key.as_deref(),
    )
    .await?;

    // Collect DOIs from results for the library check.
    let dois: Vec<String> =
        api_response.results.iter().filter_map(mapping::work_doi_normalized).collect();

    // Batch-check which DOIs are already in the library.
    let library_dois: HashSet<String> = {
        if dois.is_empty() {
            HashSet::new()
        } else {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            article_repo::check_dois_in_library(&conn, &dois)?.into_iter().collect()
        }
    };

    // Build result items with reconstructed abstracts + snippets + library flags.
    let results: Vec<OpenAlexResultItem> = api_response
        .results
        .into_iter()
        .map(|work| {
            let abstract_text = mapping::reconstruct_abstract(&work.abstract_inverted_index);
            let snippet = mapping::truncate_snippet(&abstract_text);
            let doi = mapping::work_doi_normalized(&work);
            let already_in_library =
                doi.as_ref().map(|d| library_dois.contains(d)).unwrap_or(false);
            OpenAlexResultItem { work, abstract_text, snippet, already_in_library }
        })
        .collect();

    Ok(OpenAlexSearchResponse {
        results,
        total_count: api_response.meta.count,
        page: api_response.meta.page,
        per_page: api_response.meta.per_page,
    })
}

/// Synchronous DB work for OpenAlex import: insert, classify, resolve, audit.
/// Returns the import result + the data needed for the async phases.
struct ImportDbResult {
    import_payload: ImportResult,
    /// (article_id, openalex_work_id) pairs for fetching full work data.
    article_work_pairs: Vec<(String, String)>,
    /// (article_id, pdf_url) pairs for the PDF download phase.
    pdf_pairs: Vec<(String, String)>,
    retrieve_references: bool,
    mailto: String,
    api_key: Option<String>,
}

/// Import selected OpenAlex works into the article library.
///
/// Three-phase pipeline:
/// 1. **Sync DB work** (`spawn_blocking`): insert, classify, resolve, audit,
///    mark staleness, enqueue translations.
/// 2. **Reference harvest** (async, if `retrieve_references` is enabled):
///    fetch full work data via `fetch_works_by_ids` to get `referenced_works`,
///    then batch-insert them as `reference_papers` + `article_reference_links`.
/// 3. **PDF download** (async, for each imported article with an OA URL):
///    download + `attach_full_text_inner`. Non-fatal: failures are logged to
///    the article's audit trail (not the generic diagnostic log) so the user
///    can see them in the Audit Timeline. When `auto_summarize` is true and
///    the LLM is configured, the AI summary pipeline runs after a successful
///    attach (mirrors the `bango-full-text-summaries` localStorage behavior
///    from the manual attach path).
#[tauri::command]
pub async fn import_openalex_articles(
    app: AppHandle,
    db_state: State<'_, DbState>,
    works: Vec<OpenAlexWork>,
    auto_summarize: Option<bool>,
    include_section_summaries: Option<bool>,
) -> Result<ImportResult, AppError> {
    if works.is_empty() {
        return Err(AppError::Import(
            "No works to import. Select at least one result.".to_string(),
        ));
    }

    let auto_summarize = auto_summarize.unwrap_or(false);
    let include_section_summaries = include_section_summaries.unwrap_or(false);

    // Clone the app handle so it's available in both the spawn_blocking closure
    // and the async phases below.
    let app_for_blocking = app.clone();

    // Phase 1: Synchronous DB work (insert, classify, resolve, audit).
    let db_result = tokio::task::spawn_blocking(move || -> Result<ImportDbResult, AppError> {
        let db_state = app_for_blocking.state::<DbState>();

        let new_articles = mapping::map_works_to_new_articles(&works);
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        let imported = article_repo::insert_articles_batch(&conn, &new_articles, "openalex")?;

        let _classification = crate::commands::dedup::classify_imported_articles(&conn, &imported)?;
        let _journal_resolved = article_repo::resolve_journal_links(&conn, &imported);

        let links_created = reference_repo::link_imported_articles_to_papers(&conn, &imported);
        if links_created > 0 {
            let _ = audit_repo::create_entry(
                &conn,
                "",
                "reference_link",
                None,
                None,
                Some(&format!(
                    "Auto-linked {} imported articles to existing reference papers",
                    links_created
                )),
                "system",
            );
        }

        let updated_articles: Vec<_> = imported
            .iter()
            .filter_map(|a| article_repo::get_article_by_id(&conn, &a.id).ok())
            .collect();

        let remaining = article_repo::remaining_capacity(&conn)?;

        app_settings_repo::mark_biblio_needs_refresh(&conn);
        app_settings_repo::mark_wiki_needs_refresh(&conn);

        // Read settings for the async phases.
        let retrieve_references =
            app_settings_repo::get_setting(&conn, "openalex_retrieve_references")?
                .map(|v| v == "true")
                .unwrap_or(false);
        let mailto = openalex::get_mailto(&conn)?;
        let api_key = openalex::get_api_key(&conn)?;

        // Collect (article_id, openalex_work_id) pairs for the reference harvest.
        // The search select excludes `referenced_works`, so we need to re-fetch
        // the full work data to get them.
        let article_work_pairs: Vec<(String, String)> = if retrieve_references {
            works
                .iter()
                .zip(imported.iter())
                .map(|(work, article)| (article.id.clone(), work.id.clone()))
                .collect()
        } else {
            Vec::new()
        };

        // Collect (article_id, pdf_url) pairs for the PDF download phase.
        // Prefer primaryLocation.pdfUrl, fall back to openAccess.oaUrl.
        let pdf_pairs: Vec<(String, String)> = works
            .iter()
            .zip(imported.iter())
            .filter_map(|(work, article)| {
                let pdf_url = work
                    .primary_location
                    .as_ref()
                    .and_then(|loc| loc.pdf_url.clone())
                    .or_else(|| work.open_access.as_ref().and_then(|oa| oa.oa_url.clone()));
                pdf_url.map(|url| (article.id.clone(), url))
            })
            .collect();

        let imported_ids: Vec<String> = updated_articles.iter().map(|a| a.id.clone()).collect();

        let import_payload = ImportResult {
            imported_count: updated_articles.len(),
            skipped_count: 0,
            skipped_by_user: 0,
            articles: updated_articles,
            remaining_capacity: remaining,
            validation_errors: vec![],
            error_groups: vec![],
        };

        // Drop the guard before the (re-locking) enqueue call.
        drop(conn);

        crate::commands::translation::try_enqueue_translations_for_import(
            &app_for_blocking,
            &db_state.conn,
            &imported_ids,
        );

        Ok(ImportDbResult {
            import_payload,
            article_work_pairs,
            pdf_pairs,
            retrieve_references,
            mailto,
            api_key,
        })
    })
    .await
    .map_err(|e| AppError::Import(format!("Task panicked: {e}")))??;

    // Phase 2: Reference + Citation harvest (async, only if retrieve_references is enabled).
    // Delegates to the `openalex::reference_harvest` module which fetches both
    // outgoing references (the article's bibliography) and incoming citations
    // (works that cite this article), inserting them as `reference_papers` +
    // `article_reference_links` with the appropriate `ReferenceType`.
    if db_result.retrieve_references && !db_result.article_work_pairs.is_empty() {
        openalex::reference_harvest::harvest_references_and_citations(
            &db_result.article_work_pairs,
            &db_result.mailto,
            db_result.api_key.as_deref(),
            &db_state,
        )
        .await;
    }

    // Phase 3: PDF download (async, for each imported article with an OA URL).
    // Non-fatal: failures are logged to the article's audit trail (action =
    // "error") so they surface in the Audit Timeline, NOT just the generic
    // Diagnostics feed (`log_error_best_effort` writes article_id = NULL).
    let orchestrator = app.state::<Arc<LlmOrchestrator>>();
    for (article_id, pdf_url) in &db_result.pdf_pairs {
        match openalex::client::download_pdf(pdf_url).await {
            Ok(pdf_bytes) => {
                // Write to temp file, then attach via the existing pipeline.
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join(format!("openalex_{article_id}.pdf"));
                if let Err(e) = std::fs::write(&temp_file, &pdf_bytes) {
                    eprintln!("[openalex] PDF temp write failed for article {article_id}: {e}");
                    let _ = log_article_error(
                        &db_state,
                        article_id,
                        &format!("OpenAlex PDF download: temp file write failed: {e}"),
                    );
                    continue;
                }

                let attach_result = {
                    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                    let storage_dir = crate::commands::full_text::compute_storage_dir(&conn)?;
                    crate::commands::full_text::attach_full_text_inner(
                        &conn,
                        article_id,
                        &temp_file,
                        &storage_dir,
                    )
                };

                let _ = std::fs::remove_file(&temp_file);

                match attach_result {
                    Ok(_) => {
                        let _ = audit_entry(
                            &db_state,
                            article_id,
                            "full_text_attach",
                            &format!("PDF downloaded from OpenAlex: {pdf_url}"),
                        );

                        // Trigger automatic AI summary only when the user has
                        // enabled auto-summarize (the `bango-full-text-summaries`
                        // localStorage flag, passed through from the frontend)
                        // AND the LLM is configured. Mirrors the manual attach
                        // path's `onAttached` hook in `article-list.vue`.
                        if auto_summarize {
                            let llm_config = {
                                let conn = crate::db::connection::lock_conn(&db_state.conn)?;
                                crate::db::llm_config_repo::get_config(&conn)?
                            };
                            if llm_config.is_some() {
                                match crate::commands::summary::generate_article_ai_summary_inner(
                                    &db_state,
                                    &app,
                                    &orchestrator,
                                    article_id,
                                    include_section_summaries,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        let _ = audit_entry(
                                            &db_state,
                                            article_id,
                                            "ai_summary",
                                            "Auto-generated AI summary after OpenAlex PDF download",
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!("[openalex] auto AI summary failed for article {article_id}: {e}");
                                        let _ = log_article_error(
                                            &db_state,
                                            article_id,
                                            &format!("OpenAlex auto AI summary failed: {e}"),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[openalex] PDF text extraction failed for article {article_id}: {e}"
                        );
                        let _ = log_article_error(
                            &db_state,
                            article_id,
                            &format!(
                            "OpenAlex PDF downloaded from {pdf_url} but text extraction failed: {e}"
                        ),
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("[openalex] PDF download failed for article {article_id}: {e}");
                let _ = log_article_error(
                    &db_state,
                    article_id,
                    &format!("OpenAlex PDF download failed from {pdf_url}: {e}"),
                );
            }
        }
    }

    Ok(db_result.import_payload)
}

/// Write an article-scoped audit entry (action = "error") so the failure
/// surfaces in the article's Audit Timeline, not just the generic Diagnostics
/// feed. Acquires the DB lock tolerantly; if the mutex is poisoned the failure
/// is swallowed because the caller is already on an error/non-fatal path.
fn log_article_error(
    db_state: &State<'_, DbState>,
    article_id: &str,
    details: &str,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::create_entry(&conn, article_id, "error", None, None, Some(details), "system")
        .map(|_| ())
}

/// Write an article-scoped audit entry with the given action + details.
fn audit_entry(
    db_state: &State<'_, DbState>,
    article_id: &str,
    action: &str,
    details: &str,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    audit_repo::create_entry(&conn, article_id, action, None, None, Some(details), "openalex")
        .map(|_| ())
}

/// Check which DOIs from the input list already exist in the `articles` table.
#[tauri::command]
pub fn check_dois_in_library(
    db_state: State<'_, DbState>,
    dois: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::check_dois_in_library(&conn, &dois)
}

// ── Settings commands ──────────────────────────────────────────────────────

/// OpenAlex settings (API key is returned as a masked string for security).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexSettings {
    pub has_api_key: bool,
    pub mailto: String,
    pub retrieve_references: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlexSettingsInput {
    pub api_key: Option<String>,
    pub mailto: Option<String>,
    pub retrieve_references: Option<bool>,
}

/// Read the OpenAlex settings from `app_settings`.
#[tauri::command]
pub fn get_openalex_settings(db_state: State<'_, DbState>) -> Result<OpenAlexSettings, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    let has_api_key = openalex::get_api_key(&conn)?.is_some();
    let mailto = openalex::get_mailto(&conn)?;
    let retrieve_references =
        app_settings_repo::get_setting(&conn, "openalex_retrieve_references")?
            .map(|v| v == "true")
            .unwrap_or(false);
    Ok(OpenAlexSettings { has_api_key, mailto, retrieve_references })
}

/// Write the OpenAlex settings to `app_settings`. Only non-`None` fields are
/// updated, so the frontend can send a partial update.
#[tauri::command]
pub fn set_openalex_settings(
    db_state: State<'_, DbState>,
    settings: OpenAlexSettingsInput,
) -> Result<(), AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    if let Some(key) = settings.api_key {
        openalex::set_api_key(&conn, Some(&key))?;
    }
    if let Some(mailto) = settings.mailto {
        app_settings_repo::set_setting(&conn, "openalex_mailto", Some(&mailto))?;
    }
    if let Some(retrieve) = settings.retrieve_references {
        app_settings_repo::set_setting(
            &conn,
            "openalex_retrieve_references",
            Some(if retrieve { "true" } else { "false" }),
        )?;
    }
    Ok(())
}

/// Download a PDF from an OpenAlex OA URL and attach it as full text for the
/// given article. Gracefully handles CAPTCHA/paywall pages by returning an
/// error message instead of crashing.
#[tauri::command]
pub async fn download_and_attach_openalex_pdf(
    db_state: State<'_, DbState>,
    article_id: String,
    pdf_url: String,
) -> Result<bool, AppError> {
    // 1. Download the PDF bytes (async, no DB lock).
    let pdf_bytes = openalex::client::download_pdf(&pdf_url).await?;

    // 2. Write to a temp file, then use the existing attach_full_text_inner.
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("openalex_{article_id}.pdf"));
    std::fs::write(&temp_file, &pdf_bytes)
        .map_err(|e| AppError::Import(format!("Failed to write temp PDF: {e}")))?;

    // 3. Attach via the existing pipeline (extract text, copy to fulltext/, chunk).
    let attach_result = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let storage_dir = crate::commands::full_text::compute_storage_dir(&conn)?;
        crate::commands::full_text::attach_full_text_inner(
            &conn,
            &article_id,
            &temp_file,
            &storage_dir,
        )
    };

    // 4. Clean up the temp file (best-effort).
    let _ = std::fs::remove_file(&temp_file);

    match attach_result {
        Ok(_) => Ok(true),
        Err(e) => Err(AppError::Import(format!("PDF downloaded but text extraction failed: {e}"))),
    }
}

// ── Smart Search command ───────────────────────────────────────────────────

/// Generate an OpenAlex Boolean query from the research aims + inclusion/exclusion
/// criteria via the LLM. The user reviews the query before executing it.
#[tauri::command]
pub async fn smart_search_openalex(
    db_state: State<'_, DbState>,
    orchestrator: State<'_, Arc<LlmOrchestrator>>,
) -> Result<SmartSearchQuery, AppError> {
    let (config, aims, inclusion, exclusion) = {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        let config = llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let (aims, inclusion, exclusion) = smart_search::read_aims_and_criteria(&conn)?;
        (config, aims, inclusion, exclusion)
    };

    let (system_prompt, user_prompt) =
        smart_search::build_smart_search_prompt(&aims, &inclusion, &exclusion);

    let result = orchestrator
        .send(&config, &system_prompt, &user_prompt, LlmRequestType::OpenAlexSmartSearch)
        .await;
    if let Err(ref e) = result {
        audit_repo::log_error_best_effort(
            &db_state.conn,
            &format!("OpenAlex smart search generation failed: {e}"),
        );
    }
    let (response, _tokens) = result?;

    let parsed = smart_search::parse_smart_search_response(&response)?;

    {
        let conn = crate::db::connection::lock_conn(&db_state.conn)?;
        audit_repo::log_system_action(
            &conn,
            audit_repo::SystemAction::SearchStrategy,
            &format!("Generated OpenAlex smart search for {} aim(s)", aims.len()),
        )?;
    }

    Ok(parsed)
}
