//! Integration tests for `ris::cr_parser`.
//!
//! Combines existing entry-level tests with the helper-level tests
//! (`clean_wos_cr_line`, `extract_doi`, `looks_like_journal`, `parse_cr_line`)
//! extracted from the inline `#[cfg(test)] mod tests` in `src/ris/cr_parser.rs`.

use bango_lib::ris::cr_parser;
use bango_lib::ris::cr_parser::{
    clean_wos_cr_line, extract_doi, looks_like_journal, parse_cr_line,
};
use serde_json::json;

// ── Entry-level parsing ────────────────────────────────────────

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

// ── clean_wos_cr_line ─────────────────────────────────────────

#[test]
fn test_clean_standard_entry() {
    assert_eq!(
        clean_wos_cr_line(
            "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005."
        ),
        "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005"
    );
}

#[test]
fn test_clean_doi_array_entry() {
    assert_eq!(
        clean_wos_cr_line("Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]."),
        "Alexander H. D., 2021, DATA OBJECT, DOI [10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]"
    );
}

#[test]
fn test_clean_anonymous_bracket() {
    assert_eq!(
        clean_wos_cr_line("{[}Anonymous], 1978, Canadian System of Soil Classification."),
        "[Anonymous], 1978, Canadian System of Soil Classification"
    );
}

// ── extract_doi ───────────────────────────────────────────────

#[test]
fn test_extract_doi_simple() {
    assert_eq!(
        extract_doi("10.1016/j.foreco.2017.04.005"),
        Some("10.1016/j.foreco.2017.04.005".to_string())
    );
}

#[test]
fn test_extract_doi_with_prefix() {
    assert_eq!(
        extract_doi("DOI 10.1016/j.foreco.2017.04.005"),
        Some("10.1016/j.foreco.2017.04.005".to_string())
    );
}

#[test]
fn test_extract_doi_array() {
    assert_eq!(
        extract_doi("DOI [10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C]"),
        Some("10.6073/pasta/7367d64e999c830a508a7e012ad0824c".to_string())
    );
}

#[test]
fn test_extract_doi_bare_array() {
    assert_eq!(
        extract_doi("[10.1139/x03-183, 10.1139/X03-183]"),
        Some("10.1139/x03-183".to_string())
    );
}

#[test]
fn test_extract_doi_not_a_doi() {
    assert_eq!(extract_doi("V396"), None);
}

// ── looks_like_journal ────────────────────────────────────────

#[test]
fn test_journal_all_caps() {
    assert!(looks_like_journal("FOREST ECOL MANAG"));
    assert!(looks_like_journal("SCIENCE"));
    assert!(looks_like_journal("NATURE"));
}

#[test]
fn test_not_journal_mixed_case() {
    assert!(!looks_like_journal("Canadian System of Soil Classification"));
    assert!(!looks_like_journal("A key for predicting postfire successional trajectories"));
}

// ── parse_cr_line: real WoS patterns ──────────────────────────

#[test]
fn parse_standard_entry() {
    let line =
        "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Alexander HD"]);
    assert_eq!(paper.publication_year, Some(2017));
    assert_eq!(paper.journal.as_deref(), Some("FOREST ECOL MANAG"));
    assert_eq!(paper.volume.as_deref(), Some("396"));
    assert_eq!(paper.start_page.as_deref(), Some("35"));
    assert_eq!(paper.doi.as_deref(), Some("10.1016/j.foreco.2017.04.005"));
    // Title should be constructed since CR lines don't have article titles
    assert!(paper.title.is_some());
    assert!(paper.title.as_ref().unwrap().contains("Alexander HD"));
}

#[test]
fn parse_doi_array_entry() {
    let line = "Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C].";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Alexander H. D."]);
    assert_eq!(paper.publication_year, Some(2021));
    // {*}{*}DATA OBJECT{*}{*} → DATA OBJECT (ALL CAPS → treated as journal)
    assert_eq!(paper.journal.as_deref(), Some("DATA OBJECT"));
    assert!(paper.title.is_some()); // auto-constructed descriptive title
                                    // First DOI from the array
    assert_eq!(paper.doi.as_deref(), Some("10.6073/pasta/7367d64e999c830a508a7e012ad0824c"));
}

#[test]
fn parse_anonymous_book() {
    let line = "{[}Anonymous], 1978, Canadian System of Soil Classification.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["[Anonymous]"]);
    assert_eq!(paper.publication_year, Some(1978));
    // Mixed case → title (it's a book title, not a journal)
    assert_eq!(paper.title.as_deref(), Some("Canadian System of Soil Classification"));
    assert!(paper.journal.is_none());
}

#[test]
fn parse_minimal_entry() {
    let line = "Barton Kamil, 2024, CRAN.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Barton Kamil"]);
    assert_eq!(paper.publication_year, Some(2024));
    // "CRAN" is short but all caps → journal
    assert_eq!(paper.journal.as_deref(), Some("CRAN"));
}

#[test]
fn parse_no_doi_entry() {
    let line = "Johnson E. A, 1992, FIRE VEGETATION DYNA.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Johnson E. A"]);
    assert_eq!(paper.publication_year, Some(1992));
    assert_eq!(paper.journal.as_deref(), Some("FIRE VEGETATION DYNA"));
    assert!(paper.doi.is_none());
}

#[test]
fn parse_doi_array_bracket_form() {
    let line = "Johnstone JF, 2004, CAN J FOREST RES, V34, P267, DOI {[}10.1139/x03-183, 10.1139/X03-183].";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Johnstone JF"]);
    assert_eq!(paper.publication_year, Some(2004));
    assert_eq!(paper.journal.as_deref(), Some("CAN J FOREST RES"));
    assert_eq!(paper.volume.as_deref(), Some("34"));
    assert_eq!(paper.start_page.as_deref(), Some("267"));
    assert_eq!(paper.doi.as_deref(), Some("10.1139/x03-183"));
}

