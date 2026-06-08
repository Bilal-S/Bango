use super::types::{RisParseError, RisParseResult, RisRecord};
use crate::error::AppError;
use crate::ris::n1_parser::parse_n1_citation_data;

/// Parses a complete RIS file content into records.
/// Records are delimited by `ER` tags.
pub fn parse_ris(content: &str) -> Result<RisParseResult, AppError> {
    // Strip BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    // Normalize line endings: \r\n -> \n, then standalone \r -> \n
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

    let mut records = Vec::new();
    let mut errors = Vec::new();
    let mut current = RisRecord::default();
    let mut record_index = 0;
    let mut in_record = false;
    let mut last_tag: Option<String> = None;

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if this line starts with a valid RIS tag pattern (e.g., "XX  - ")
        let has_tag = is_ris_tag_line(trimmed);

        if !has_tag {
            // This is a continuation line — append to the last tag's value
            if let Some(ref tag) = last_tag {
                if in_record {
                    append_continuation(tag, trimmed, &mut current);
                }
            }
            continue;
        }

        // Extract all tag-value pairs from the line.
        // RIS files sometimes concatenate multiple tags on one line
        // (e.g., "AD  - valueC3  - value").
        let pairs = parse_line_tags(trimmed);

        for (tag, value) in pairs {
            if tag == "TY" {
                in_record = true;
            }

            if tag == "ER" {
                if in_record {
                    // Before finalizing, extract N1 citation data from the full notes
                    finalize_n1(&mut current);
                    records.push(current);
                    current = RisRecord::default();
                    record_index += 1;
                    in_record = false;
                    last_tag = None;
                }
                continue;
            }

            if !in_record {
                // Skip tags outside of a record (before first TY)
                continue;
            }

            apply_tag(tag, value, &mut current);
            last_tag = Some(tag.to_string());
        }
    }

    // If file doesn't end with ER, collect the last record
    if in_record {
        record_index += 1;
        errors.push(RisParseError {
            record_index,
            message: "Record missing ER (end of reference) tag".to_string(),
        });
        finalize_n1(&mut current);
        records.push(current);
    }

    Ok(RisParseResult { records, errors })
}

/// Checks if a trimmed line starts with a valid RIS tag pattern ("XX  -" or "XX  - ").
/// RIS tags are two characters: first is an uppercase letter, second is uppercase or digit
/// (e.g., "N1", "T2", "C3", "J9").
fn is_ris_tag_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 5
        && bytes[0].is_ascii_uppercase()
        && (bytes[1].is_ascii_uppercase() || bytes[1].is_ascii_digit())
        && bytes[2] == b' '
        && bytes[3] == b' '
        && bytes[4] == b'-'
}

/// Extracts N1 citation data (num_cited, num_references) from the assembled notes field
/// after all multi-line continuations have been applied.
fn finalize_n1(record: &mut RisRecord) {
    if let Some(ref notes) = record.notes {
        let (num_cited, num_references) = parse_n1_citation_data(notes);
        record.num_cited = num_cited;
        record.num_references = num_references;
    }
}

/// Appends a continuation line to the appropriate field for the given tag.
fn append_continuation(tag: &str, text: &str, record: &mut RisRecord) {
    match tag {
        "TI" | "T1" => append_option(&mut record.title, text),
        "AB" => append_option(&mut record.abstract_text, text),
        "N2" => append_option(&mut record.abstract_text, text),
        "N1" => append_option(&mut record.notes, text),
        "AD" => append_option(&mut record.author_address, text),
        "DA" => append_option(&mut record.date, text),
        "UR" => append_option(&mut record.url, text),
        "LA" => append_option(&mut record.language, text),
        "PB" => append_option(&mut record.publisher, text),
        "PU" => append_option(&mut record.publisher, text),
        "PA" => append_option(&mut record.publisher_address, text),
        "PI" => append_option(&mut record.publisher_city, text),
        "SN" => append_option(&mut record.issn, text),
        "AN" => append_option(&mut record.accession_number, text),
        "C3" => append_option(&mut record.custom_field3, text),
        "J9" => append_option(&mut record.journal_abbreviation, text),
        "JI" => append_option(&mut record.journal_iso_abbreviation, text),
        "WE" => append_option(&mut record.web_of_science_db, text),
        "T2" => append_option(&mut record.journal, text),
        "JO" => append_option(&mut record.journal, text),
        "VL" => append_option(&mut record.volume, text),
        "IS" => append_option(&mut record.issue, text),
        "SP" => append_option(&mut record.start_page, text),
        "EP" => append_option(&mut record.end_page, text),
        "DO" => append_option(&mut record.doi, text),
        "M3" => append_option(&mut record.reference_type, text),
        "TY" => append_option(&mut record.reference_type, text),
        "AU" => append_last_vec(&mut record.authors, text),
        "KW" => append_last_vec(&mut record.keywords, text),
        _ => {
            // For extras, append to the last value of the tag's vector
            if let Some(values) = record.extras.get_mut(tag) {
                if let Some(last) = values.last_mut() {
                    last.push('\n');
                    last.push_str(text);
                }
            }
        }
    }
}

