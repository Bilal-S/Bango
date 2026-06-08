use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::db::reference_repo;
use crate::error::AppError;
use crate::models::reference::{
    ArticleReference, ArticleReferenceLink, MatchStatus, NewReferencePaper, ReferenceType,
};
use crate::ris::cr_parser;
use crate::ris::parser;
use crate::ris::types::RisRecord;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractCrPayload {
    pub article_id: String,
    pub ris_extras: Option<serde_json::Value>,
}

/// Extract CR (Cited References) from an article's RIS extras,
/// insert them as reference papers, and link them to the article.
#[tauri::command]
pub fn extract_cr_references(
    db_state: tauri::State<'_, DbState>,
    payload: ExtractCrPayload,
) -> Result<ExtractResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let Some(ref extras) = payload.ris_extras else {
        return Ok(ExtractResult { papers_created: 0, links_created: 0, errors: vec![] });
    };

    let cr_papers = cr_parser::parse_cr_entries(extras);
    if cr_papers.is_empty() {
        return Ok(ExtractResult { papers_created: 0, links_created: 0, errors: vec![] });
    }

    let mut papers_created = 0usize;
    let mut links_created = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for cr_paper in &cr_papers {
        let mut paper_to_insert = cr_paper.clone();
        paper_to_insert.import_source = Some("cr_extraction".into());

        match reference_repo::insert_or_find_paper(&conn, &paper_to_insert) {
            Ok((paper, was_created)) => {
                if was_created {
                    papers_created += 1;

                    // Try to auto-match the new paper to an existing article
                    if let Ok(Some(matched_id)) =
                        reference_repo::auto_match_paper_to_article(&conn, &paper)
                    {
                        if let Err(e) = reference_repo::update_paper_match(
                            &conn,
                            &paper.id,
                            &MatchStatus::Matched,
                            Some(&matched_id),
                        ) {
                            errors.push(format!(
                                "Failed to update match for paper {}: {}",
                                paper.id, e
                            ));
                        }
                    }
                }

                // Create link: CR entries are references (works cited by the parent article)
                match reference_repo::create_link(
                    &conn,
                    &payload.article_id,
                    &paper.id,
                    &ReferenceType::Reference,
                ) {
                    Ok(_) => {
                        links_created += 1;
                    }
                    Err(e) => {
                        errors.push(format!(
                            "Failed to link paper {} to article {}: {}",
                            paper.id, payload.article_id, e
                        ));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "Failed to insert CR paper '{}': {}",
                    cr_paper.title.as_deref().unwrap_or("(unknown)"),
                    e
                ));
            }
        }
    }

    // Audit log
    let details = format!(
        "Extracted {} CR references for article {} ({} new papers, {} links)",
        cr_papers.len(),
        payload.article_id,
        papers_created,
        links_created,
    );
    let _ = audit_repo::create_entry(
        &conn,
        &payload.article_id,
        "reference_import",
        None,
        None,
        Some(&details),
        "system",
    );

    // Log individual errors
    for err in &errors {
        let _ = audit_repo::create_entry(
            &conn,
            &payload.article_id,
            "error",
            None,
            None,
            Some(err),
            "system",
        );
    }

    Ok(ExtractResult { papers_created, links_created, errors })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractResult {
    pub papers_created: usize,
    pub links_created: usize,
    pub errors: Vec<String>,
}

/// Get all reference papers linked to an article.
#[tauri::command]
pub fn get_article_references(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
    ref_type: Option<ReferenceType>,
) -> Result<Vec<ArticleReference>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    reference_repo::get_references_for_article(&conn, &article_id, ref_type.as_ref())
}

/// Manually link a reference paper to an article.
#[tauri::command]
pub fn link_reference_to_article(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
    reference_paper_id: String,
    ref_type: ReferenceType,
) -> Result<ArticleReferenceLink, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let link = reference_repo::create_link(&conn, &article_id, &reference_paper_id, &ref_type)?;

    let _ = audit_repo::create_entry(
        &conn,
        &article_id,
        "reference_import",
        None,
        None,
        Some(&format!("Linked {} paper {}", ref_type.as_int(), reference_paper_id)),
        "user",
    );

    Ok(link)
}