#[test]
fn parse_entry_with_doi_prefix() {
    let line = "Fenner M., 2005, The Ecology of Seeds, DOI DOI 10.1017/CBO9780511614101.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Fenner M."]);
    assert_eq!(paper.publication_year, Some(2005));
    // Mixed case → title (it's a book)
    assert_eq!(paper.title.as_deref(), Some("The Ecology of Seeds"));
    // "DOI DOI 10.1017/..." → extract handles double DOI prefix
    assert!(paper.doi.is_some());
    assert!(paper.doi.as_ref().unwrap().starts_with("10."));
}

#[test]
fn parse_complex_doi_with_parens() {
    let line = "Osterkamp TE, 1999, PERMAFROST PERIGLAC, V10, P17, DOI 10.1002/(SICI)1099-1530(199901/03)10:1<17::AID-PPP303>3.0.CO;2-4.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Osterkamp TE"]);
    assert_eq!(paper.publication_year, Some(1999));
    assert_eq!(paper.journal.as_deref(), Some("PERMAFROST PERIGLAC"));
    assert!(paper.doi.is_some());
    assert!(paper.doi.as_ref().unwrap().starts_with("10.1002/"));
}

#[test]
fn parse_entry_without_year() {
    // Some entries lack a year: "Melvin A. M, ECISYSTEMS, V18, P1472."
    let line = "Melvin A. M, ECISYSTEMS, V18, P1472.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Melvin A. M"]);
    // Year not parseable → stays None
    assert!(paper.publication_year.is_none());
    // ECISYSTEMS looks like journal (mostly caps)
    assert!(paper.journal.is_some());
}

#[test]
fn parse_ahrens_book_reference() {
    let line = "Ahrens RJ, 2004, CRYOSOLS: PERMAFROST-AFFECTED SOILS, P627.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Ahrens RJ"]);
    assert_eq!(paper.publication_year, Some(2004));
    // ALL CAPS → journal (book title that's in all caps)
    assert_eq!(paper.journal.as_deref(), Some("CRYOSOLS: PERMAFROST-AFFECTED SOILS"));
    assert_eq!(paper.start_page.as_deref(), Some("627"));
}

#[test]
fn parse_full_cr_line_standard() {
    let line = "Smith J, 2020, NATURE, V581, P364, 10.1038/s41586-020-2012-7";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Smith J"]);
    assert_eq!(paper.publication_year, Some(2020));
    assert_eq!(paper.journal.as_deref(), Some("NATURE"));
    assert_eq!(paper.volume.as_deref(), Some("581"));
    assert_eq!(paper.start_page.as_deref(), Some("364"));
    assert_eq!(paper.doi.as_deref(), Some("10.1038/s41586-020-2012-7"));
    assert!(paper.title.is_some());
}

#[test]
fn parse_too_short_cr_line() {
    assert!(parse_cr_line("").is_none());
    assert!(parse_cr_line("   ").is_none());
}

#[test]
fn parse_cr_entries_from_extras() {
    let extras = json!({
        "CR": [
            "Smith J, 2020, NATURE, V581, P364",
            "Doe A, 2019, SCIENCE"
        ]
    });
    let papers = cr_parser::parse_cr_entries(&extras);
    assert_eq!(papers.len(), 2);
    assert_eq!(papers[0].authors, vec!["Smith J"]);
    assert_eq!(papers[1].authors, vec!["Doe A"]);
}

#[test]
fn parse_cr_entries_no_cr_field() {
    let extras = json!({"AU": ["someone"]});
    let papers = cr_parser::parse_cr_entries(&extras);
    assert!(papers.is_empty());
}

#[test]
fn parse_cr_entries_from_real_bibtex_patterns() {
    let extras = json!({
        "CR": [
            "Ahrens RJ, 2004, CRYOSOLS: PERMAFROST-AFFECTED SOILS, P627.",
            "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.",
            "{[}Anonymous], 1978, Canadian System of Soil Classification.",
            "Barton Kamil, 2024, CRAN.",
            "Johnstone JF, 2004, CAN J FOREST RES, V34, P267, DOI {[}10.1139/x03-183, 10.1139/X03-183].",
            "Osterkamp TE, 1999, PERMAFROST PERIGLAC, V10, P17, DOI 10.1002/(SICI)1099-1530(199901/03)10:1<17::AID-PPP303>3.0.CO;2-4."
        ]
    });
    let papers = cr_parser::parse_cr_entries(&extras);
    assert_eq!(papers.len(), 6, "Should parse all 6 reference patterns");

    // Ahrens - book with page
    assert_eq!(papers[0].authors, vec!["Ahrens RJ"]);
    assert_eq!(papers[0].publication_year, Some(2004));

    // Alexander - standard journal entry with DOI
    assert_eq!(papers[1].doi.as_deref(), Some("10.1016/j.foreco.2017.04.005"));
    assert_eq!(papers[1].volume.as_deref(), Some("396"));

    // Anonymous - book title
    assert_eq!(papers[2].authors, vec!["[Anonymous]"]);
    assert_eq!(papers[2].title.as_deref(), Some("Canadian System of Soil Classification"));

    // Barton - minimal entry
    assert_eq!(papers[3].publication_year, Some(2024));

    // Johnstone - DOI array
    assert_eq!(papers[4].doi.as_deref(), Some("10.1139/x03-183"));

    // Osterkamp - complex DOI
    assert!(papers[5].doi.as_ref().unwrap().starts_with("10.1002/"));
}
