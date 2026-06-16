use super::parser::parse_ris;
use super::types::{RisParseError, RisRecord};
use super::validator::{validate_all_grouped, ErrorGroup};
use crate::error::AppError;

/// Maximum RIS file size: 100 MB. Prevents OOM from accidentally importing huge files.
pub const MAX_RIS_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Generic preview of a parsed record, used for both article and reference previews.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRecord {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
}

impl PreviewRecord {
    pub fn from_ris_record(record: &RisRecord) -> Self {
        Self {
            title: record.title.clone(),
            authors: record.authors.clone(),
            publication_year: record.publication_year,
            journal: record.journal.clone(),
            doi: record.doi.clone(),
        }
    }

    /// Flattened version used by article preview (title is required, default to empty).
    pub fn into_article_preview(self) -> PreviewArticleFlat {
        PreviewArticleFlat {
            title: self.title.unwrap_or_default(),
            authors: self.authors,
            publication_year: self.publication_year,
            journal: self.journal,
            doi: self.doi,
        }
    }
}

/// Flat preview record where title is guaranteed non-None (for article import UI).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewArticleFlat {
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
}

/// A validation error associated with a specific record.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportValidationError {
    pub record_index: usize,
    pub message: String,
}

impl From<RisParseError> for ImportValidationError {
    fn from(e: RisParseError) -> Self {
        Self { record_index: e.record_index, message: e.message }
    }
}

/// Validation mode: strict (articles require title+abstract+authors),
/// lenient (references only warn, don't block import), or none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidationMode {
    /// Full validation - records without title/abstract/authors are excluded.
    Strict,
    /// No validation - all records are accepted (used for reference imports).
    None,
}

/// Result of the parse-and-validate phase.
#[derive(Debug, Clone)]
pub struct ParseOutput {
    /// All records that passed validation (or all records if ValidationMode::None).
    pub valid_records: Vec<RisRecord>,
    /// Original total count before validation.
    pub total_records: usize,
    /// Validation errors (if Strict mode).
    pub errors: Vec<ImportValidationError>,
    /// Errors grouped by message for summarised UI display.
    pub error_groups: Vec<ErrorGroup>,
}

/// Read file content from a string or file path, with size limit.
pub fn read_content(
    content: Option<String>,
    file_path: Option<String>,
) -> Result<String, AppError> {
    if let Some(c) = content {
        Ok(c)
    } else if let Some(p) = file_path {
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
            .map_err(|e| AppError::Import(format!("Failed to read file: {}", e)))
    } else {
        Err(AppError::Import("No content or file path provided".into()))
    }
}

/// Parse RIS content and apply validation.
pub fn parse_and_validate(content: &str, mode: ValidationMode) -> Result<ParseOutput, AppError> {
    let parse_result = parse_ris(content)?;
    let total = parse_result.records.len();

    match mode {
        ValidationMode::Strict => {
            let (valid, errors, error_groups) = validate_all_grouped(&parse_result.records);
            Ok(ParseOutput {
                valid_records: valid,
                total_records: total,
                errors: errors.into_iter().map(ImportValidationError::from).collect(),
                error_groups,
            })
        }
        ValidationMode::None => Ok(ParseOutput {
            valid_records: parse_result.records,
            total_records: total,
            errors: vec![],
            error_groups: vec![],
        }),
    }
}

/// Build a list of preview records from validated RIS records, limited to `max_preview`.
pub fn build_preview_records(records: &[RisRecord], max_preview: usize) -> Vec<PreviewRecord> {
    records.iter().take(max_preview).map(PreviewRecord::from_ris_record).collect()
}

/// Parse and validate already-converted RisRecord list (e.g., from BibTeX conversion).
pub fn parse_and_validate_from_records(
    records: &[RisRecord],
    mode: ValidationMode,
) -> Result<ParseOutput, AppError> {
    let total = records.len();
    match mode {
        ValidationMode::Strict => {
            let (valid, errors, error_groups) = validate_all_grouped(records);
            Ok(ParseOutput {
                valid_records: valid,
                total_records: total,
                errors: errors.into_iter().map(ImportValidationError::from).collect(),
                error_groups,
            })
        }
        ValidationMode::None => Ok(ParseOutput {
            valid_records: records.to_vec(),
            total_records: total,
            errors: vec![],
            error_groups: vec![],
        }),
    }
}

/// Filter out user-excluded records (by valid-record index).
/// Returns the records to import and the count of excluded records.
pub fn filter_excluded<'a>(
    records: &'a [RisRecord],
    excluded_indices: &[usize],
) -> (Vec<&'a RisRecord>, usize) {
    let excluded_set: std::collections::HashSet<usize> = excluded_indices.iter().copied().collect();
    let filtered: Vec<&RisRecord> = records
        .iter()
        .enumerate()
        .filter(|(i, _)| !excluded_set.contains(i))
        .map(|(_, r)| r)
        .collect();
    let skipped = excluded_set.len();
    (filtered, skipped)
}
