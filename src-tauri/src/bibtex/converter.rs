use std::collections::HashMap;

use crate::biblio::normalizer::{split_authors, split_keywords};
use crate::ris::types::RisRecord;

use super::parser::BibtexEntry;

/// Maps a BibTeX entry type to an RIS-style reference type string.
fn map_entry_type(entry_type: &str) -> String {
    match entry_type {
        "article" => "article".to_string(),
        "book" => "book".to_string(),
        "inbook" => "book".to_string(),
        "incollection" => "incollection".to_string(),
        "inproceedings" | "conference" => "conference".to_string(),
        "phdthesis" => "phdthesis".to_string(),
        "mastersthesis" => "mastersthesis".to_string(),
        "techreport" => "techreport".to_string(),
        "manual" => "manual".to_string(),
        "misc" => "misc".to_string(),
        "unpublished" => "unpublished".to_string(),
        "proceedings" => "proceedings".to_string(),
        "booklet" => "booklet".to_string(),
        other => other.to_string(),
    }
}

/// Splits a BibTeX pages field (e.g., "12--23" or "635-639") into start/end pages.
/// Returns (start_page, end_page) as Option<String>.
#[must_use]
pub fn split_pages(pages: &str) -> (Option<String>, Option<String>) {
    let pages = pages.trim();
    if pages.is_empty() || pages == "null" {
        return (None, None);
    }

    // Try double-dash first, then single dash
    if let Some((start, end)) = pages.split_once("--") {
        let start = start.trim().to_string();
        let end = end.trim().to_string();
        return (
            if start.is_empty() || start == "null" { None } else { Some(start) },
            if end.is_empty() || end == "null" { None } else { Some(end) },
        );
    }

    // Single dash: need to distinguish range from negative number or part of page
    // e.g., "635-639" is a range, "S123" is not
    if let Some(dash_pos) = pages.find('-') {
        let start = pages[..dash_pos].trim().to_string();
        let end = pages[dash_pos + 1..].trim().to_string();
        // Treat "null" end as missing
        if end == "null" || end.is_empty() {
            return (if start.is_empty() { None } else { Some(start) }, None);
        }
        // Split as range if both parts look like numbers
        if start.parse::<i32>().is_ok() && end.parse::<i32>().is_ok() {
            return (Some(start), Some(end));
        }
    }

    // No dash found or not a range - treat as start page only
    (Some(pages.to_string()), None)
}

/// Cleans an ISSN/ISBN string by stripping format suffixes like "; Print" or "; Electronic".
#[must_use]
pub fn clean_issn(issn: &str) -> String {
    issn.split(';').next().unwrap_or(issn).trim().to_string()
}

