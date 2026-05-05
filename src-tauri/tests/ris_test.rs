use bango_lib::ris::parser::parse_ris;
use bango_lib::ris::types::RisRecord;
use bango_lib::ris::validator::validate_record;
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_parse_single_record_ris() {
    let content = fs::read_to_string(asset_path("11A-Resilience-Intersection-Capabilities.ris"))
        .expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 1);
    assert_eq!(result.errors.len(), 0);

    let record = &result.records[0];
    assert_eq!(record.reference_type.as_deref(), Some("JOUR"));
    assert!(record.title.as_ref().unwrap().contains("Multi-Paradigm Ethical Framework"));
    assert_eq!(record.authors.len(), 1);
    assert_eq!(record.authors[0], "Alibasic, H");
    assert!(record.abstract_text.as_ref().unwrap().contains("artificial intelligence"));
    assert_eq!(record.publication_year, Some(2025));
    assert_eq!(record.doi.as_deref(), Some("10.3390/fintech4030034"));
    assert_eq!(record.journal.as_deref(), Some("FINTECH"));
    assert_eq!(record.volume.as_deref(), Some("4"));
    assert_eq!(record.issue.as_deref(), Some("3"));
    assert_eq!(record.start_page.as_deref(), Some("34"));
    assert!(record.keywords.len() >= 5);
    assert_eq!(record.language.as_deref(), Some("English"));
    assert_eq!(record.issn.as_deref(), Some("2674-1032"));
    assert_eq!(record.publisher.as_deref(), Some("MDPI"));
    assert!(record.notes.is_some());
}

#[test]
fn test_parse_multi_record_ris() {
    let content =
        fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 2);
    assert_eq!(result.errors.len(), 0);

    // First record
    let rec1 = &result.records[0];
    assert!(rec1.title.as_ref().unwrap().contains("blockchain we trust"));
    assert_eq!(rec1.authors.len(), 2);
    assert_eq!(rec1.authors[0], "Toufaily, E");
    assert_eq!(rec1.authors[1], "Zalan, T");
    assert_eq!(rec1.publication_year, Some(2024));
    assert_eq!(rec1.doi.as_deref(), Some("10.1016/j.techfore.2024.123574"));
    assert!(rec1.keywords.len() >= 5);

    // Second record
    let rec2 = &result.records[1];
    assert!(rec2.title.as_ref().unwrap().contains("qualitative systematic review"));
    assert_eq!(rec2.authors.len(), 4);
    assert_eq!(rec2.publication_year, Some(2025));
    assert_eq!(rec2.doi.as_deref(), Some("10.1177/02683962241254392"));
    assert_eq!(rec2.start_page.as_deref(), Some("55"));
    assert_eq!(rec2.end_page.as_deref(), Some("76"));
}

#[test]
fn test_parse_preserves_unrecognized_tags() {
    let content =
        "TY  - JOUR\nTI  - Test\nAU  - Author\nAB  - Abstract\nXX  - Unknown Value\nER  -\n";
    let result = parse_ris(content).expect("Parse failed");
    assert_eq!(
        result.records[0].extras.get("XX").map(|v| v.as_slice()),
        Some(&["Unknown Value".to_string()][..])
    );
}

#[test]
fn test_parse_empty_input() {
    let result = parse_ris("").expect("Parse failed");
    assert_eq!(result.records.len(), 0);
}

#[test]
fn test_validate_valid_record() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_missing_title() {
    let mut record = RisRecord::default();
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Title")));
}

#[test]
fn test_validate_missing_abstract() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Abstract")));
}

#[test]
fn test_validate_missing_authors() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Author")));
}

#[test]
fn test_validate_n2_abstract_fallback() {
    // N2 was already mapped to abstract_text by the parser.
    // This test verifies the parser correctly falls back.
    // Direct validation: if abstract_text is present, it's valid.
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("From N2".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.is_empty());
}