/// Delete all references for an article.
#[tauri::command]
pub fn delete_article_references(
    db_state: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<(), AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    reference_repo::delete_references_for_article(&conn, &article_id)?;

    let _ = audit_repo::create_entry(
        &conn,
        &article_id,
        "reference_import",
        None,
        None,
        Some("Removed all reference links"),
        "user",
    );

    Ok(())
}

/// Insert a reference paper (or find existing by DOI/title).
#[tauri::command]
pub fn upsert_reference_paper(
    db_state: tauri::State<'_, DbState>,
    paper: NewReferencePaper,
) -> Result<ReferencePaperResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let (paper, was_created) = reference_repo::insert_or_find_paper(&conn, &paper)?;

    // Auto-match to article if newly created
    if was_created {
        if let Ok(Some(matched_id)) = reference_repo::auto_match_paper_to_article(&conn, &paper) {
            let _ = reference_repo::update_paper_match(
                &conn,
                &paper.id,
                &MatchStatus::Matched,
                Some(&matched_id),
            );
        }
    }

    Ok(ReferencePaperResult { paper, was_created })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencePaperResult {
    pub paper: crate::models::reference::ReferencePaper,
    pub was_created: bool,
}

/// Convert a parsed RIS record into a NewReferencePaper for the reference papers table.
fn ris_record_to_reference_paper(record: &RisRecord) -> NewReferencePaper {
    NewReferencePaper {
        title: record.title.clone(),
        abstract_text: record.abstract_text.clone(),
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
        reference_type: record.reference_type.clone(),
        date: record.date.clone(),
        notes: record.notes.clone(),
        ris_extras: None,
        match_status: None,
        matched_article_id: None,
        import_source: Some("reference_file_import".into()),
    }
}

/// Lightweight preview of a reference paper (no DB writes).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPaper {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
}

/// Result from previewing a reference import (parse-only, no DB writes).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub papers: Vec<PreviewPaper>,
    pub total_count: usize,
    pub errors: Vec<String>,
}

