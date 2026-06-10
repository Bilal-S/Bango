//! Integration tests for importing BibTeX and RIS files with references and extra data.
//!
//! Uses `tests/assets/ExampleWReferences.bib` and `tests/assets/ExampleWReferences.ris`
//! which both contain 3 Web of Science articles with cited references, citation counts,
//! keywords, affiliations, and other WoS-specific fields.

use std::fs;
use std::path::Path;

use bango_lib::bibtex::converter::convert_bibtex_entries;
use bango_lib::bibtex::parser::parse_bibtex;
use bango_lib::ris::parser::parse_ris;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn assets_dir() -> &'static Path {
    Path::new("../tests/assets")
}

fn read_asset(name: &str) -> String {
    fs::read_to_string(assets_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to read asset '{}': {}", name, e))
}

/// Macro to assert an optional field is Some with the expected value.
macro_rules! assert_some {
    ($actual:expr, $expected:expr) => {
        match &$actual {
            Some(v) => assert_eq!(v, &$expected, "field value mismatch"),
            None => panic!("expected Some({:?}), got None", $expected),
        }
    };
}

// ── Shared expected data for the 3 WoS articles ─────────────────────────────

/// Record 1: Mack et al. 2021 — "Carbon loss from boreal forest wildfires..."
mod mack {
    pub const TITLE: &str = "Carbon loss from boreal forest wildfires offset by increased \
        dominance of deciduous trees";
    pub const JOURNAL: &str = "SCIENCE";
    pub const YEAR: i32 = 2021;
    pub const VOLUME: &str = "372";
    pub const ISSUE: &str = "6539";
    pub const DOI: &str = "10.1126/science.abf3903";
    pub const FIRST_AUTHOR: &str = "Mack, MC";
    pub const AUTHOR_COUNT_RIS: usize = 7;
    pub const ISSN: &str = "0036-8075";
    pub const ACCESSION: &str = "WOS:000641286700038";
    pub const START_PAGE: &str = "280";
    pub const NUM_CITED: i32 = 262; // Total Times Cited (not just WoS Core)
    pub const NUM_REFERENCES: i32 = 75;
    pub const KW_COUNT_RIS: usize = 10; // Keywords-Plus in KW fields
}

/// Record 2: Chen et al. 2021 — "Future increases in Arctic lightning..."
mod chen {
    pub const TITLE: &str =
        "Future increases in Arctic lightning and fire risk for permafrost carbon";
    pub const JOURNAL: &str = "NATURE CLIMATE CHANGE";
    pub const YEAR: i32 = 2021;
    pub const VOLUME: &str = "11";
    pub const ISSUE: &str = "5";
    pub const DOI: &str = "10.1038/s41558-021-01011-y";
    pub const FIRST_AUTHOR: &str = "Chen, Y";
    pub const AUTHOR_COUNT_RIS: usize = 7;
    pub const ISSN: &str = "1758-678X";
    pub const ACCESSION: &str = "WOS:000636979500002";
    pub const START_PAGE: &str = "404";
    pub const NUM_CITED: i32 = 201; // Total Times Cited
    pub const NUM_REFERENCES: i32 = 89;
    pub const KW_COUNT_RIS: usize = 10;
}

/// Record 3: Brodie et al. 2024 — "Forest thinning and prescribed burning..."
mod brodie {
    pub const TITLE: &str = "Forest thinning and prescribed burning treatments reduce wildfire \
        severity and buffer the impacts of severe fire weather";
    pub const JOURNAL: &str = "FIRE ECOLOGY";
    pub const YEAR: i32 = 2024;
    pub const VOLUME: &str = "20";
    pub const ISSUE: &str = "1";
    pub const DOI: &str = "10.1186/s42408-023-00241-z";
    pub const FIRST_AUTHOR: &str = "Brodie, EG";
    pub const AUTHOR_COUNT_RIS: usize = 5;
    pub const ISSN: &str = "1933-9747";
    pub const ACCESSION: &str = "WOS:001157024900001";
    pub const START_PAGE: &str = "17"; // C7 article-number field
    pub const NUM_CITED: i32 = 83; // Total Times Cited
    pub const NUM_REFERENCES: i32 = 120;
    pub const KW_COUNT_RIS: usize = 16; // 6 author KW + 10 Keywords-Plus
}

// ══════════════════════════════════════════════════════════════════════════════
// RIS IMPORT TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn ris_parse_three_records() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    assert_eq!(result.records.len(), 3, "Expected 3 RIS records");
    assert!(result.errors.is_empty(), "Unexpected parse errors: {:?}", result.errors);
}

