use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::bibtex::converter::convert_bibtex_entries;
use crate::bibtex::parser::parse_bibtex;
use crate::commands::dedup::classify_imported_articles;
use crate::db::app_settings_repo;
use crate::db::article_repo;
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::reference_repo;
use crate::error::AppError;
use crate::models::article::{Article, NewArticle};
use crate::models::reference::ReferenceType;
use crate::ris::cr_parser;
use crate::ris::import_pipeline::{
    filter_excluded, parse_and_validate, parse_and_validate_from_records, read_content,
    ValidationMode,
};
use crate::ris::types::RisRecord;
use crate::ris::validator::ErrorGroup;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub total_records: usize,
    pub valid_records: usize,
    pub error_count: usize,
    /// Valid records whose canonical DOI already exists in the library (an
    /// early informational signal; the import itself is unchanged - the
    /// classify phase still marks them as duplicates).
    pub duplicate_count: usize,
    /// Indices into the valid records that duplicate the library (the
    /// review-step Skip checkbox and the confirm-button arithmetic).
    pub duplicate_indices: Vec<usize>,
    pub errors: Vec<ImportError>,
    pub error_groups: Vec<ErrorGroup>,
    pub preview_articles: Vec<PreviewArticle>,
}

/// The canonical-DOI set from `dois` already present in the library
/// (case-insensitive, prefix-stripped - `check_dois_in_library` canonicalizes
/// both sides). Empty input short-circuits. Shared with the Zotero import's
/// skip path.
pub fn library_dois_present(
    conn: &rusqlite::Connection,
    dois: &[String],
) -> Result<std::collections::HashSet<String>, AppError> {
    if dois.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    Ok(article_repo::check_dois_in_library(conn, dois)?.into_iter().collect())
}

/// Indices (into `records`) whose canonical DOI is already present in the
/// library (case-insensitive, prefix-stripped). Powers the preview's
/// `duplicate_count`/`duplicate_indices` (the review-step Skip checkbox and
/// the confirm-button count) and the import-time skip filter. Runs in the
/// caller's short DB lock.
pub fn library_duplicate_indices(
    conn: &rusqlite::Connection,
    records: &[&RisRecord],
) -> Result<Vec<usize>, AppError> {
    let dois: Vec<String> = records.iter().filter_map(|r| r.doi.clone()).collect();
    let present = library_dois_present(conn, &dois)?;
    Ok(records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            crate::ris::doi::normalize_doi(record.doi.as_deref())
                .is_some_and(|d| present.contains(&d))
        })
        .map(|(index, _)| index)
        .collect())
}

/// Split records into (not-in-library, duplicate count) by canonical DOI
/// against the current library (the review-step Skip checkbox; records
/// without a DOI never match). Runs in the caller's short DB lock.
pub fn filter_library_duplicates<'a>(
    conn: &rusqlite::Connection,
    records: &[&'a RisRecord],
) -> Result<(Vec<&'a RisRecord>, usize), AppError> {
    let duplicates: std::collections::HashSet<usize> =
        library_duplicate_indices(conn, records)?.into_iter().collect();
    let mut kept: Vec<&RisRecord> = Vec::with_capacity(records.len());
    let mut skipped = 0usize;
    for (index, record) in records.iter().enumerate() {
        if duplicates.contains(&index) {
            skipped += 1;
        } else {
            kept.push(*record);
        }
    }
    Ok((kept, skipped))
}

