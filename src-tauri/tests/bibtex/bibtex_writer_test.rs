//! Tests for the BibTeX export writer (`bibtex::writer`).

use std::collections::HashMap;

use bango_lib::bibtex::parser::{parse_bibtex, BibtexEntry};
use bango_lib::bibtex::writer::{
    article_to_bibtex, articles_to_bibtex, escape_field, first_author_surname, make_citation_key,
    map_reference_type,
};
use bango_lib::export::ris_writer::RisExportArticle;

fn make_article(title: &str, authors: &[&str]) -> RisExportArticle {
    RisExportArticle {
        reference_type: Some("JOUR".to_string()),
        title: title.to_string(),
        abstract_text: "Abstract text.".to_string(),
        authors: authors.iter().map(|a| (*a).to_string()).collect(),
        publication_year: Some(2023),
        doi: Some("10.1001/example.2023.001".to_string()),
        journal: Some("Journal of Testing".to_string()),
        volume: Some("12".to_string()),
        issue: Some("3".to_string()),
        start_page: Some("100".to_string()),
        end_page: Some("110".to_string()),
        keywords: vec!["sugar tax".to_string(), "policy".to_string()],
        tags: vec!["clinical-trial".to_string()],
        url: Some("https://example.com/paper".to_string()),
        language: Some("en".to_string()),
        publisher: Some("Test Publisher".to_string()),
        issn: Some("1234-5678".to_string()),
        notes: Some("Imported note.".to_string()),
        ai_reasoning: Some("Meets inclusion criterion 1.".to_string()),
        user_notes: Some("Working note.".to_string()),
        ai_decision: None,
        labels: vec!["priority-read".to_string()],
        matched_inclusion_criteria: vec!["criterion-1".to_string()],
        matched_exclusion_criteria: vec![],
    }
}

/// Parse the rendered `.bib` and return (entry, field map) for assertions.
fn parse_single(bib: &str) -> (BibtexEntry, HashMap<String, String>) {
    let parsed = parse_bibtex(bib);
    assert!(parsed.errors.is_empty(), "parse errors: {:?}", parsed.errors);
    assert_eq!(parsed.entries.len(), 1);
    let entry = parsed.entries.into_iter().next().unwrap_or_default();
    let fields: HashMap<String, String> = entry.fields.iter().cloned().collect();
    (entry, fields)
}

#[test]
fn round_trip_through_parser() {
    let bib =
        articles_to_bibtex(&[make_article("Sugar Tax Effects", &["Smith, John", "Doe, Anne"])]);
    let (entry, fields) = parse_single(&bib);
    assert_eq!(entry.entry_type, "article");
    for (name, expected) in [
        ("title", "Sugar Tax Effects"),
        ("abstract", "Abstract text."),
        ("author", "Smith, John and Doe, Anne"),
        ("journal", "Journal of Testing"),
        ("year", "2023"),
        ("volume", "12"),
        ("number", "3"),
        ("pages", "100--110"),
        ("doi", "10.1001/example.2023.001"),
        ("keywords", "sugar tax, policy, Bango:clinical-trial, Bango:priority-read"),
        ("annote", "Imported note."),
        ("comment", "Meets inclusion criterion 1."),
        ("note", "Working note."),
        ("url", "https://example.com/paper"),
        ("language", "en"),
        ("publisher", "Test Publisher"),
        ("issn", "1234-5678"),
        // C8 mirror: raw balanced JSON round-trips through the parser.
        ("bango-criteria", "{\"inc\":[\"criterion-1\"],\"exc\":[]}"),
    ] {
        assert_eq!(fields.get(name).map(String::as_str), Some(expected), "field {name}");
    }
}

#[test]
fn citation_key_surname_year_title_word_with_ascii_folding() {
    assert_eq!(
        make_citation_key(&make_article("A Sugar Tax Study", &["Müller, Anna"])),
        "muller2023sugar"
    );
}