/// Converts a parsed BibTeX entry into an RisRecord for use with the
/// existing validation and import pipeline.
#[must_use]
pub fn bibtex_to_ris_record(entry: &BibtexEntry) -> RisRecord {
    // Build a lookup for fields (last value wins for duplicates)
    let field_map: HashMap<&str, &str> =
        entry.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let mut record =
        RisRecord { reference_type: Some(map_entry_type(&entry.entry_type)), ..Default::default() };

    // Direct string mappings
    record.title = field_map.get("title").map(|v| (*v).to_string());

    record.abstract_text =
        field_map.get("abstract").or_else(|| field_map.get("annote")).map(|v| (*v).to_string());

    record.doi =
        crate::ris::doi::normalize_doi(field_map.get("doi").copied()).map(|s| s.to_string());

    record.url =
        field_map.get("url").or_else(|| field_map.get("howpublished")).map(|v| (*v).to_string());

    record.journal = field_map
        .get("journal")
        .or_else(|| field_map.get("journaltitle"))
        .map(|v| (*v).to_string());

    record.volume = field_map.get("volume").map(|v| (*v).to_string());
    record.issue =
        field_map.get("number").or_else(|| field_map.get("issue")).map(|v| (*v).to_string());

    record.language = field_map.get("language").map(|v| (*v).to_string());

    record.publisher = field_map
        .get("publisher")
        .or_else(|| field_map.get("school")) // for theses
        .or_else(|| field_map.get("institution")) // for tech reports
        .map(|v| (*v).to_string());

    record.publisher_address = field_map.get("address").map(|v| (*v).to_string());

    record.notes = field_map.get("note").map(|v| (*v).to_string());

    // ISSN (clean suffixes like "; Print" or "; Electronic")
    record.issn = field_map.get("issn").or_else(|| field_map.get("isbn")).map(|v| clean_issn(v));

    // Electronic ISSN
    record.eissn = field_map.get("eissn").map(|v| clean_issn(v));

    // Year
    record.publication_year = field_map.get("year").and_then(|y| y.trim().parse::<i32>().ok());

    // Date (combine month + year if available)
    if let (Some(month), Some(year)) = (field_map.get("month"), field_map.get("year")) {
        // Map month names to numbers
        let month_lower = month.to_lowercase();
        let month_num = match month_lower.as_str() {
            "jan" | "january" => "01",
            "feb" | "february" => "02",
            "mar" | "march" => "03",
            "apr" | "april" => "04",
            "may" => "05",
            "jun" | "june" => "06",
            "jul" | "july" => "07",
            "aug" | "august" => "08",
            "sep" | "september" => "09",
            "oct" | "october" => "10",
            "nov" | "november" => "11",
            "dec" | "december" => "12",
            other if other.chars().all(|c| c.is_ascii_digit()) => other,
            _ => "",
        };
        if !month_num.is_empty() {
            record.date = Some(format!("{}-{}", year, month_num));
        }
    }

    // Authors (split by " and ")
    if let Some(author_str) = field_map.get("author") {
        record.authors = split_authors(author_str);
    }

    // Pages
    if let Some(pages_str) = field_map.get("pages") {
        let (start, end) = split_pages(pages_str);
        record.start_page = start;
        record.end_page = end;
    }

    // Keywords
    if let Some(kw_str) = field_map.get("keywords") {
        record.keywords = split_keywords(kw_str);
    }

    // Affiliation extraction with priority:
    // 1. institution → use directly
    // 2. organization → use directly
    // 3. affiliation → if contains comma, extract last comma part; otherwise use as-is
    if let Some(inst) = field_map.get("institution") {
        record.affiliation = Some(inst.trim().to_string());
    } else if let Some(org) = field_map.get("organization") {
        record.affiliation = Some(org.trim().to_string());
    } else if let Some(aff) = field_map.get("affiliation") {
        let trimmed = aff.trim();
        if trimmed.contains(',') {
            // "Department of X, University of Y" → "University of Y"
            record.affiliation = trimmed
                .split(',')
                .next_back()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        } else {
            record.affiliation = Some(trimmed.to_string());
        }
    }

    // Store entry type and key in extras for traceability
    record.extras.entry("_bibtex_type".to_string()).or_default().push(entry.entry_type.clone());
    record.extras.entry("_bibtex_key".to_string()).or_default().push(entry.key.clone());

    // Map WoS-specific citation count fields
    record.num_cited = field_map.get("times-cited").and_then(|v| v.trim().parse::<i32>().ok());

    record.num_references =
        field_map.get("number-of-cited-references").and_then(|v| v.trim().parse::<i32>().ok());

    // Normalize BibTeX "cited-references" field to "CR" for CR parser compatibility.
    // WoS BibTeX stores each cited reference on its own line inside braces.
    // The BibTeX parser preserves newlines within brace-delimited values.
    // Each line ends with a period '.' - we split on newlines, not periods,
    // to avoid breaking DOIs like "10.1016/j.foreco.2017.04.005".
    if let Some(cr_text) = field_map.get("cited-references") {
        let lines: Vec<String> = cr_text
            .split('\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if !lines.is_empty() {
            record.extras.entry("CR".to_string()).or_default().extend(lines);
        }
    }

    // Store unrecognized fields in extras
    let known_fields = [
        "title",
        "author",
        "abstract",
        "annote",
        "year",
        "month",
        "doi",
        "journal",
        "journaltitle",
        "volume",
        "number",
        "issue",
        "pages",
        "keywords",
        "url",
        "howpublished",
        "language",
        "publisher",
        "school",
        "institution",
        "organization",
        "affiliation",
        "address",
        "issn",
        "eissn",
        "isbn",
        "note",
        "times-cited",
        "number-of-cited-references",
        "cited-references",
    ];

    for (field_name, field_value) in &entry.fields {
        if !known_fields.contains(&field_name.as_str()) {
            record.extras.entry(field_name.clone()).or_default().push(field_value.clone());
        }
    }

    record
}

/// Converts all BibTeX entries to RisRecords.
#[must_use]
pub fn convert_bibtex_entries(entries: &[BibtexEntry]) -> Vec<RisRecord> {
    entries.iter().map(bibtex_to_ris_record).collect()
}
