//! Parser for Web of Science `CR` (Cited References) tag values.
//!
//! WoS CR format: `Author, Year, Journal, V, P, DOI`
//! Example: `Smith J, 2020, NATURE, V581, P364, 10.1038/s41586-020-2012-7`
//!
//! The fields are comma-separated and may be partially present.

use crate::models::reference::NewReferencePaper;

/// Parse a single CR line into a NewReferencePaper.
/// Returns None if the line is too short to be meaningful.
pub fn parse_cr_line(cr_line: &str) -> Option<NewReferencePaper> {
    let parts: Vec<&str> = cr_line.split(',').map(|s| s.trim()).collect();

    // Need at least an author and year
    if parts.len() < 2 {
        return None;
    }

    let mut paper = NewReferencePaper::default();

    // Author(s)
    let author_str = parts.first()?.trim();
    if !author_str.is_empty() {
        // CR lines typically have a single author "LastName AB"
        paper.authors = vec![author_str.to_string()];
    }

    // Year
    if let Some(year_str) = parts.get(1) {
        paper.publication_year = year_str.trim().parse().ok();
    }

    // Journal (3rd field)
    if let Some(journal_str) = parts.get(2) {
        let journal = journal_str.trim().to_string();
        if !journal.is_empty() {
            paper.journal = Some(journal);
        }
    }

    // Parse remaining fields looking for volume, pages, DOI
    for part in parts.iter().skip(3) {
        let trimmed = part.trim();

        // Volume: "V123" or "V 123"
        if let Some(vol) = trimmed.strip_prefix('V') {
            let vol = vol.trim();
            if !vol.is_empty() && paper.volume.is_none() {
                paper.volume = Some(vol.to_string());
                continue;
            }
        }

        // Start page: "P123" or "P 123"
        if let Some(page) = trimmed.strip_prefix('P') {
            let page = page.trim();
            if !page.is_empty() && paper.start_page.is_none() {
                paper.start_page = Some(page.to_string());
                continue;
            }
        }

        // DOI: starts with "10." or contains a DOI pattern
        if trimmed.starts_with("10.") && trimmed.contains('/') {
            paper.doi = Some(trimmed.to_string());
            continue;
        }

        // If nothing matched and it looks like a title, use it
        if paper.title.is_none()
            && !trimmed.is_empty()
            && !trimmed.starts_with('V')
            && !trimmed.starts_with('P')
        {
            paper.title = Some(trimmed.to_string());
        }
    }

    // Build a title from available info if none was parsed
    if paper.title.is_none() {
        let year = paper.publication_year.map_or_else(|| "n/a".into(), |y| y.to_string());
        let journal = paper.journal.as_deref().unwrap_or("unknown");
        paper.title = Some(format!("{}, {} ({})", author_str, journal, year));
    }

    Some(paper)
}

/// Parse all CR entries from the extras map.
/// The `extras` map may contain `"CR" -> Vec<String>` of cited reference lines.
/// Returns the list of successfully parsed papers.
pub fn parse_cr_entries(extras: &serde_json::Value) -> Vec<NewReferencePaper> {
    let Some(cr_lines) = extras.get("CR") else {
        return vec![];
    };

    // CR can be a single string or an array of strings
    let lines: Vec<String> = if let Some(arr) = cr_lines.as_array() {
        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
    } else if let Some(s) = cr_lines.as_str() {
        vec![s.to_string()]
    } else {
        return vec![];
    };

    lines.iter().filter_map(|line| parse_cr_line(line)).collect()
}

/// Parse CR entries from a RIS extras JSON string.
pub fn parse_cr_from_extras_json(extras_json: Option<&str>) -> Vec<NewReferencePaper> {
    let extras = extras_json.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    match extras {
        Some(v) => parse_cr_entries(&v),
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_cr_line() {
        let line = "Smith J, 2020, NATURE, V581, P364, 10.1038/s41586-020-2012-7";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Smith J"]);
        assert_eq!(paper.publication_year, Some(2020));
        assert_eq!(paper.journal.as_deref(), Some("NATURE"));
        assert_eq!(paper.volume.as_deref(), Some("581"));
        assert_eq!(paper.start_page.as_deref(), Some("364"));
        assert_eq!(paper.doi.as_deref(), Some("10.1038/s41586-020-2012-7"));
        assert!(paper.title.is_some());
    }

    #[test]
    fn parse_minimal_cr_line() {
        let line = "Doe A, 2019, SCIENCE";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Doe A"]);
        assert_eq!(paper.publication_year, Some(2019));
        assert_eq!(paper.journal.as_deref(), Some("SCIENCE"));
    }

    #[test]
    fn parse_cr_line_without_doi() {
        let line = "Brown K, 2018, CELL, V175, P1024";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.volume.as_deref(), Some("175"));
        assert_eq!(paper.start_page.as_deref(), Some("1024"));
        assert!(paper.doi.is_none());
    }

    #[test]
    fn parse_too_short_cr_line() {
        let line = "JustOneField";
        assert!(parse_cr_line(line).is_none());
    }

    #[test]
    fn parse_empty_cr_line() {
        assert!(parse_cr_line("").is_none());
    }

    #[test]
    fn parse_cr_entries_from_extras() {
        let extras = serde_json::json!({
            "CR": [
                "Smith J, 2020, NATURE, V581, P364",
                "Doe A, 2019, SCIENCE"
            ]
        });
        let papers = parse_cr_entries(&extras);
        assert_eq!(papers.len(), 2);
        assert_eq!(papers[0].authors, vec!["Smith J"]);
        assert_eq!(papers[1].authors, vec!["Doe A"]);
    }

    #[test]
    fn parse_cr_entries_no_cr_field() {
        let extras = serde_json::json!({"AU": ["someone"]});
        let papers = parse_cr_entries(&extras);
        assert!(papers.is_empty());
    }
}
