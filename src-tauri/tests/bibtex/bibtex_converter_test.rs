//! Unit tests for `bibtex::converter` helpers and record conversion.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/bibtex/converter.rs` to keep the source file compact.

use bango_lib::biblio::normalizer::{split_authors, split_keywords};
use bango_lib::bibtex::converter::{bibtex_to_ris_record, clean_issn, split_pages};
use bango_lib::bibtex::parser::parse_bibtex;

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
    assert_eq!(record.extras.get("customfield").map(|v| &v[0]), Some(&"custom value".to_string()));
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
