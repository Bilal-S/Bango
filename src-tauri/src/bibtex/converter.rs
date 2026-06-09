use std::collections::HashMap;

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

/// Splits a BibTeX author string by ` and ` into individual author names.
/// Handles both "First Last" and "Last, First" formats.
fn split_authors(author_str: &str) -> Vec<String> {
    author_str.split(" and ").map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect()
}

/// Splits a BibTeX pages field (e.g., "12--23" or "635-639") into start/end pages.
/// Returns (start_page, end_page) as Option<String>.
fn split_pages(pages: &str) -> (Option<String>, Option<String>) {
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

/// Splits a keywords string by semicolons or commas into individual keywords.
fn split_keywords(keywords_str: &str) -> Vec<String> {
    let trimmed = keywords_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Try semicolon separator first (common in EBSCO exports)
    if trimmed.contains(';') {
        return trimmed
            .split(';')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();
    }

    // Fall back to comma separator
    if trimmed.contains(',') {
        return trimmed
            .split(',')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();
    }

    // Single keyword
    vec![trimmed.to_string()]
}

/// Cleans an ISSN/ISBN string by stripping format suffixes like "; Print" or "; Electronic".
fn clean_issn(issn: &str) -> String {
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
    // Each line ends with a period '.' — we split on newlines, not periods,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex::parser::parse_bibtex;

    #[test]
    fn test_split_authors_single() {
        let authors = split_authors("John Doe");
        assert_eq!(authors, vec!["John Doe"]);
    }

    #[test]
    fn test_split_authors_multiple() {
        let authors = split_authors("Bossie, Andrew and Kuehn, Daniel");
        assert_eq!(authors, vec!["Bossie, Andrew", "Kuehn, Daniel"]);
    }

    #[test]
    fn test_split_authors_empty() {
        let authors = split_authors("");
        assert!(authors.is_empty());
    }

    #[test]
    fn test_split_pages_range_double_dash() {
        let (start, end) = split_pages("12--23");
        assert_eq!(start, Some("12".to_string()));
        assert_eq!(end, Some("23".to_string()));
    }

    #[test]
    fn test_split_pages_range_single_dash() {
        let (start, end) = split_pages("635-639");
        assert_eq!(start, Some("635".to_string()));
        assert_eq!(end, Some("639".to_string()));
    }

    #[test]
    fn test_split_pages_single_page() {
        let (start, end) = split_pages("42");
        assert_eq!(start, Some("42".to_string()));
        assert_eq!(end, None);
    }

    #[test]
    fn test_split_pages_null_end() {
        let (start, end) = split_pages("13-null");
        assert_eq!(start, Some("13".to_string()));
        assert_eq!(end, None);
    }

    #[test]
    fn test_split_pages_empty() {
        let (start, end) = split_pages("");
        assert_eq!(start, None);
        assert_eq!(end, None);
    }

    #[test]
    fn test_split_keywords_semicolons() {
        let keywords = split_keywords("MILITARY spending; LABOR market; CONTRACTS");
        assert_eq!(keywords, vec!["MILITARY spending", "LABOR market", "CONTRACTS"]);
    }

    #[test]
    fn test_split_keywords_commas() {
        let keywords = split_keywords("keyword1, keyword2, keyword3");
        assert_eq!(keywords, vec!["keyword1", "keyword2", "keyword3"]);
    }

    #[test]
    fn test_split_keywords_empty() {
        let keywords = split_keywords("");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_clean_issn() {
        assert_eq!(clean_issn("0036-8733; Print"), "0036-8733");
        assert_eq!(clean_issn("1742-6316; Electronic"), "1742-6316");
        assert_eq!(clean_issn("0952-1909"), "0952-1909");
    }

    #[test]
    fn test_convert_simple_entry() {
        let input = r#"@article{key1,
  author = "Bossie, Andrew and Kuehn, Daniel",
  title = "A Test Title",
  year = "2021",
  journal = "Test Journal",
  volume = "28",
  number = "8",
  pages = "635-639",
  doi = "10.1234/test",
  keywords = "keyword1; keyword2",
  issn = "1350-4851",
}"#;
        let parse_result = parse_bibtex(input);
        assert_eq!(parse_result.entries.len(), 1);

        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.reference_type.as_deref(), Some("article"));
        assert_eq!(record.title.as_deref(), Some("A Test Title"));
        assert_eq!(record.authors, vec!["Bossie, Andrew", "Kuehn, Daniel"]);
        assert_eq!(record.publication_year, Some(2021));
        assert_eq!(record.journal.as_deref(), Some("Test Journal"));
        assert_eq!(record.volume.as_deref(), Some("28"));
        assert_eq!(record.issue.as_deref(), Some("8"));
        assert_eq!(record.start_page.as_deref(), Some("635"));
        assert_eq!(record.end_page.as_deref(), Some("639"));
        assert_eq!(record.doi.as_deref(), Some("10.1234/test"));
        assert_eq!(record.keywords, vec!["keyword1", "keyword2"]);
        assert_eq!(record.issn.as_deref(), Some("1350-4851"));
    }

    #[test]
    fn test_convert_entry_with_empty_fields() {
        let input = r#"@article{key1,
  author = "Single Author",
  title = "Title Only",
  abstract = "",
  keywords = "",
  note = "",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);

        assert_eq!(record.title.as_deref(), Some("Title Only"));
        assert_eq!(record.abstract_text.as_deref(), Some("")); // Empty but present
        assert!(record.keywords.is_empty()); // Empty keywords = no entries
        assert_eq!(record.authors, vec!["Single Author"]);
    }

    #[test]
    fn test_convert_book_entry() {
        let input = r#"@book{key1,
  author = "Knuth, Donald E.",
  title = "The Art of Computer Programming",
  publisher = "Addison-Wesley",
  year = "1997",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);

        assert_eq!(record.reference_type.as_deref(), Some("book"));
        assert_eq!(record.publisher.as_deref(), Some("Addison-Wesley"));
    }

    #[test]
    fn test_convert_preserves_bibtex_metadata_in_extras() {
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
  customfield = "custom value",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);

        assert_eq!(record.extras.get("_bibtex_type").map(|v| &v[0]), Some(&"article".to_string()));
        assert_eq!(record.extras.get("_bibtex_key").map(|v| &v[0]), Some(&"key1".to_string()));
        assert_eq!(
            record.extras.get("customfield").map(|v| &v[0]),
            Some(&"custom value".to_string())
        );
    }

    #[test]
    fn test_convert_issn_with_suffix() {
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
  issn = "0036-8733; Print",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);

        assert_eq!(record.issn.as_deref(), Some("0036-8733"));
    }

    #[test]
    fn test_convert_pages_with_null() {
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
  pages = "13-null",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);

        assert_eq!(record.start_page.as_deref(), Some("13"));
        assert_eq!(record.end_page, None);
    }

    #[test]
    fn test_affiliation_from_institution() {
        let input = r#"@techreport{key1,
  author = "Author",
  title = "Title",
  institution = "University of Z",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.affiliation.as_deref(), Some("University of Z"));
    }

    #[test]
    fn test_affiliation_from_organization() {
        let input = r#"@inproceedings{key1,
  author = "Author",
  title = "Title",
  organization = "Institute Name",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.affiliation.as_deref(), Some("Institute Name"));
    }

    #[test]
    fn test_affiliation_from_affiliation_field() {
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
  affiliation = "University of Y",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.affiliation.as_deref(), Some("University of Y"));
    }

    #[test]
    fn test_affiliation_from_affiliation_with_comma() {
        // "Department of X, University of Y" → "University of Y"
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
  affiliation = "Department of X, University of Y",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.affiliation.as_deref(), Some("University of Y"));
    }

    #[test]
    fn test_affiliation_priority_institution_over_organization() {
        let input = r#"@techreport{key1,
  author = "Author",
  title = "Title",
  institution = "University of Z",
  organization = "Institute Name",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert_eq!(record.affiliation.as_deref(), Some("University of Z"));
    }

    #[test]
    fn test_affiliation_no_field() {
        let input = r#"@article{key1,
  author = "Author",
  title = "Title",
}"#;
        let parse_result = parse_bibtex(input);
        let record = bibtex_to_ris_record(&parse_result.entries[0]);
        assert!(record.affiliation.is_none());
    }
}
