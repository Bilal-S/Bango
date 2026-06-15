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
#[must_use]
pub fn clean_wos_cr_line(line: &str) -> String {
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
#[must_use]
pub fn extract_doi(text: &str) -> Option<String> {
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
#[must_use]
pub fn looks_like_journal(text: &str) -> bool {
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