/// Count records whose canonical DOI is already present in the library
/// (case-insensitive, prefix-stripped - `check_dois_in_library` canonicalizes
/// both sides). Records without a DOI never match. Runs in the caller's
/// short DB lock.
pub fn count_library_duplicates(
    conn: &rusqlite::Connection,
    records: &[&RisRecord],
) -> Result<usize, AppError> {
    Ok(library_duplicate_indices(conn, records)?.len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub record_index: usize,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewArticle {
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    /// Library duplicates dropped before insert (review-step Skip checkbox).
    pub skipped_duplicates: usize,
    pub skipped_by_user: usize,
    pub articles: Vec<Article>,
    pub remaining_capacity: usize,
    pub validation_errors: Vec<ImportError>,
    pub error_groups: Vec<ErrorGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseRisRequest {
    pub content: Option<String>,
    pub file_path: Option<String>,
    pub file_name: String,
    #[serde(default)]
    pub excluded_indices: Vec<usize>,
    /// Drop valid records whose canonical DOI already exists in the library
    /// (review-step Skip checkbox; default on from the UI).
    #[serde(default)]
    pub skip_duplicates: bool,
}

#[tauri::command]
pub async fn parse_ris_file(
    app: AppHandle,
    request: ParseRisRequest,
) -> Result<ImportPreview, AppError> {
    // `State` cannot cross into spawn_blocking; derive it from the app handle
    // inside the closure (OpenAlex pattern).
    let app_for_blocking = app.clone();
    tokio::task::spawn_blocking(move || {
        let db_state = app_for_blocking.state::<DbState>();
        let content = read_content(request.content, request.file_path)?;
        let output = parse_and_validate(&content, ValidationMode::Strict)?;

        // Early duplicate signal: valid records' DOIs vs the current library
        // (one short lock; never held across the parse).
        let duplicate_indices = {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            let valid_refs: Vec<&RisRecord> = output.valid_records.iter().collect();
            library_duplicate_indices(&conn, &valid_refs)?
        };

        let preview_articles: Vec<PreviewArticle> = output
            .valid_records
            .iter()
            .take(10)
            .map(|r| PreviewArticle {
                title: r.title.clone().unwrap_or_default(),
                authors: r.authors.clone(),
                publication_year: r.publication_year,
                journal: r.journal.clone(),
                doi: r.doi.clone(),
            })
            .collect();

        Ok(ImportPreview {
            total_records: output.total_records,
            valid_records: output.valid_records.len(),
            error_count: output.errors.len(),
            duplicate_count: duplicate_indices.len(),
            duplicate_indices,
            errors: output
                .errors
                .into_iter()
                .map(|e| ImportError { record_index: e.record_index, message: e.message })
                .collect(),
            error_groups: output.error_groups,
            preview_articles,
        })
    })
    .await
    .map_err(|e| AppError::Import(format!("Task panicked: {}", e)))?
}

pub fn ris_record_to_new_article(record: &RisRecord) -> NewArticle {
    let extras: Option<serde_json::Value> = if record.extras.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&record.extras).unwrap_or(serde_json::Value::Null))
    };

    let title = record.title.clone().unwrap_or_default();
    let abstract_text = record.abstract_text.clone().unwrap_or_default();
    let data_length = title.chars().count() + abstract_text.chars().count();
    let token_estimate = data_length / 4;

    /* Affiliation: prefer explicit field (set by BibTeX converter), otherwise
    extract first comma-separated part of author_address (RIS AD).
    e.g. "McGill Univ, Sch Comp Sci, Montreal..." → "McGill Univ" */
    let affiliation = record.affiliation.clone().or_else(|| {
        record.author_address.as_ref().and_then(|addr| {
            addr.split(',').next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        })
    });

    NewArticle {
        title,
        abstract_text,
        authors: record.authors.clone(),
        publication_year: record.publication_year,
        doi: record.doi.clone(),
        journal: record.journal.clone(),
        volume: record.volume.clone(),
        issue: record.issue.clone(),
        start_page: record.start_page.clone(),
        end_page: record.end_page.clone(),
        keywords: record.keywords.clone(),
        url: record.url.clone(),
        language: record.language.clone(),
        publisher: record.publisher.clone(),
        publisher_city: record.publisher_city.clone(),
        publisher_address: record.publisher_address.clone(),
        issn: record.issn.clone(),
        eissn: record.eissn.clone(),
        journal_index_id: None,
        reference_type: record.reference_type.clone(),
        date: record.date.clone(),
        author_address: record.author_address.clone(),
        affiliation,
        accession_number: record.accession_number.clone(),
        custom_field3: record.custom_field3.clone(),
        journal_abbreviation: record.journal_abbreviation.clone(),
        journal_iso_abbreviation: record.journal_iso_abbreviation.clone(),
        notes: record.notes.clone(),
        web_of_science_db: record.web_of_science_db.clone(),
        ris_extras: extras,
        import_source: None,
        data_length: Some(data_length),
        token_estimate: Some(token_estimate),
        num_cited: record.num_cited,
        num_references: record.num_references,
        has_full_text: false,
        full_text_file_name: None,
    }
}