#[test]
fn ris_record1_mack_core_fields() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[0];

    assert_some!(rec.reference_type, "JOUR".to_string());
    assert_some!(rec.title, mack::TITLE.to_string());
    assert_some!(rec.journal, mack::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(mack::YEAR));
    assert_some!(rec.volume, mack::VOLUME.to_string());
    assert_some!(rec.issue, mack::ISSUE.to_string());
    assert_some!(rec.doi, mack::DOI.to_string());
    assert_some!(rec.start_page, mack::START_PAGE.to_string());
    assert_eq!(rec.end_page.as_deref(), Some("+"));
    assert_some!(rec.issn, mack::ISSN.to_string());
    assert_some!(rec.accession_number, mack::ACCESSION.to_string());

    // Authors
    assert!(!rec.authors.is_empty(), "Should have authors");
    assert_eq!(rec.authors[0], mack::FIRST_AUTHOR);
    assert_eq!(rec.authors.len(), mack::AUTHOR_COUNT_RIS);

    // Abstract should be non-empty
    assert!(rec.abstract_text.is_some());
    assert!(!rec.abstract_text.as_ref().unwrap().is_empty());

    // Keywords (Keywords-Plus mapped to KW)
    assert_eq!(rec.keywords.len(), mack::KW_COUNT_RIS);
    assert!(rec.keywords.contains(&"CLIMATE-CHANGE".to_string()));
    assert!(rec.keywords.contains(&"SUCCESSION".to_string()));
}

#[test]
fn ris_record1_mack_citation_data() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[0];

    // N1 citation extraction
    assert_eq!(rec.num_cited, Some(mack::NUM_CITED));
    assert_eq!(rec.num_references, Some(mack::NUM_REFERENCES));

    // Notes should contain the raw N1 text
    assert!(rec.notes.is_some());
    let notes = rec.notes.as_ref().unwrap();
    assert!(notes.contains("224"));
}

#[test]
fn ris_record1_mack_extras_and_references() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[0];

    // CR (cited references) should be in extras
    let cr = rec.extras.get("CR");
    assert!(cr.is_some(), "CR should be in extras");
    let cr_entries = cr.unwrap();
    assert!(cr_entries.len() > 50, "Should have many cited references, got {}", cr_entries.len());

    // Verify some specific cited references
    assert!(cr_entries.iter().any(|r| r.contains("Johnstone JF") && r.contains("2010")));
    assert!(cr_entries.iter().any(|r| r.contains("Walker XJ") && r.contains("2019")));
}

#[test]
fn ris_record2_chen_core_fields() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[1];

    assert_some!(rec.title, chen::TITLE.to_string());
    assert_some!(rec.journal, chen::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(chen::YEAR));
    assert_some!(rec.volume, chen::VOLUME.to_string());
    assert_some!(rec.issue, chen::ISSUE.to_string());
    assert_some!(rec.doi, chen::DOI.to_string());
    assert_some!(rec.start_page, chen::START_PAGE.to_string());
    assert_some!(rec.issn, chen::ISSN.to_string());
    assert_some!(rec.accession_number, chen::ACCESSION.to_string());
    assert_eq!(rec.authors[0], chen::FIRST_AUTHOR);
    assert_eq!(rec.authors.len(), chen::AUTHOR_COUNT_RIS);

    // N1 citation data
    assert_eq!(rec.num_cited, Some(chen::NUM_CITED));
    assert_eq!(rec.num_references, Some(chen::NUM_REFERENCES));

    // C6 (EarlyAccessDate) should be in extras
    let c6 = rec.extras.get("C6");
    assert!(c6.is_some(), "C6 EarlyAccessDate should be in extras");
    assert!(c6.unwrap().iter().any(|v| v.contains("APR 2021")));
}

