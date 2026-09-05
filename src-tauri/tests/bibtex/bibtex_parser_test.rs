//! Unit tests for `bibtex::parser::parse_bibtex`.
//!
//! Extracted from inline `#[cfg(test)] mod tests` in
//! `src/bibtex/parser.rs` to keep the source file compact.

use std::collections::HashMap;

use bango_lib::bibtex::parser::parse_bibtex;

#[test]
fn test_parse_simple_article() {
    let input = r#"@article{key1,
  author = {John Doe},
  title = {A Test Title},
  year = {2023},
}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.errors.len(), 0);

    let entry = &result.entries[0];
    assert_eq!(entry.entry_type, "article");
    assert_eq!(entry.key, "key1");

    let fields: HashMap<&str, &str> =
        entry.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("author").copied(), Some("John Doe"));
    assert_eq!(fields.get("title").copied(), Some("A Test Title"));
    assert_eq!(fields.get("year").copied(), Some("2023"));
}

#[test]
fn test_parse_quoted_values() {
    let input = r#"@article{key1,
  author = "John Doe",
  title = "A \"quoted\" title",
  year = "2023",
}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.errors.len(), 0);

    let entry = &result.entries[0];
    let fields: HashMap<&str, &str> =
        entry.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("author").copied(), Some("John Doe"));
    assert!(fields.get("title").unwrap().contains("quoted"));
}

#[test]
fn test_parse_multiple_entries() {
    let input = r#"@article{key1, title = {First}}
@book{key2, title = {Second}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 2);
    assert_eq!(result.entries[0].entry_type, "article");
    assert_eq!(result.entries[1].entry_type, "book");
}

#[test]
fn test_parse_empty_input() {
    let result = parse_bibtex("");
    assert_eq!(result.entries.len(), 0);
    assert_eq!(result.errors.len(), 0);
}

#[test]
fn test_parse_comments() {
    let input = r#"% This is a comment
@article{key1, title = {Title}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
}

#[test]
fn test_parse_skip_preamble_and_comment() {
    let input = r#"@preamble{"Some preamble text"}
@comment{Some comment}
@article{key1, title = {Title}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].key, "key1");
}

#[test]
fn test_parse_string_macro() {
    let input = r#"@string{myjournal = "Journal of Tests"}
@article{key1,
  journal = myjournal,
  title = {Title},
}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    let entry = &result.entries[0];
    let fields: HashMap<&str, &str> =
        entry.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("journal").copied(), Some("Journal of Tests"));
}

#[test]
fn test_parse_nested_braces() {
    let input = r#"@article{key1, title = {The {Art} of {Programming}}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    let entry = &result.entries[0];
    let fields: HashMap<&str, &str> =
        entry.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("title").copied(), Some("The {Art} of {Programming}"));
}

#[test]
fn test_parse_bare_number_value() {
    let input = r#"@article{key1, year = 2023, title = {Title}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    let fields: HashMap<&str, &str> =
        result.entries[0].fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("year").copied(), Some("2023"));
}

#[test]
fn test_parse_concatenation() {
    let input = r#"@string{myjournal = "Nice Journal"}
@article{key1, journal = "The " # myjournal, title = {Title}}"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    let fields: HashMap<&str, &str> =
        result.entries[0].fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(fields.get("journal").copied(), Some("The Nice Journal"));
}

#[test]
fn test_parse_parenthesis_delimiters() {
    let input = r#"@article(key1, title = {Title})"#;
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].key, "key1");
}

#[test]
fn test_parse_bom() {
    let input = "\u{FEFF}@article{key1, title = {Title}}";
    let result = parse_bibtex(input);
    assert_eq!(result.entries.len(), 1);
}

#[test]
fn test_parse_ebsco_double_quote_in_title() {
    // Entry 6 from 8-valid-2-invalid-sugar.bibtex has "" at start of title
    let input = r#"@article{2854824120170701,
  abstract  = " Some abstract.",
  author    = "Rutherford, Alexandra",
  number    = "3",
  title     = ""Making better use of U.S. women" Psychology, sex roles, and womanpower in post-WWII America.",
  volume    = "53",
  year      = "2017",
}"#;
    let result = parse_bibtex(input);
    eprintln!("ENTRIES: {}", result.entries.len());
    eprintln!("ERRORS: {}", result.errors.len());
    for (i, e) in result.entries.iter().enumerate() {
        eprintln!(
            "  entry[{}] key={}, title_field={:?}",
            i,
            e.key,
            e.fields.iter().find(|(k, _)| k == "title").map(|(_, v)| v.as_str())
        );
    }
    for e in &result.errors {
        eprintln!("  error: idx={} msg={}", e.entry_index, e.message);
    }
    assert_eq!(
        result.entries.len(),
        1,
        "Should parse 1 entry, got {} entries, {} errors",
        result.entries.len(),
        result.errors.len()
    );
}