#[tauri::command]
pub async fn import_ris_file(
    app: AppHandle,
    request: ParseRisRequest,
) -> Result<ImportResult, AppError> {
    let app_for_logging = app.clone();
    let file_name = request.file_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        let db_state = app.state::<DbState>();

        let content = read_content(request.content, request.file_path)?;
        let output = parse_and_validate(&content, ValidationMode::Strict)?;

        let (to_import, skipped_by_user) =
            filter_excluded(&output.valid_records, &request.excluded_indices);
        let skipped_validation = output.total_records - output.valid_records.len();

        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        // Pre-import duplicate skip (review-step Skip checkbox): drop records
        // whose canonical DOI already exists in the library. Everything else
        // (within-file duplicates and every other strategy) still flows to the
        // classify phase below.
        let (to_import, skipped_duplicates) = if request.skip_duplicates {
            filter_library_duplicates(&conn, &to_import)?
        } else {
            (to_import, 0)
        };

        if to_import.is_empty() {
            return Err(AppError::Import(
                "No articles to import. All records were either excluded, skipped as duplicates, or failed validation."
                    .to_string(),
            ));
        }

        let new_articles: Vec<NewArticle> =
            to_import.iter().map(|r| ris_record_to_new_article(r)).collect();

        let imported =
            article_repo::insert_articles_batch(&conn, &new_articles, &request.file_name)?;

        // Classify: move non-duplicates to working, keep duplicates in 'duplicate'
        let _classification = classify_imported_articles(&conn, &imported)?;

        // Resolve journal_index_id from journal_index table
        let _journal_resolved = article_repo::resolve_journal_links(&conn, &imported);

        // Extract CR (Cited References) from imported RIS records
        let _cr_errors = extract_cr_for_imported(&conn, &imported, &to_import);

        // Auto-link imported articles to existing unmatched reference papers
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

        // Re-fetch articles to reflect updated statuses after classification
        let updated_articles: Vec<Article> = imported
            .iter()
            .filter_map(|a| article_repo::get_article_by_id(&conn, &a.id).ok())
            .collect();

        let remaining = article_repo::remaining_capacity(&conn)?;

        // Imported articles affect bibliometrics - mark it stale.
        app_settings_repo::mark_biblio_needs_refresh(&conn);
        app_settings_repo::mark_wiki_needs_refresh(&conn);

        /* Auto-translate trigger: enqueue metadata-only translation jobs for
        non-English articles when `auto_translate = true`. Non-fatal.
        Tier 1a: capture imported IDs, then drop the connection guard BEFORE
        enqueuing so the import lock is not held across the (separately
        locking) batch-enqueue round-trip. */
        let imported_ids: Vec<String> = updated_articles.iter().map(|a| a.id.clone()).collect();
        let import_payload = ImportResult {
            imported_count: updated_articles.len(),
            skipped_count: skipped_validation,
            skipped_duplicates,
            skipped_by_user,
            articles: updated_articles,
            remaining_capacity: remaining,
            validation_errors: output
                .errors
                .into_iter()
                .map(|e| ImportError { record_index: e.record_index, message: e.message })
                .collect(),
            error_groups: output.error_groups,
        };
        // Drop the guard before the (re-locking) enqueue call.
        drop(conn);
        crate::commands::translation::try_enqueue_translations_for_import(
            &app,
            &db_state.conn,
            &imported_ids,
        );
        Ok(import_payload)
    })
    .await;

    match result {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(e)) => {
            eprintln!("[import] error in '{}': {e}", file_name);
            log_import_error(&app_for_logging, &format!("Import error ({}): {e}", file_name));
            Err(e)
        }
        Err(e) => {
            let err = AppError::Import(format!("Task panicked: {e}"));
            eprintln!("[import] panic in '{}': {err}", file_name);
            log_import_error(&app_for_logging, &format!("Import panic ({}): {err}", file_name));
            Err(err)
        }
    }
}

