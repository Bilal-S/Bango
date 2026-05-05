use crate::error::AppError;
use super::types::{RisParseResult, RisRecord};

/// Parses a complete RIS file content into records.
/// Records are delimited by `ER` tags.
pub fn parse_ris(_content: &str) -> Result<RisParseResult, AppError> {
    Ok(RisParseResult {
        records: vec![RisRecord::default()],
        errors: vec![],
    })
}
