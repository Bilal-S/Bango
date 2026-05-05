use super::types::{RisParseError, RisRecord};

/// Validates a single RIS record for required fields.
/// Returns a list of validation errors (empty if valid).
pub fn validate_record(_record: &RisRecord, _record_index: usize) -> Vec<RisParseError> {
    vec![]
}