#[test]
fn ris_record3_brodie_core_fields() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[2];

    assert_some!(rec.title, brodie::TITLE.to_string());
    assert_some!(rec.journal, brodie::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(brodie::YEAR));
    assert_some!(rec.volume, brodie::VOLUME.to_string());
    assert_some!(rec.issue, brodie::ISSUE.to_string());
    assert_some!(rec.doi, brodie::DOI.to_string());
    assert_some!(rec.accession_number, brodie::ACCESSION.to_string());
    assert_eq!(rec.authors[0], brodie::FIRST_AUTHOR);
    assert_eq!(rec.authors.len(), brodie::AUTHOR_COUNT_RIS);

    // C7 article-number = 17 — used as start_page since SP is absent
    // In RIS, this record has C7=17 but no SP, so start_page should be Some("17")
    assert_some!(rec.start_page, brodie::START_PAGE.to_string());

    // N1 citation data
    assert_eq!(rec.num_cited, Some(brodie::NUM_CITED));
    assert_eq!(rec.num_references, Some(brodie::NUM_REFERENCES));

    // Keywords should include both author keywords and Keywords-Plus
    assert_eq!(rec.keywords.len(), brodie::KW_COUNT_RIS);
    assert!(rec.keywords.contains(&"Thinning".to_string()));
    assert!(rec.keywords.contains(&"MIXED-CONIFER FOREST".to_string()));
}

#[test]
fn ris_author_address_populated() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");

    // All 3 records should have author addresses
    for (i, rec) in result.records.iter().enumerate() {
        assert!(
            rec.author_address.is_some(),
            "Record {} should have author_address (AD field)",
            i + 1
        );
        let addr = rec.author_address.as_ref().unwrap();
        assert!(!addr.is_empty(), "Record {} AD should not be empty", i + 1);
    }
}

#[test]
fn ris_all_records_have_cited_references_in_extras() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");

    let expected_cr_counts = [mack::NUM_REFERENCES, chen::NUM_REFERENCES, brodie::NUM_REFERENCES];
    for (i, rec) in result.records.iter().enumerate() {
        let cr = rec
            .extras
            .get("CR")
            .unwrap_or_else(|| panic!("Record {} should have CR entries in extras", i + 1));
        assert_eq!(cr.len(), expected_cr_counts[i] as usize, "Record {} CR count mismatch", i + 1);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// BIBTEX IMPORT TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn bibtex_parse_three_entries() {
    let content = read_asset("ExampleWReferences.bib");
    let result = parse_bibtex(&content);
    assert_eq!(result.entries.len(), 3, "Expected 3 BibTeX entries");
    assert!(result.errors.is_empty(), "Unexpected parse errors: {:?}", result.errors);
}

#[test]
fn bibtex_convert_all_entries() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    assert_eq!(records.len(), 3);
}

#[test]
fn bibtex_record1_mack_core_fields() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0];

    assert_some!(rec.reference_type, "article".to_string());
    // Title may contain newlines from BibTeX wrapping — normalize for comparison
    let title_norm = rec
        .title
        .as_deref()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    assert_eq!(title_norm, mack::TITLE);
    assert_some!(rec.journal, mack::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(mack::YEAR));
    assert_some!(rec.volume, mack::VOLUME.to_string());
    assert_some!(rec.issue, mack::ISSUE.to_string());
    assert_some!(rec.doi, mack::DOI.to_string());
    assert_some!(rec.issn, mack::ISSN.to_string());

    // Pages = "280+" — kept as-is (no stripping of '+')
    assert_some!(rec.start_page, "280+".to_string());
    assert_eq!(rec.end_page, None, "Pages '280+' should not produce an end_page");

    // Authors split by " and " (may be 6 or 7 depending on multi-line handling)
    assert!(!rec.authors.is_empty());
    assert!(rec.authors[0].contains("Mack"));
    assert!(rec.authors[0].contains("Michelle"));
    assert!(rec.authors.len() >= 6, "Expected at least 6 authors, got {}", rec.authors.len());

    // Abstract
    assert!(rec.abstract_text.is_some());
    assert!(!rec.abstract_text.as_ref().unwrap().is_empty());

    // Month field is consumed by the BibTeX converter as a recognized field.
    // The converter handles it internally; we just verify the record parsed correctly.
}