/// Preview references/citations from a file without importing.
/// Parses the file and returns what would be imported.
#[tauri::command]
pub fn preview_references_import(file_path: String) -> Result<PreviewResult, AppError> {
    let content = std::fs::read_to_string(&file_path).map_err(|e| {
        AppError::Import(format!("Failed to read file '{}': {}", file_path, e))
    })?;

    let parse_result = parser::parse_ris(&content)?;
    let records = &parse_result.records;

    if records.is_empty() {
        return Ok(PreviewResult { papers: vec![], total_count: 0, errors: vec![] });
    }

    let mut papers = Vec::with_capacity(records.len());
    let errors: Vec<String> = Vec::new();

    for record in records {
        papers.push(PreviewPaper {
            title: record.title.clone(),
            authors: record.authors.clone(),
            publication_year: record.publication_year,
            doi: record.doi.clone(),
            journal: record.journal.clone(),
        });

        // Also count CR sub-references in the preview
        if let Some(extras) = record.extras.get("CR") {
            let cr_json = serde_json::json!({ "CR": extras });
            let cr_papers = cr_parser::parse_cr_entries(&cr_json);
            for cr in &cr_papers {
                papers.push(PreviewPaper {
                    title: cr.title.clone(),
                    authors: cr.authors.clone(),
                    publication_year: cr.publication_year,
                    doi: cr.doi.clone(),
                    journal: cr.journal.clone(),
                });
            }
        }
    }

    let total_count = papers.len();

    Ok(PreviewResult { papers, total_count, errors })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReferencesPayload {
    pub article_id: String,
    pub file_path: String,
    /// "reference" = Backward Citations (references used in this paper)
    /// "citation" = Forward Citations (papers referencing this article)
    pub ref_type: String,
}

/// Import references/citations from an RIS file and link them to an article.
#[tauri::command]
pub fn import_references_for_article(
    db_state: tauri::State<'_, DbState>,
    payload: ImportReferencesPayload,
) -> Result<ExtractResult, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    // Determine reference type
    let ref_type = match payload.ref_type.as_str() {
        "citation" => ReferenceType::Citation,
        _ => ReferenceType::Reference,
    };

    // Read and parse RIS file
    let content = std::fs::read_to_string(&payload.file_path).map_err(|e| {
        AppError::Import(format!("Failed to read file '{}': {}", payload.file_path, e))
    })?;

    let parse_result = parser::parse_ris(&content)?;
    let records = &parse_result.records;

    if records.is_empty() {
        return Ok(ExtractResult { papers_created: 0, links_created: 0, errors: vec![] });
    }

    let mut papers_created = 0usize;
    let mut links_created = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for record in records {
        let paper = ris_record_to_reference_paper(record);

        // If the record has CR entries, also parse those
        let cr_papers = if let Some(extras) = record.extras.get("CR") {
            let cr_json = serde_json::json!({ "CR": extras });
            cr_parser::parse_cr_entries(&cr_json)
        } else {
            vec![]
        };

        // Insert the main record as a paper
        match reference_repo::insert_or_find_paper(&conn, &paper) {
            Ok((inserted, was_created)) => {
                if was_created {
                    papers_created += 1;
                    if let Ok(Some(matched_id)) =
                        reference_repo::auto_match_paper_to_article(&conn, &inserted)
                    {
                        let _ = reference_repo::update_paper_match(
                            &conn,
                            &inserted.id,
                            &MatchStatus::Matched,
                            Some(&matched_id),
                        );
                    }
                }

                match reference_repo::create_link(
                    &conn,
                    &payload.article_id,
                    &inserted.id,
                    &ref_type,
                ) {
                    Ok(_) => links_created += 1,
                    Err(e) => errors.push(format!(
                        "Failed to link paper '{}' to article: {}",
                        paper.title.as_deref().unwrap_or("(unknown)"),
                        e
                    )),
                }
            }
            Err(e) => errors.push(format!(
                "Failed to insert paper '{}': {}",
                paper.title.as_deref().unwrap_or("(unknown)"),
                e
            )),
        }

        // Also insert any CR sub-references (always as Reference type, not the selected type)
        for cr_paper in &cr_papers {
            let mut cr_to_insert = cr_paper.clone();
            cr_to_insert.import_source = Some("cr_extraction".into());

            match reference_repo::insert_or_find_paper(&conn, &cr_to_insert) {
                Ok((cr_inserted, cr_created)) => {
                    if cr_created {
                        papers_created += 1;
                        if let Ok(Some(matched_id)) =
                            reference_repo::auto_match_paper_to_article(&conn, &cr_inserted)
                        {
                            let _ = reference_repo::update_paper_match(
                                &conn,
                                &cr_inserted.id,
                                &MatchStatus::Matched,
                                Some(&matched_id),
                            );
                        }
                    }
                    // CR entries link to the imported paper, not the original article
                    let _ = reference_repo::create_link(
                        &conn,
                        &payload.article_id,
                        &cr_inserted.id,
                        &ref_type,
                    );
                }
                Err(e) => errors.push(format!(
                    "Failed to insert CR paper '{}': {}",
                    cr_paper.title.as_deref().unwrap_or("(unknown)"),
                    e
                )),
            }
        }
    }

    // Audit log
    let type_label = match ref_type {
        ReferenceType::Reference => "backward references",
        ReferenceType::Citation => "forward citations",
    };
    let details = format!(
        "Imported {} {} from file for article {} ({} new papers, {} links)",
        records.len(),
        type_label,
        payload.article_id,
        papers_created,
        links_created,
    );
    let _ = audit_repo::create_entry(
        &conn,
        &payload.article_id,
        "reference_import",
        None,
        None,
        Some(&details),
        "user",
    );

    // Log individual errors
    for err in &errors {
        let _ = audit_repo::create_entry(
            &conn,
            &payload.article_id,
            "error",
            None,
            None,
            Some(err),
            "system",
        );
    }

    Ok(ExtractResult { papers_created, links_created, errors })
}
