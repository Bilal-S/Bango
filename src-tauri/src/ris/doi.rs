/// Placeholder DOIs that should be treated as missing (commonly emitted by
/// Citation Chaser and other tools).
const DOI_PLACEHOLDERS: &[&str] = &["NA", "N/A", "NULL", "NONE", "-"];

/// Returns `None` for empty, whitespace, or known placeholder values.
#[must_use]
pub fn normalize_doi(doi: Option<&str>) -> Option<&str> {
    doi.and_then(|d| {
        let trimmed = d.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Check against known placeholders (case-insensitive)
        for placeholder in DOI_PLACEHOLDERS {
            if trimmed.eq_ignore_ascii_case(placeholder) {
                return None;
            }
        }
        Some(trimmed)
    })
}