#[test]
fn bibtex_record1_mack_extras() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0];

    // BibTeX metadata preserved
    assert_eq!(rec.extras.get("_bibtex_type").map(|v| &v[0]), Some(&"article".to_string()));
    assert_eq!(
        rec.extras.get("_bibtex_key").map(|v| &v[0]),
        Some(&"WOS:000641286700038".to_string())
    );

    // times-cited → num_cited (mapped to record field, NOT in extras)
    assert_eq!(rec.num_cited, Some(224), "times-cited should map to num_cited");
    assert!(!rec.extras.contains_key("times-cited"), "times-cited should NOT be in extras");

    // number-of-cited-references → num_references (mapped to record field, NOT in extras)
    assert_eq!(
        rec.num_references,
        Some(75),
        "number-of-cited-references should map to num_references"
    );
    assert!(
        !rec.extras.contains_key("number-of-cited-references"),
        "number-of-cited-references should NOT be in extras"
    );

    // Cited-References is normalized to "CR" in extras, split into individual lines
    let cr = rec.extras.get("CR");
    assert!(
        cr.is_some(),
        "CR should be in extras (normalized from cited-references). Keys: {:?}",
        rec.extras.keys().collect::<Vec<_>>()
    );
    let cr_entries = cr.unwrap();
    assert!(!cr_entries.is_empty(), "CR entries should not be empty");
    assert!(cr_entries.len() > 50, "Should have many CR entries, got {}", cr_entries.len());

    // Old lowercase key should NOT be present
    assert!(
        !rec.extras.contains_key("cited-references"),
        "cited-references should be normalized to CR, not kept as-is"
    );

    // Affiliation should be mapped to the record's affiliation field (not in extras)
    assert!(rec.affiliation.is_some(), "affiliation should be mapped to record.affiliation field");

    // Keywords-Plus
    let kp = rec.extras.get("keywords-plus");
    assert!(kp.is_some(), "keywords-plus should be in extras");
    assert!(kp.unwrap()[0].contains("CLIMATE-CHANGE"));

    // Usage counts
    assert!(rec.extras.contains_key("usage-count-last-180-days"));
    assert!(rec.extras.contains_key("usage-count-since-2013"));

    // ESI fields
    assert!(rec.extras.contains_key("esi-highly-cited-paper"));
    assert_eq!(rec.extras["esi-highly-cited-paper"][0], "Y");
    assert!(rec.extras.contains_key("esi-hot-paper"));
    assert_eq!(rec.extras["esi-hot-paper"][0], "N");
}

#[test]
fn bibtex_record2_chen_core_fields() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[1];

    let title_norm = rec
        .title
        .as_deref()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    assert_eq!(title_norm, chen::TITLE);
    assert_some!(rec.journal, chen::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(chen::YEAR));
    assert_some!(rec.volume, chen::VOLUME.to_string());
    assert_some!(rec.issue, chen::ISSUE.to_string());
    assert_some!(rec.doi, chen::DOI.to_string());

    // Pages = "404+" — kept as-is
    assert_some!(rec.start_page, "404+".to_string());
    assert_eq!(rec.end_page, None);

    // EISSN should be in extras (ISSN field maps to issn)
    assert_some!(rec.issn, chen::ISSN.to_string());

    // Authors (BibTeX may have more than RIS due to multi-line handling)
    assert!(rec.authors[0].contains("Chen"));
    assert!(rec.authors.len() >= 7, "Expected at least 7 authors, got {}", rec.authors.len());
}