/// Log an import error to both the audit table and stderr.
fn log_import_error(app: &AppHandle, message: &str) {
    if let Some(db_state) = app.try_state::<DbState>() {
        // `try_state` returns Option (state may be tearing down during shutdown);
        // the inner best-effort helper tolerates a poisoned mutex.
        audit_repo::log_error_best_effort(&db_state.conn, message);
    }
}

#[tauri::command]
pub async fn parse_bibtex_file(
    app: AppHandle,
    request: ParseRisRequest,
) -> Result<ImportPreview, AppError> {
    // `State` cannot cross into spawn_blocking; derive it from the app handle
    // inside the closure (OpenAlex pattern).
    let app_for_blocking = app.clone();
    tokio::task::spawn_blocking(move || {
        let db_state = app_for_blocking.state::<DbState>();
        let content = read_content(request.content, request.file_path)?;

        let bibtex_result = parse_bibtex(&content);
        let records: Vec<RisRecord> = convert_bibtex_entries(&bibtex_result.entries);
        let output = parse_and_validate_from_records(&records, ValidationMode::Strict)?;

        // Early duplicate signal: valid records' DOIs vs the current library
        // (one short lock; never held across the parse).
        let duplicate_indices = {
            let conn = crate::db::connection::lock_conn(&db_state.conn)?;
            let valid_refs: Vec<&RisRecord> = output.valid_records.iter().collect();
            library_duplicate_indices(&conn, &valid_refs)?
        };

        let preview_articles: Vec<PreviewArticle> = output
            .valid_records
            .iter()
            .take(10)
            .map(|r| PreviewArticle {
                title: r.title.clone().unwrap_or_default(),
                authors: r.authors.clone(),
                publication_year: r.publication_year,
                journal: r.journal.clone(),
                doi: r.doi.clone(),
            })
            .collect();

        Ok(ImportPreview {
            total_records: output.total_records,
            valid_records: output.valid_records.len(),
            error_count: output.errors.len(),
            duplicate_count: duplicate_indices.len(),
            duplicate_indices,
            errors: output
                .errors
                .into_iter()
                .map(|e| ImportError { record_index: e.record_index, message: e.message })
                .collect(),
            error_groups: output.error_groups,
            preview_articles,
        })
    })
    .await
    .map_err(|e| AppError::Import(format!("Task panicked: {}", e)))?
}