#[test]
fn citation_key_no_comma_author_uses_last_token() {
    assert_eq!(first_author_surname(&["John A. Smith".to_string()]), "Smith");
    assert_eq!(
        make_citation_key(&make_article("Sugar Taxes", &["John A. Smith"])),
        "smith2023sugar"
    );
}

#[test]
fn citation_keys_deduplicated_with_letter_suffix() {
    let a = make_article("Sugar Taxes", &["Smith, John"]);
    let parsed = parse_bibtex(&articles_to_bibtex(&[a.clone(), a.clone(), a.clone()]));
    let keys: Vec<&str> = parsed.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["smith2023sugar", "smith2023sugarb", "smith2023sugarc"]);
}

#[test]
fn single_page_value_passes_through_verbatim() {
    let mut a = make_article("Sugar Taxes", &["Smith, John"]);
    a.start_page = Some("e12345".to_string());
    a.end_page = None;
    let (_, fields) = parse_single(&articles_to_bibtex(&[a]));
    assert_eq!(fields.get("pages").map(String::as_str), Some("e12345"));
}

#[test]
fn braces_escaped_in_field_values() {
    assert_eq!(escape_field("A {Nested} Title"), "A \\{Nested\\} Title");
    let a = make_article("A {Nested} Title", &["Smith, John"]);
    let bib = article_to_bibtex(&a, "smith2023nested");
    assert!(bib.contains("title = {A \\{Nested\\} Title}"));
}

#[test]
fn keywords_union_includes_prefixed_tags_and_labels() {
    let mut a = make_article("Sugar Taxes", &["Smith, John"]);
    a.keywords = vec![];
    a.tags = vec!["ml".to_string()];
    // Empty plain keywords: the prefixed tags/labels still emit (RIS KW parity).
    let (_, fields) = parse_single(&articles_to_bibtex(&[a]));
    assert_eq!(fields.get("keywords").map(String::as_str), Some("Bango:ml, Bango:priority-read"));
}

#[test]
fn reference_type_mapping() {
    assert_eq!(map_reference_type(Some("JOUR")), "article");
    assert_eq!(map_reference_type(Some("book")), "book");
    assert_eq!(map_reference_type(Some("conference")), "inproceedings");
    assert_eq!(map_reference_type(None), "article");
    assert_eq!(map_reference_type(Some("weird")), "weird");
}

#[test]
fn doi_canonicalized_in_output() {
    let mut a = make_article("Sugar Taxes", &["Smith, John"]);
    a.doi = Some("https://doi.org/10.1/AbC".to_string());
    let (_, fields) = parse_single(&articles_to_bibtex(&[a]));
    assert_eq!(fields.get("doi").map(String::as_str), Some("10.1/abc"));
}

#[test]
fn empty_optional_fields_omitted() {
    let minimal = RisExportArticle {
        reference_type: None,
        title: "Only Title".to_string(),
        abstract_text: String::new(),
        authors: vec![],
        publication_year: None,
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec![],
        tags: vec![],
        url: None,
        language: None,
        publisher: None,
        issn: None,
        notes: None,
        ai_reasoning: None,
        user_notes: None,
        ai_decision: None,
        labels: vec![],
        matched_inclusion_criteria: vec![],
        matched_exclusion_criteria: vec![],
    };
    let bib = articles_to_bibtex(&[minimal]);
    for absent in [
        "author",
        "abstract",
        "year",
        "journal",
        "volume",
        "number",
        "pages",
        "doi",
        "keywords",
        "annote",
        "comment",
        "note",
        "url",
        "language",
        "publisher",
        "issn",
        "bango-criteria",
    ] {
        assert!(!bib.contains(&format!("  {absent} = ")), "field {absent} must be omitted");
    }
    assert!(bib.contains("@article{anonndonly,"));
}

fn citation_key_authorless_uses_anon_and_yearless_nd() {
    assert_eq!(make_citation_key(&make_article("Sugar Taxes", &[])), "anon2023sugar");
    let mut a = make_article("Sugar Taxes", &["Smith, John"]);
    a.publication_year = None;
    assert_eq!(make_citation_key(&a), "smithndsugar");
}