#[test]
fn bibtex_record3_brodie_core_fields() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[2];

    let title_norm = rec
        .title
        .as_deref()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    assert_eq!(title_norm, brodie::TITLE);
    assert_some!(rec.journal, brodie::JOURNAL.to_string());
    assert_eq!(rec.publication_year, Some(brodie::YEAR));
    assert_some!(rec.volume, brodie::VOLUME.to_string());
    assert_some!(rec.issue, brodie::ISSUE.to_string());
    assert_some!(rec.doi, brodie::DOI.to_string());
    assert_some!(rec.issn, brodie::ISSN.to_string());

    // BibTeX doesn't have C7; no pages field in this entry
    assert_eq!(rec.start_page, None, "BibTeX entry has no pages field");
    assert_eq!(rec.end_page, None);

    // Author keywords are consumed into record.keywords (split by semicolons)
    // The 'keywords' field itself does NOT go to extras since it's a recognized field
    assert!(
        rec.keywords.iter().any(|k| k.contains("Thinning")),
        "Thinning should be in keywords. Got: {:?}",
        rec.keywords
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// CROSS-FORMAT EQUIVALENCE
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn cross_format_core_fields_match() {
    let ris_content = read_asset("ExampleWReferences.ris");
    let bib_content = read_asset("ExampleWReferences.bib");

    let ris_result = parse_ris(&ris_content).expect("RIS parsing failed");
    let bib_parse = parse_bibtex(&bib_content);
    let bib_records = convert_bibtex_entries(&bib_parse.entries);

    assert_eq!(ris_result.records.len(), bib_records.len());

    let expected = [
        (mack::TITLE, mack::DOI, mack::YEAR),
        (chen::TITLE, chen::DOI, chen::YEAR),
        (brodie::TITLE, brodie::DOI, brodie::YEAR),
    ];

    for (i, (title, doi, year)) in expected.iter().enumerate() {
        let ris = &ris_result.records[i];
        let bib = &bib_records[i];

        // DOI should match exactly
        assert_eq!(ris.doi.as_deref(), Some(*doi), "RIS DOI mismatch for record {}", i + 1);
        assert_eq!(bib.doi.as_deref(), Some(*doi), "BibTeX DOI mismatch for record {}", i + 1);

        // Year should match
        assert_eq!(ris.publication_year, Some(*year), "RIS year mismatch");
        assert_eq!(bib.publication_year, Some(*year), "BibTeX year mismatch");

        // Title should contain the same core text (whitespace may differ)
        let ris_title = ris.title.as_deref().unwrap_or_default();
        let bib_title = bib.title.as_deref().unwrap_or_default();
        assert!(
            ris_title.contains(&title.to_string().replace("  ", " ").trim())
                || bib_title.contains(&title.to_string().replace("  ", " ").trim()),
            "Titles should match for record {}\n  RIS: {}\n  BIB: {}",
            i + 1,
            ris_title,
            bib_title
        );

        // Both should have authors
        assert!(!ris.authors.is_empty(), "RIS should have authors for record {}", i + 1);
        assert!(!bib.authors.is_empty(), "BibTeX should have authors for record {}", i + 1);

        // Both should have abstracts
        assert!(ris.abstract_text.is_some(), "RIS should have abstract for record {}", i + 1);
        assert!(bib.abstract_text.is_some(), "BibTeX should have abstract for record {}", i + 1);
    }
}

#[test]
fn cross_format_journal_volume_issue_match() {
    let ris_content = read_asset("ExampleWReferences.ris");
    let bib_content = read_asset("ExampleWReferences.bib");

    let ris_result = parse_ris(&ris_content).expect("RIS parsing failed");
    let bib_parse = parse_bibtex(&bib_content);
    let bib_records = convert_bibtex_entries(&bib_parse.entries);

    let expected = [
        (mack::JOURNAL, mack::VOLUME, mack::ISSUE),
        (chen::JOURNAL, chen::VOLUME, chen::ISSUE),
        (brodie::JOURNAL, brodie::VOLUME, brodie::ISSUE),
    ];

    for (i, (journal, volume, issue)) in expected.iter().enumerate() {
        let ris = &ris_result.records[i];
        let bib = &bib_records[i];

        // Journal should match (case-sensitive)
        assert_eq!(ris.journal.as_deref(), Some(*journal));
        assert_eq!(bib.journal.as_deref(), Some(*journal));

        // Volume and issue should match exactly
        assert_eq!(ris.volume.as_deref(), Some(*volume));
        assert_eq!(bib.volume.as_deref(), Some(*volume));
        assert_eq!(ris.issue.as_deref(), Some(*issue));
        assert_eq!(bib.issue.as_deref(), Some(*issue));
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// FALLBACK BEHAVIOUR TESTS
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn bibtex_unrecognized_fields_go_to_extras() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0]; // Mack record

    // These WoS-specific fields should all be in extras (keys are lowercase)
    let extra_fields =
        ["type", "author-email", "research-areas", "doc-delivery-number", "unique-id", "da"];

    for field in &extra_fields {
        assert!(
            rec.extras.contains_key(*field),
            "Expected '{}' in extras, but not found. Extras keys: {:?}",
            field,
            rec.extras.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn bibtex_eissn_fallback_to_extras() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0]; // Mack record has both ISSN and EISSN

    // ISSN maps to the issn field
    assert_some!(rec.issn, mack::ISSN.to_string());

    // EISSN is now a first-class field (mapped from BibTeX EISSN)
    assert_eq!(
        rec.eissn.as_deref(),
        Some("1095-9203"),
        "eissn should be populated from EISSN field"
    );
    // It should NOT be in extras since it's now properly extracted
    assert!(
        !rec.extras.contains_key("eissn"),
        "eissn should not be in extras since it's a first-class field"
    );
}

#[test]
fn bibtex_pages_with_plus_handled() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);

    // Record 1: Pages = "280+" → start_page = "280+" (not stripped), end_page = None
    assert_eq!(records[0].start_page.as_deref(), Some("280+"));
    assert_eq!(records[0].end_page, None);

    // Record 2: Pages = "404+" → start_page = "404+" (not stripped), end_page = None
    assert_eq!(records[1].start_page.as_deref(), Some("404+"));
    assert_eq!(records[1].end_page, None);
}