#[tauri::command]
pub async fn import_bibtex_file(
    app: AppHandle,
    request: ParseRisRequest,
) -> Result<ImportResult, AppError> {
    let app_for_logging = app.clone();
    let file_name = request.file_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        let db_state = app.state::<DbState>();

        let content = read_content(request.content, request.file_path)?;

        let bibtex_result = parse_bibtex(&content);
        let records: Vec<RisRecord> = convert_bibtex_entries(&bibtex_result.entries);
        let output = parse_and_validate_from_records(&records, ValidationMode::Strict)?;

        let (to_import, skipped_by_user) =
            filter_excluded(&output.valid_records, &request.excluded_indices);
        let skipped_validation = output.total_records - output.valid_records.len();

        let conn = crate::db::connection::lock_conn(&db_state.conn)?;

        // Pre-import duplicate skip (review-step Skip checkbox): drop records
        // whose canonical DOI already exists in the library. Everything else
        // (within-file duplicates and every other strategy) still flows to the
        // classify phase below.
        let (to_import, skipped_duplicates) = if request.skip_duplicates {
            filter_library_duplicates(&conn, &to_import)?
        } else {
            (to_import, 0)
        };

        if to_import.is_empty() {
            return Err(AppError::Import(
                "No articles to import. All records were either excluded, skipped as duplicates, or failed validation."
                    .to_string(),
            ));
        }

        let new_articles: Vec<NewArticle> =
            to_import.iter().map(|r| ris_record_to_new_article(r)).collect();

        let imported =
            article_repo::insert_articles_batch(&conn, &new_articles, &request.file_name)?;

        let _classification = classify_imported_articles(&conn, &imported)?;

        // Resolve journal_index_id from journal_index table
        let _journal_resolved = article_repo::resolve_journal_links(&conn, &imported);

        // Extract CR (Cited References) from imported BibTeX records
        let _cr_errors = extract_cr_for_imported(&conn, &imported, &to_import);

        // Auto-link imported articles to existing unmatched reference papers
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

        let updated_articles: Vec<Article> = imported
            .iter()
            .filter_map(|a| article_repo::get_article_by_id(&conn, &a.id).ok())
            .collect();

        let remaining = article_repo::remaining_capacity(&conn)?;

        // Imported articles affect bibliometrics - mark it stale.
        app_settings_repo::mark_biblio_needs_refresh(&conn);
        app_settings_repo::mark_wiki_needs_refresh(&conn);

        // Auto-translate trigger (see `import_ris_file` for rationale).
        // Tier 1a: drop the guard before the (re-locking) enqueue call.
        let imported_ids: Vec<String> = updated_articles.iter().map(|a| a.id.clone()).collect();
        let import_payload = ImportResult {
            imported_count: updated_articles.len(),
            skipped_count: skipped_validation,
            skipped_duplicates,
            skipped_by_user,
            articles: updated_articles,
            remaining_capacity: remaining,
            validation_errors: output
                .errors
                .into_iter()
                .map(|e| ImportError { record_index: e.record_index, message: e.message })
                .collect(),
            error_groups: output.error_groups,
        };
        drop(conn);
        crate::commands::translation::try_enqueue_translations_for_import(
            &app,
            &db_state.conn,
            &imported_ids,
        );
        Ok(import_payload)
    })
    .await;

    match result {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(e)) => {
            eprintln!("[import] error in '{}': {e}", file_name);
            log_import_error(&app_for_logging, &format!("Import error ({}): {e}", file_name));
            Err(e)
        }
        Err(e) => {
            let err = AppError::Import(format!("Task panicked: {e}"));
            eprintln!("[import] panic in '{}': {err}", file_name);
            log_import_error(&app_for_logging, &format!("Import panic ({}): {err}", file_name));
            Err(err)
        }
    }
}

/// Extract CR (Cited References) from imported RIS records, insert them as
/// reference papers, and link to the articles. Non-fatal: errors logged to audit.
pub fn extract_cr_for_imported(
    conn: &rusqlite::Connection,
    imported: &[Article],
    records: &[&RisRecord],
) -> Vec<String> {
    let mut errors = Vec::new();

    for (article, record) in imported.iter().zip(records.iter()) {
        let extras_val = serde_json::to_value(&record.extras).unwrap_or(serde_json::Value::Null);
        let cr_papers = cr_parser::parse_cr_entries(&extras_val);
        if cr_papers.is_empty() {
            continue;
        }

        for cr_paper in &cr_papers {
            let mut paper_to_insert = cr_paper.clone();
            paper_to_insert.import_source = Some("cr_extraction".into());

            match reference_repo::insert_or_find_paper(conn, &paper_to_insert) {
                Ok((paper, _was_created)) => {
                    if let Err(e) = reference_repo::create_link(
                        conn,
                        &article.id,
                        &paper.id,
                        &ReferenceType::Reference,
                    ) {
                        errors.push(format!("CR link error for article {}: {}", article.id, e));
                    }
                }
                Err(e) => {
                    errors.push(format!("CR paper insert error for article {}: {}", article.id, e));
                }
            }
        }

        // Audit log per article
        if !cr_papers.is_empty() {
            let _ = audit_repo::create_entry(
                conn,
                &article.id,
                "reference_import",
                None,
                None,
                Some(&format!("Extracted {} CR references", cr_papers.len())),
                "system",
            );
        }
    }

    // Log errors
    for err in &errors {
        let _ = audit_repo::create_entry(
            conn,
            "",
            "error",
            None,
            None,
            Some(&format!("CR extraction: {}", err)),
            "system",
        );
    }

    errors
}

#[tauri::command]
pub fn get_articles(db_state: State<'_, DbState>) -> Result<Vec<Article>, AppError> {
    let conn = crate::db::connection::lock_conn(&db_state.conn)?;
    article_repo::get_all_articles(&conn)
}
