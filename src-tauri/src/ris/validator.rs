use std::collections::HashMap;

use super::types::{RisParseError, RisRecord};

/// Validation errors sharing the same message, for summarised UI display
/// (e.g. "7 records missing Abstract").
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorGroup {
    pub message: String,
    pub count: usize,
    pub record_indices: Vec<usize>,
}

/// Checks a record for required fields (title, abstract, authors).
pub fn validate_record(record: &RisRecord, record_index: usize) -> Vec<RisParseError> {
    let mut errors = Vec::new();

    if record.title.as_ref().is_none_or(|t| t.trim().is_empty()) {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: Title (TI or T1)".to_string(),
        });
    }

    if record.abstract_text.as_ref().is_none_or(|a| a.trim().is_empty()) {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: Abstract (AB or N2)".to_string(),
        });
    }

    if record.authors.is_empty() {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: at least one Author (AU)".to_string(),
        });
    }

    errors
}

/// Filters records, returning valid ones and all errors.
pub fn validate_all(records: &[RisRecord]) -> (Vec<RisRecord>, Vec<RisParseError>) {
    let mut valid = Vec::new();
    let mut all_errors = Vec::new();

    for (i, record) in records.iter().enumerate() {
        let errors = validate_record(record, i + 1);
        if errors.is_empty() {
            valid.push(record.clone());
        } else {
            all_errors.extend(errors);
        }
    }

    (valid, all_errors)
}

/// Like [`validate_all`] but groups errors by message for summarised display.
pub fn validate_all_grouped(
    records: &[RisRecord],
) -> (Vec<RisRecord>, Vec<RisParseError>, Vec<ErrorGroup>) {
    let (valid, all_errors) = validate_all(records);

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for err in &all_errors {
        groups.entry(err.message.clone()).or_default().push(err.record_index);
    }

    let error_groups: Vec<ErrorGroup> = groups
        .into_iter()
        .map(|(message, indices)| ErrorGroup {
            count: indices.len(),
            record_indices: indices,
            message,
        })
        .collect();

    (valid, all_errors, error_groups)
}