#[test]
fn bibtex_multiple_sn_fields_kept() {
    // BibTeX doesn't have the SN tag, but verify ISSN/ISBN fallback behavior
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);

    // Record 1 has ISSN = "0036-8075"
    assert_eq!(records[0].issn.as_deref(), Some("0036-8075"));
    // Record 3 has ISSN = "1933-9747"
    assert_eq!(records[2].issn.as_deref(), Some("1933-9747"));
}

#[test]
fn ris_c7_article_number_as_start_page() {
    let content = read_asset("ExampleWReferences.ris");
    let result = parse_ris(&content).expect("RIS parsing failed");
    let rec = &result.records[2]; // Brodie record has C7=17, no SP

    // C7 should be used as start_page when SP is absent
    assert_some!(rec.start_page, "17".to_string());
}

// ══════════════════════════════════════════════════════════════════════════════
// BIBTEX CR PARSING (CITED REFERENCES FROM WOS BIBTEX)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn bibtex_cr_entries_split_by_newline_not_period() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0]; // Mack — has 75 cited references

    let cr = rec.extras.get("CR").expect("CR should be in extras");

    // With newline-based splitting, we should get exactly 75 entries (one per line)
    // NOT the thousands of fragments that period-based splitting would produce
    assert_eq!(
        cr.len(),
        mack::NUM_REFERENCES as usize,
        "Mack record should have {} CR entries (one per line), got {}",
        mack::NUM_REFERENCES,
        cr.len()
    );

    // Verify no DOI fragments — a strong signal that period-splitting is NOT used
    for (i, entry) in cr.iter().enumerate() {
        // A legitimate CR entry should contain a comma (WoS format: Author, Year, ...)
        // If it's a period-split fragment like "04" or "005", it won't have a comma
        assert!(
            entry.contains(','),
            "CR entry {} should contain a comma (full WoS line), got: '{}'",
            i,
            entry
        );
    }
}

#[test]
fn bibtex_cr_entries_preserve_full_dois() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0]; // Mack

    let cr = rec.extras.get("CR").expect("CR should be in extras");

    // Find the Alexander entry — it has DOI 10.1016/j.foreco.2017.04.005
    let alexander = cr.iter().find(|r| r.contains("Alexander HD") && r.contains("2017"));
    assert!(alexander.is_some(), "Should find Alexander HD 2017 in CR entries");
    let entry = alexander.unwrap();

    // The full DOI should be intact in the entry (not broken by period splitting)
    assert!(
        entry.contains("10.1016/j.foreco.2017.04.005"),
        "Full DOI should be preserved in CR entry: '{}'",
        entry
    );
}

