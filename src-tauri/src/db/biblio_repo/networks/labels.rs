/// Format a short paper label from an authors JSON string and optional year.
///
/// Produces `FirstAuthor (Year)` when there is a single author and
/// `FirstAuthor et al. (Year)` when there are multiple authors. When the year
/// is missing the parentheses are omitted entirely.
#[must_use]
pub fn format_paper_label(authors_str: &str, year: Option<i32>) -> String {
    let parsed = crate::biblio::normalizer::parse_authors(authors_str);
    let year_suffix = match year {
        Some(y) => format!(" ({})", y),
        None => String::new(),
    };
    if parsed.is_empty() {
        return format!("Unknown{}", year_suffix);
    }
    // Use the surname portion of the display name ("Last, First" → "Last").
    let first_author = parsed
        .first()
        .map(|a| a.display_name.split(',').next().unwrap_or(&a.display_name).trim())
        .unwrap_or("Unknown");
    if parsed.len() == 1 {
        format!("{}{}", first_author, year_suffix)
    } else {
        format!("{} et al.{}", first_author, year_suffix)
    }
}