/// Appends text (with newline separator) to an Option<String>.
fn append_option(opt: &mut Option<String>, text: &str) {
    if let Some(ref mut existing) = opt {
        existing.push('\n');
        existing.push_str(text);
    }
}

/// Appends text (with newline separator) to the last entry in a Vec<String>.
fn append_last_vec(vec: &mut [String], text: &str) {
    if let Some(last) = vec.last_mut() {
        last.push('\n');
        last.push_str(text);
    }
}

/// Extracts all RIS tag-value pairs from a single line.
/// A line may contain concatenated tags (e.g., "AD  - fooC3  - bar").
/// Returns a vector of (tag, value) pairs.
fn parse_line_tags(line: &str) -> Vec<(&str, &str)> {
    let mut pairs = Vec::new();
    let mut remaining = line;

    loop {
        if remaining.len() < 5 {
            break;
        }

        let tag = &remaining[..2];
        let rest = &remaining[2..];

        // Accept "  - " (4 chars) or "  -" (3 chars, e.g., "ER  -")
        let value_and_rest = if let Some(stripped) = rest.strip_prefix("  - ") {
            stripped
        } else if let Some(stripped) = rest.strip_prefix("  -") {
            stripped
        } else {
            // Not a valid tag separator; try to find the next tag start.
            if let Some(pos) = find_next_tag_start(remaining) {
                remaining = &remaining[pos..];
                continue;
            }
            break;
        };

        // Find where the next tag starts within the value portion
        let (value, leftover) = if let Some(pos) = find_next_tag_start(value_and_rest) {
            (&value_and_rest[..pos], &value_and_rest[pos..])
        } else {
            (value_and_rest, "")
        };

        pairs.push((tag, value.trim()));
        remaining = leftover;

        if remaining.is_empty() {
            break;
        }
    }

    pairs
}

/// Finds the position of the next RIS tag pattern ("XX  - ") in the string.
/// Returns the byte offset or None.
fn find_next_tag_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 6 {
        return None;
    }

    (1..=len.saturating_sub(6)).find(|&i| {
        bytes[i].is_ascii_uppercase()
            && bytes[i + 1].is_ascii_uppercase()
            && bytes[i + 2] == b' '
            && bytes[i + 3] == b' '
            && bytes[i + 4] == b'-'
            && bytes[i + 5] == b' '
    })
}

/// Applies a single RIS tag-value pair to a record.
fn apply_tag(tag: &str, value: &str, record: &mut RisRecord) {
    match tag {
        "TY" => record.reference_type = Some(value.to_string()),
        "TI" | "T1" => record.title = Some(value.to_string()),
        "AB" => record.abstract_text = Some(value.to_string()),
        "AU" => record.authors.push(value.to_string()),
        "PY" => {
            // PY can be "2023" or "2023/12/31/" - extract year
            let year_str = value.split('/').next().unwrap_or(value);
            record.publication_year = year_str.parse().ok();
        }
        "DO" => record.doi = Some(value.to_string()),
        "T2" => record.journal = Some(value.to_string()),
        "VL" => record.volume = Some(value.to_string()),
        "IS" => record.issue = Some(value.to_string()),
        "SP" => record.start_page = Some(value.to_string()),
        "EP" => record.end_page = Some(value.to_string()),
        "C7" => {
            // C7 is article number / start page in some exports
            if record.start_page.is_none() {
                record.start_page = Some(value.to_string());
            }
        }
        "KW" => record.keywords.push(value.to_string()),
        "UR" => record.url = Some(value.to_string()),
        "LA" => record.language = Some(value.to_string()),
        "PB" => record.publisher = Some(value.to_string()),
        "PU" => {
            if record.publisher.is_none() {
                record.publisher = Some(value.to_string());
            }
        }
        "SN" => {
            // Keep the first ISSN found
            if record.issn.is_none() {
                record.issn = Some(value.to_string());
            }
        }
        "M3" => {
            if record.reference_type.is_none() {
                record.reference_type = Some(value.to_string());
            }
        }
        "N2" => {
            if record.abstract_text.is_none() {
                record.abstract_text = Some(value.to_string());
            }
        }
        "JO" => {
            if record.journal.is_none() {
                record.journal = Some(value.to_string());
            }
        }
        "DA" => record.date = Some(value.to_string()),
        "AD" => record.author_address = Some(value.to_string()),
        "AN" => record.accession_number = Some(value.to_string()),
        "C3" => record.custom_field3 = Some(value.to_string()),
        "J9" => record.journal_abbreviation = Some(value.to_string()),
        "JI" => record.journal_iso_abbreviation = Some(value.to_string()),
        "N1" => {
            // Just store the value; citation data is extracted at record finalization
            record.notes = Some(value.to_string());
        }
        "PA" => record.publisher_address = Some(value.to_string()),
        "PI" => record.publisher_city = Some(value.to_string()),
        "WE" => record.web_of_science_db = Some(value.to_string()),
        "ER" => { /* handled by caller */ }
        _ => {
            record.extras.entry(tag.to_string()).or_default().push(value.to_string());
        }
    }
}