#[test]
fn bibtex_cr_all_records_have_correct_count() {
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);

    let expected_counts = [
        mack::NUM_REFERENCES as usize,
        chen::NUM_REFERENCES as usize,
        brodie::NUM_REFERENCES as usize,
    ];

    for (i, rec) in records.iter().enumerate() {
        let cr = rec
            .extras
            .get("CR")
            .unwrap_or_else(|| panic!("Record {} should have CR entries in extras", i + 1));
        assert_eq!(
            cr.len(),
            expected_counts[i],
            "Record {} CR count: expected {}, got {}",
            i + 1,
            expected_counts[i],
            cr.len()
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CR PARSER INTEGRATION (parse_cr_line on real BibTeX CR data)
// ══════════════════════════════════════════════════════════════════════════════

use bango_lib::ris::cr_parser::parse_cr_line;

#[test]
fn cr_parser_parses_real_bibtex_standard_entry() {
    // Real line from Mack 2021 BibTeX: Alexander with DOI
    let line =
        "Alexander HD, 2017, FOREST ECOL MANAG, V396, P35, DOI 10.1016/j.foreco.2017.04.005.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Alexander HD"]);
    assert_eq!(paper.publication_year, Some(2017));
    assert_eq!(paper.journal.as_deref(), Some("FOREST ECOL MANAG"));
    assert_eq!(paper.volume.as_deref(), Some("396"));
    assert_eq!(paper.start_page.as_deref(), Some("35"));
    assert_eq!(paper.doi.as_deref(), Some("10.1016/j.foreco.2017.04.005"));
    assert!(paper.title.is_some());
}

#[test]
fn cr_parser_parses_real_bibtex_doi_array_entry() {
    // Real line with {*}{*}DATA OBJECT{*}{*} and DOI array
    let line = "Alexander H. D., 2021, {*}{*}DATA OBJECT{*}{*}, DOI {[}10.6073/pasta/7367d64e999c830a508a7e012ad0824c, DOI 10.6073/PASTA/7367D64E999C830A508A7E012AD0824C].";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Alexander H. D."]);
    assert_eq!(paper.publication_year, Some(2021));
    // "DATA OBJECT" is ALL CAPS → treated as journal; title is auto-constructed
    assert_eq!(paper.journal.as_deref(), Some("DATA OBJECT"));
    assert_eq!(paper.doi.as_deref(), Some("10.6073/pasta/7367d64e999c830a508a7e012ad0824c"));
    assert!(paper.title.is_some());
    assert!(paper.title.as_ref().unwrap().contains("Alexander H. D."));
}

#[test]
fn cr_parser_parses_real_bibtex_anonymous_book() {
    let line = "{[}Anonymous], 1978, Canadian System of Soil Classification.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["[Anonymous]"]);
    assert_eq!(paper.publication_year, Some(1978));
    assert_eq!(paper.title.as_deref(), Some("Canadian System of Soil Classification"));
    assert!(paper.journal.is_none());
}

#[test]
fn cr_parser_parses_real_bibtex_minimal_entry() {
    let line = "Barton Kamil, 2024, CRAN.";
    let paper = parse_cr_line(line).unwrap();
    assert_eq!(paper.authors, vec!["Barton Kamil"]);
    assert_eq!(paper.publication_year, Some(2024));
    assert_eq!(paper.journal.as_deref(), Some("CRAN"));
}

#[test]
fn cr_parser_handles_all_real_bibtex_cr_lines() {
    // Parse ALL CR lines from the Mack record and ensure every one succeeds
    let content = read_asset("ExampleWReferences.bib");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let rec = &records[0];

    let cr = rec.extras.get("CR").expect("CR should be in extras");
    let mut parsed_count = 0;
    let mut failed_lines: Vec<String> = Vec::new();

    for line in cr {
        match parse_cr_line(line) {
            Some(paper) => {
                parsed_count += 1;
                // Every parsed paper must have at least a title and author
                assert!(paper.title.is_some(), "Parsed paper should have a title");
                assert!(!paper.authors.is_empty(), "Parsed paper should have authors");
            }
            None => failed_lines.push(line.clone()),
        }
    }

    assert!(
        failed_lines.is_empty(),
        "All CR lines should parse. Failed:\n  - {}",
        failed_lines.join("\n  - ")
    );
    assert_eq!(parsed_count, mack::NUM_REFERENCES as usize);
}
