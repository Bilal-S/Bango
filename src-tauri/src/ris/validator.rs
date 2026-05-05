use super::types::{RisParseError, RisRecord};

/// Validates a single RIS record for required fields.
/// Returns a list of validation errors (empty if valid).
pub fn validate_record(record: &RisRecord, record_index: usize) -> Vec<RisParseError> {
    let mut errors = Vec::new();

    if record.title.as_ref().is_none_or(|t| t.trim().is_empty()) {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: Title (TI)".to_string(),
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

/// Validates all records in a parse result, returning only valid records
/// and collecting all validation errors.
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
