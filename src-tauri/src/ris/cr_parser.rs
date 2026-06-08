//! Parser for Web of Science `CR` (Cited References) tag values.
//!
//! WoS CR format (from BibTeX exports):
//! `Author, Year, Journal/Title, [Vvol], [Ppage], [DOI ...].`
//!
//! Examples from actual WoS BibTeX exports:
//! - Standard: `Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.`
//! - DOI array: `Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/..., DOI 10.6073/PASTA/...].`
//! - Book: `{[}Anonymous], 1978, Canadian System of Soil Classification.`
//! - Minimal: `Barton Kamil, 2024, CRAN.`
//! - No year: `Melvin A. M, ECISYSTEMS, V18, P1472.`
//!
//! Key parsing rules:
//! - Each line is a separate reference ending with '.'
//! - `{[}` and `]` are BibTeX escapes for `[` and `]` (used in DOI arrays)
//! - `{*}` markers denote special formatting (stripped)
//! - Minimum required: author and (year or identifiable fields)
//! - 3rd field: ALL CAPS → journal, Mixed case → title/book

use crate::models::reference::NewReferencePaper;

/// Clean WoS BibTeX escaping artifacts from a CR line.
/// - `{[}` → `[`  (escaped bracket from WoS)
/// - `{*}` → removed (special formatting marker)
/// - Trailing `.` or `].` → removed
fn clean_wos_cr_line(line: &str) -> String {
    let mut s = line.trim().to_string();

    // Strip {*}{*}...{*}{*} markers (WoS special data object markers)
    // Replace with empty since these are formatting artifacts
    s = s.replace("{*}", "");

    // Replace {[} with [  (BibTeX bracket escaping)
    s = s.replace("{[}", "[");

    // Strip trailing '].' or '.' at end of line
    while s.ends_with('.') {
        s.pop();
    }
    s = s.trim_end().to_string();
    // Also strip trailing ']' that may remain after removing '.'
    // e.g., "DOI {[}xxx, DOI xxx]." → after clean → "DOI [xxx, DOI xxx]"
    // We keep the brackets since they may be meaningful for DOI arrays

    s
}

/// Extract the first DOI from a potentially bracketed DOI array.
/// Input examples:
/// - `DOI 10.1016/j.foreco.2017.04.005` → `10.1016/j.foreco.2017.04.005`
/// - `DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]`
///   → `10.6073/pasta/7367d64e999c830a508a7e012ad0824c`
/// - `10.1126/science.abf3903` → `10.1126/science.abf3903`
fn extract_doi(text: &str) -> Option<String> {
    let text = text.trim();

    // Strip "DOI " prefix(es) — WoS sometimes produces "DOI DOI 10.xxx"
    let mut text = text;
    while let Some(stripped) = text.strip_prefix("DOI ") {
        text = stripped.trim();
    }

    // Check if this is a DOI array: starts with '['
    if let Some(inner) = text.strip_prefix('[') {
        // Array like: [10.6073/pasta/7367..., DOI 10.6073/PASTA/7367...]
        // or after clean_wos: [10.6073/pasta/7367..., DOI 10.6073/PASTA/7367...]
        // Take the first entry, strip trailing ']'
        let first_entry = inner.split(',').next().unwrap_or(inner).trim();
        let first_entry = first_entry.strip_prefix("DOI ").unwrap_or(first_entry).trim();
        let first_entry = first_entry.trim_end_matches(']');
        if first_entry.starts_with("10.") && first_entry.contains('/') {
            return Some(first_entry.to_string());
        }
    }

    // Simple DOI: starts with "10." and contains '/'
    if text.starts_with("10.") && text.contains('/') {
        return Some(text.to_string());
    }

    None
}

/// Check if a string looks like a journal abbreviation (mostly uppercase, short).
fn looks_like_journal(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    // Journal abbreviations are typically ALL CAPS or mostly caps
    // e.g., "FOREST ECOL MANAG", "SCIENCE", "ECOSYSTEMS"
    let upper = text.chars().filter(|c| c.is_ascii_uppercase()).count();
    let alpha = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if alpha == 0 {
        return false;
    }
    // If >60% of alpha chars are uppercase, treat as journal abbreviation
    (upper as f64 / alpha as f64) > 0.6
}

