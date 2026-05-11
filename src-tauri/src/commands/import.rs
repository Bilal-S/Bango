use serde::{Deserialize, Serialize};
use tauri::State;

/// Maximum RIS file size: 100 MB. Prevents OOM from accidentally importing huge files.
const MAX_RIS_FILE_SIZE: u64 = 100 * 1024 * 1024;

use crate::commands::dedup::classify_imported_articles;
use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::{Article, NewArticle};
use crate::ris::parser::parse_ris;
use crate::ris::types::RisRecord;
use crate::ris::validator::{validate_all_grouped, ErrorGroup};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub total_records: usize,
    pub valid_records: usize,
    pub error_count: usize,
    pub errors: Vec<ImportError>,
    pub error_groups: Vec<ErrorGroup>,
    pub preview_articles: Vec<PreviewArticle>,
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
}

#[tauri::command]
pub fn parse_ris_file(request: ParseRisRequest) -> Result<ImportPreview, AppError> {
    let content = if let Some(c) = request.content {
        c
    } else if let Some(p) = request.file_path {
        let metadata = std::fs::metadata(&p)
            .map_err(|e| AppError::Import(format!("Failed to read file metadata: {}", e)))?;
        if metadata.len() > MAX_RIS_FILE_SIZE {
            return Err(AppError::Import(format!(
                "File too large: {:.1} MB (maximum is {:.0} MB)",
                metadata.len() as f64 / (1024.0 * 1024.0),
                MAX_RIS_FILE_SIZE as f64 / (1024.0 * 1024.0)
            )));
        }
        std::fs::read_to_string(p)
            .map_err(|e| AppError::Import(format!("Failed to read file: {}", e)))?
    } else {
        return Err(AppError::Import("No content or file path provided".into()));
    };

    let parse_result = parse_ris(&content)?;
    let (valid, errors, error_groups) = validate_all_grouped(&parse_result.records);

    let preview_articles: Vec<PreviewArticle> = valid
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
        total_records: parse_result.records.len(),
        valid_records: valid.len(),
        error_count: errors.len(),
        errors: errors
            .into_iter()
            .map(|e| ImportError { record_index: e.record_index, message: e.message })
            .collect(),
        error_groups,
        preview_articles,
    })
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
        reference_type: record.reference_type.clone(),
        date: record.date.clone(),
        author_address: record.author_address.clone(),
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
    }
}

#[tauri::command]
pub fn import_ris_file(
    db_state: State<'_, DbState>,
    request: ParseRisRequest,
) -> Result<ImportResult, AppError> {
    let content = if let Some(c) = request.content {
        c
    } else if let Some(p) = request.file_path {
        let metadata = std::fs::metadata(&p)
            .map_err(|e| AppError::Import(format!("Failed to read file metadata: {}", e)))?;
        if metadata.len() > MAX_RIS_FILE_SIZE {
            return Err(AppError::Import(format!(
                "File too large: {:.1} MB (maximum is {:.0} MB)",
                metadata.len() as f64 / (1024.0 * 1024.0),
                MAX_RIS_FILE_SIZE as f64 / (1024.0 * 1024.0)
            )));
        }
        std::fs::read_to_string(p)
            .map_err(|e| AppError::Import(format!("Failed to read file: {}", e)))?
    } else {
        return Err(AppError::Import("No content or file path provided".into()));
    };

    let parse_result = parse_ris(&content)?;
    let (valid, errors, error_groups) = validate_all_grouped(&parse_result.records);

    let excluded_set: std::collections::HashSet<usize> =
        request.excluded_indices.iter().copied().collect();

    // Filter out user-excluded articles (by valid-record index)
    let to_import: Vec<&RisRecord> = valid
        .iter()
        .enumerate()
        .filter(|(i, _)| !excluded_set.contains(i))
        .map(|(_, r)| r)
        .collect();

    let skipped_by_user = excluded_set.len();
    let skipped_validation = parse_result.records.len() - valid.len();

    if to_import.is_empty() {
        return Err(AppError::Import(
            "No articles to import. All records were either excluded or failed validation."
                .to_string(),
        ));
    }

    let new_articles: Vec<NewArticle> =
        to_import.iter().map(|r| ris_record_to_new_article(r)).collect();
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let imported = article_repo::insert_articles_batch(&conn, &new_articles, &request.file_name)?;

    // Classify: move non-duplicates to working, keep duplicates in 'duplicate'
    let _classification = classify_imported_articles(&conn, &imported)?;

    // Re-fetch articles to reflect updated statuses after classification
    let updated_articles: Vec<Article> = imported
        .iter()
        .filter_map(|a| article_repo::get_article_by_id(&conn, &a.id).ok())
        .collect();

    let remaining = article_repo::remaining_capacity(&conn)?;

    Ok(ImportResult {
        imported_count: updated_articles.len(),
        skipped_count: skipped_validation,
        skipped_by_user,
        articles: updated_articles,
        remaining_capacity: remaining,
        validation_errors: errors
            .into_iter()
            .map(|e| ImportError { record_index: e.record_index, message: e.message })
            .collect(),
        error_groups,
    })
}

#[tauri::command]
pub fn get_articles(db_state: State<'_, DbState>) -> Result<Vec<Article>, AppError> {
    let conn = db_state
        .conn
        .lock()
        .map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_all_articles(&conn)
}
