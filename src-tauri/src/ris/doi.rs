/// Placeholder DOI values that should be treated as missing.
/// These are commonly emitted by Citation Chaser and other tools.
const DOI_PLACEHOLDERS: &[&str] = &["NA", "N/A", "NULL", "NONE", "-"];

/// Normalize a DOI value. Returns `None` for empty or placeholder values.
///
/// # Examples
/// ```ignore
/// use crate::ris::doi::normalize_doi;
/// assert_eq!(normalize_doi(Some("10.1016/j.jand.2021.06.013")), Some("10.1016/j.jand.2021.06.013"));
/// assert_eq!(normalize_doi(Some("NA")), None);
/// assert_eq!(normalize_doi(None), None);
/// ```
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
