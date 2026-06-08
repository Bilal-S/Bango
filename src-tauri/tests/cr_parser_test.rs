use bango_lib::ris::cr_parser;
use serde_json::json;

#[test]
fn test_parse_single_cr_line() {
    let extras = json!({
        "CR": ["Doe J, 2020, J MED, V1, P10, 10.1234/test"]
    });
    let papers = cr_parser::parse_cr_entries(&extras);
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].authors, vec!["Doe J"]);
    assert_eq!(papers[0].publication_year, Some(2020));
    assert_eq!(papers[0].journal.as_deref(), Some("J MED"));
    assert_eq!(papers[0].volume.as_deref(), Some("1"));
    assert_eq!(papers[0].start_page.as_deref(), Some("10"));
    assert_eq!(papers[0].doi.as_deref(), Some("10.1234/test"));
}

#[test]
fn test_parse_multiple_cr_entries() {
    let extras = json!({
        "CR": [
            "Smith A, 2019, NATURE, V5, P100, 10.1/a",
            "Jones B, 2021, SCIENCE, V2, P50, 10.2/b"
        ]
    });
    let papers = cr_parser::parse_cr_entries(&extras);
    assert_eq!(papers.len(), 2);
    assert_eq!(papers[0].doi.as_deref(), Some("10.1/a"));
    assert_eq!(papers[1].doi.as_deref(), Some("10.2/b"));
}

#[test]
fn test_parse_cr_no_doi() {
    let extras = json!({
        "CR": ["Brown K, 2018, CELL, V175, P1024"]
    });
    let papers = cr_parser::parse_cr_entries(&extras);
    assert_eq!(papers.len(), 1);
    assert!(papers[0].doi.is_none());
    assert_eq!(papers[0].volume.as_deref(), Some("175"));
}

#[test]
fn test_parse_cr_empty_extras() {
    let extras = json!({});
    let papers = cr_parser::parse_cr_entries(&extras);
    assert!(papers.is_empty());
}