/// Parse a single CR line into a NewReferencePaper.
/// Handles WoS BibTeX format with all its quirks.
/// Returns None if the line is too short to be meaningful.
pub fn parse_cr_line(cr_line: &str) -> Option<NewReferencePaper> {
    let cleaned = clean_wos_cr_line(cr_line);

    if cleaned.is_empty() {
        return None;
    }

    let parts: Vec<&str> = cleaned.split(',').map(|s| s.trim()).collect();

    // Need at least author
    if parts.is_empty() || parts[0].is_empty() {
        return None;
    }

    let mut paper = NewReferencePaper::default();

    // --- Part 0: Author ---
    let author_str = parts[0].trim();
    if author_str.is_empty() {
        return None;
    }
    // Clean up author: remove wrapping [] from {[}Anonymous] → [Anonymous]
    let author_clean = if author_str.starts_with('[') && author_str.ends_with(']') {
        // Keep the brackets — they're meaningful for "[Anonymous]"
        author_str.to_string()
    } else {
        author_str.to_string()
    };
    paper.authors = vec![author_clean.clone()];

    // --- Part 1: Year (if parseable as integer) ---
    let mut idx = 1;
    let _year_parsed = if idx < parts.len() {
        let maybe_year = parts[idx].trim();
        match maybe_year.parse::<i32>() {
            Ok(y) if (1000..=2100).contains(&y) => {
                paper.publication_year = Some(y);
                idx += 1;
                true
            }
            _ => false,
        }
    } else {
        false
    };

    // --- Part 2: Journal or Title/Book ---
    if idx < parts.len() {
        let field2 = parts[idx].trim();
        if !field2.is_empty() {
            if looks_like_journal(field2) {
                paper.journal = Some(field2.to_string());
            } else {
                // Mixed case → likely a book/report title
                // Use as title if we don't have one
                paper.title = Some(field2.to_string());
            }
        }
        idx += 1;
    }

    // --- Remaining parts: Volume, Pages, DOI ---
    for part in parts.iter().skip(idx) {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        // DOI: explicit "DOI ..." prefix or bare "10.xxx/yyy"
        if trimmed.starts_with("DOI ") {
            if paper.doi.is_none() {
                paper.doi = extract_doi(trimmed);
            }
            continue;
        }

        // Bare DOI: starts with "10." and contains '/'
        if trimmed.starts_with("10.") && trimmed.contains('/') {
            if paper.doi.is_none() {
                paper.doi = Some(trimmed.to_string());
            }
            continue;
        }

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

        // DOI in bracket array: starts with '[' (after cleaning)
        // e.g., "[10.1139/x03-183, 10.1139/X03-183]"
        if trimmed.starts_with('[') {
            if paper.doi.is_none() {
                paper.doi = extract_doi(trimmed);
            }
            continue;
        }

        // Fallback: if it looks like a DOI (contains "10." and "/")
        if trimmed.contains("10.") && trimmed.contains('/') && paper.doi.is_none() {
            paper.doi = extract_doi(trimmed);
            continue;
        }

        // If nothing matched and it looks like content, try as title
        // (only if we already have a journal set, this might be a subtitle)
        if paper.journal.is_some() && paper.title.is_none() && !trimmed.is_empty() {
            paper.title = Some(trimmed.to_string());
        }
    }

    // --- Build descriptive title if none was parsed ---
    // CR lines from WoS don't include article titles, only journal abbreviations.
    // We construct a descriptive reference string for display.
    if paper.title.is_none() {
        let year_str = paper.publication_year.map_or_else(|| "n/a".to_string(), |y| y.to_string());
        let source = paper.journal.as_deref().unwrap_or("unknown");
        paper.title = Some(format!("{}, {} ({})", author_clean, source, year_str));
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

    // ---- clean_wos_cr_line tests ----

    #[test]
    fn test_clean_standard_entry() {
        assert_eq!(
            clean_wos_cr_line("Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005."),
            "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005"
        );
    }

    #[test]
    fn test_clean_doi_array_entry() {
        assert_eq!(
            clean_wos_cr_line("Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]."),
            "Alexander H. D., 2021, DATA OBJECT, DOI [10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]"
        );
    }

    #[test]
    fn test_clean_anonymous_bracket() {
        assert_eq!(
            clean_wos_cr_line("{[}Anonymous], 1978, Canadian System of Soil Classification."),
            "[Anonymous], 1978, Canadian System of Soil Classification"
        );
    }

    // ---- extract_doi tests ----

    #[test]
    fn test_extract_doi_simple() {
        assert_eq!(
            extract_doi("10.1016/j.foreco.2017.04.005"),
            Some("10.1016/j.foreco.2017.04.005".to_string())
        );
    }

    #[test]
    fn test_extract_doi_with_prefix() {
        assert_eq!(
            extract_doi("DOI 10.1016/j.foreco.2017.04.005"),
            Some("10.1016/j.foreco.2017.04.005".to_string())
        );
    }

    #[test]
    fn test_extract_doi_array() {
        assert_eq!(
            extract_doi("DOI [10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]"),
            Some("10.6073/pasta/7367d64e999c830a508a7e012ad0824c".to_string())
        );
    }

    #[test]
    fn test_extract_doi_bare_array() {
        assert_eq!(
            extract_doi("[10.1139/x03-183, 10.1139/X03-183]"),
            Some("10.1139/x03-183".to_string())
        );
    }

    #[test]
    fn test_extract_doi_not_a_doi() {
        assert_eq!(extract_doi("V396"), None);
    }

    // ---- looks_like_journal tests ----

    #[test]
    fn test_journal_all_caps() {
        assert!(looks_like_journal("FOREST ECOL MANAG"));
        assert!(looks_like_journal("SCIENCE"));
        assert!(looks_like_journal("NATURE"));
    }

    #[test]
    fn test_not_journal_mixed_case() {
        assert!(!looks_like_journal("Canadian System of Soil Classification"));
        assert!(!looks_like_journal("A key for predicting postfire successional trajectories"));
    }

    // ---- parse_cr_line tests: real WoS patterns ----

    #[test]
    fn parse_standard_entry() {
        let line =
            "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Alexander HD"]);
        assert_eq!(paper.publication_year, Some(2017));
        assert_eq!(paper.journal.as_deref(), Some("FOREST ECOL MANAG"));
        assert_eq!(paper.volume.as_deref(), Some("396"));
        assert_eq!(paper.start_page.as_deref(), Some("35"));
        assert_eq!(paper.doi.as_deref(), Some("10.1016/j.foreco.2017.04.005"));
        // Title should be constructed since CR lines don't have article titles
        assert!(paper.title.is_some());
        assert!(paper.title.as_ref().unwrap().contains("Alexander HD"));
    }

    #[test]
    fn parse_doi_array_entry() {
        let line = "Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C].";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Alexander H. D."]);
        assert_eq!(paper.publication_year, Some(2021));
        // {*}{*}DATA OBJECT{*}{*} → DATA OBJECT (ALL CAPS → treated as journal)
        assert_eq!(paper.journal.as_deref(), Some("DATA OBJECT"));
        assert!(paper.title.is_some()); // auto-constructed descriptive title
                                        // First DOI from the array
        assert_eq!(paper.doi.as_deref(), Some("10.6073/pasta/7367d64e999c830a508a7e012ad0824c"));
    }

    #[test]
    fn parse_anonymous_book() {
        let line = "{[}Anonymous], 1978, Canadian System of Soil Classification.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["[Anonymous]"]);
        assert_eq!(paper.publication_year, Some(1978));
        // Mixed case → title (it's a book title, not a journal)
        assert_eq!(paper.title.as_deref(), Some("Canadian System of Soil Classification"));
        assert!(paper.journal.is_none());
    }

    #[test]
    fn parse_minimal_entry() {
        let line = "Barton Kamil, 2024, CRAN.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Barton Kamil"]);
        assert_eq!(paper.publication_year, Some(2024));
        // "CRAN" is short but all caps → journal
        assert_eq!(paper.journal.as_deref(), Some("CRAN"));
    }

    #[test]
    fn parse_no_doi_entry() {
        let line = "Johnson E. A, 1992, FIRE VEGETATION DYNA.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Johnson E. A"]);
        assert_eq!(paper.publication_year, Some(1992));
        assert_eq!(paper.journal.as_deref(), Some("FIRE VEGETATION DYNA"));
        assert!(paper.doi.is_none());
    }

    #[test]
    fn parse_doi_array_bracket_form() {
        let line = "Johnstone JF, 2004, CAN J FOREST RES, V34, P267, DOI {[}10.1139/x03-183, 10.1139/X03-183].";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Johnstone JF"]);
        assert_eq!(paper.publication_year, Some(2004));
        assert_eq!(paper.journal.as_deref(), Some("CAN J FOREST RES"));
        assert_eq!(paper.volume.as_deref(), Some("34"));
        assert_eq!(paper.start_page.as_deref(), Some("267"));
        assert_eq!(paper.doi.as_deref(), Some("10.1139/x03-183"));
    }

    #[test]
    fn parse_entry_with_doi_prefix() {
        let line = "Fenner M., 2005, The Ecology of Seeds, DOI DOI 10.1017/CBO9780511614101.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Fenner M."]);
        assert_eq!(paper.publication_year, Some(2005));
        // Mixed case → title (it's a book)
        assert_eq!(paper.title.as_deref(), Some("The Ecology of Seeds"));
        // "DOI DOI 10.1017/..." → extract handles double DOI prefix
        assert!(paper.doi.is_some());
        assert!(paper.doi.as_ref().unwrap().starts_with("10."));
    }

    #[test]
    fn parse_complex_doi_with_parens() {
        let line = "Osterkamp TE, 1999, PERMAFROST PERIGLAC, V10, P17, DOI 10.1002/(SICI)1099-1530(199901/03)10:1<17::AID-PPP303>3.0.CO;2-4.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Osterkamp TE"]);
        assert_eq!(paper.publication_year, Some(1999));
        assert_eq!(paper.journal.as_deref(), Some("PERMAFROST PERIGLAC"));
        assert!(paper.doi.is_some());
        assert!(paper.doi.as_ref().unwrap().starts_with("10.1002/"));
    }

    #[test]
    fn parse_entry_without_year() {
        // Some entries lack a year: "Melvin A. M, ECISYSTEMS, V18, P1472."
        let line = "Melvin A. M, ECISYSTEMS, V18, P1472.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Melvin A. M"]);
        // Year not parseable → stays None
        assert!(paper.publication_year.is_none());
        // ECISYSTEMS looks like journal (mostly caps)
        assert!(paper.journal.is_some());
    }

    #[test]
    fn parse_ahrens_book_reference() {
        let line = "Ahrens RJ, 2004, CRYOSOLS: PERMAFROST-AFFECTED SOILS, P627.";
        let paper = parse_cr_line(line).unwrap();
        assert_eq!(paper.authors, vec!["Ahrens RJ"]);
        assert_eq!(paper.publication_year, Some(2004));
        // ALL CAPS → journal (book title that's in all caps)
        assert_eq!(paper.journal.as_deref(), Some("CRYOSOLS: PERMAFROST-AFFECTED SOILS"));
        assert_eq!(paper.start_page.as_deref(), Some("627"));
    }

    #[test]
    fn parse_full_cr_line_standard() {
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
    fn parse_too_short_cr_line() {
        assert!(parse_cr_line("").is_none());
        assert!(parse_cr_line("   ").is_none());
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

    #[test]
    fn parse_cr_entries_from_real_bibtex_patterns() {
        let extras = serde_json::json!({
            "CR": [
                "Ahrens RJ, 2004, CRYOSOLS: PERMAFROST-AFFECTED SOILS, P627.",
                "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.",
                "{[}Anonymous], 1978, Canadian System of Soil Classification.",
                "Barton Kamil, 2024, CRAN.",
                "Johnstone JF, 2004, CAN J FOREST RES, V34, P267, DOI {[}10.1139/x03-183, 10.1139/X03-183].",
                "Osterkamp TE, 1999, PERMAFROST PERIGLAC, V10, P17, DOI 10.1002/(SICI)1099-1530(199901/03)10:1<17::AID-PPP303>3.0.CO;2-4."
            ]
        });
        let papers = parse_cr_entries(&extras);
        assert_eq!(papers.len(), 6, "Should parse all 6 reference patterns");

        // Ahrens — book with page
        assert_eq!(papers[0].authors, vec!["Ahrens RJ"]);
        assert_eq!(papers[0].publication_year, Some(2004));

        // Alexander — standard journal entry with DOI
        assert_eq!(papers[1].doi.as_deref(), Some("10.1016/j.foreco.2017.04.005"));
        assert_eq!(papers[1].volume.as_deref(), Some("396"));

        // Anonymous — book title
        assert_eq!(papers[2].authors, vec!["[Anonymous]"]);
        assert_eq!(papers[2].title.as_deref(), Some("Canadian System of Soil Classification"));

        // Barton — minimal entry
        assert_eq!(papers[3].publication_year, Some(2024));

        // Johnstone — DOI array
        assert_eq!(papers[4].doi.as_deref(), Some("10.1139/x03-183"));

        // Osterkamp — complex DOI
        assert!(papers[5].doi.as_ref().unwrap().starts_with("10.1002/"));
    }
}
