//! Integration tests for `ris::n1_parser::parse_n1_citation_data`.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/ris/n1_parser.rs` to keep the source file compact.

use bango_lib::ris::n1_parser::parse_n1_citation_data;

#[test]
fn standard_wos_format() {
    let input = "\
Times Cited in Web of Science Core Collection:  44
Total Times Cited:  49
Cited Reference Count:  34";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(49));
    assert_eq!(refs, Some(34));
}

#[test]
fn single_line() {
    let input = "Total Times Cited: 49 Cited Reference Count: 34";
    // strip_prefix won't match because the line doesn't start with the key after "49 "
    // Actually this is one line - strip_prefix checks start of the trimmed line
    // "Total Times Cited: 49 Cited Reference Count: 34" starts with "Total Times Cited:"
    // rest = " 49 Cited Reference Count: 34", trim -> "49 Cited Reference Count: 34"
    // parse::<i32>() will fail because it's not just digits
    // So this format won't match - that's expected per the design (each field on its own line)
    let (cited, refs) = parse_n1_citation_data(input);
    // Single-line compact format won't parse because the value isn't pure digits
    assert_eq!(cited, None);
    assert_eq!(refs, None);
}

#[test]
fn mixed_with_notes() {
    let input = "\
Important paper for methodology review
Total Times Cited:  49
Cited Reference Count:  34";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(49));
    assert_eq!(refs, Some(34));
}

#[test]
fn only_total_times_cited() {
    let input = "Total Times Cited: 12";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(12));
    assert_eq!(refs, None);
}

#[test]
fn only_cited_reference_count() {
    let input = "Cited Reference Count: 7";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, None);
    assert_eq!(refs, Some(7));
}

#[test]
fn no_citation_data() {
    let input = "This is a regular note with no citation data";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, None);
    assert_eq!(refs, None);
}

#[test]
fn empty_string() {
    let (cited, refs) = parse_n1_citation_data("");
    assert_eq!(cited, None);
    assert_eq!(refs, None);
}

#[test]
fn zero_values() {
    let input = "\
Total Times Cited: 0
Cited Reference Count: 0";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(0));
    assert_eq!(refs, Some(0));
}

#[test]
fn extra_whitespace() {
    let input = "Total Times Cited:    49\nCited Reference Count:    34";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(49));
    assert_eq!(refs, Some(34));
}

#[test]
fn large_numbers() {
    let input = "\
Total Times Cited: 1234
Cited Reference Count: 5678";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, Some(1234));
    assert_eq!(refs, Some(5678));
}

#[test]
fn non_numeric_value() {
    let input = "Total Times Cited: N/A";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, None);
    assert_eq!(refs, None);
}

#[test]
fn duplicate_keys() {
    let input = "\
Total Times Cited: 49
Total Times Cited: 99
Cited Reference Count: 34
Cited Reference Count: 88";
    let (cited, refs) = parse_n1_citation_data(input);
    // First value wins
    assert_eq!(cited, Some(49));
    assert_eq!(refs, Some(34));
}

#[test]
fn only_core_collection_line() {
    // "Times Cited in Web of Science Core Collection:" does NOT match
    // because we only match lines starting with "Total Times Cited:"
    let input = "Times Cited in Web of Science Core Collection: 44";
    let (cited, refs) = parse_n1_citation_data(input);
    assert_eq!(cited, None);
    assert_eq!(refs, None);
}
