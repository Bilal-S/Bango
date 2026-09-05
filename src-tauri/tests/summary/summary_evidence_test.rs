//! Tests for the project-wide summary evidence + criteria helpers (Shape 0 + A).
//!
//! Covers the pure functions added in `summary/prompt.rs`:
//! - `format_ai_summary_as_evidence` (Shape A): distills a per-article
//!   `full_text_ai_summary` blob into a compact evidence string.
//! - `format_screening_summary` (Shape 0): renders full criteria definitions
//!   into the Methodology context.
//!
//! Per CLAUDE.md coverage strategy: test the extracted pure logic, not the
//! `#[tauri::command]` shims.

use bango_lib::summary::prompt::{
    format_ai_summary_as_evidence, format_screening_summary, ArticleSummary, ScreeningData,
};

fn empty_screening_data() -> ScreeningData {
    ScreeningData {
        records_identified: 0,
        duplicates_removed: 0,
        records_screened: 0,
        records_excluded: 0,
        records_excluded_with_reasons: 0,
        records_assessed: 0,
        records_in_progress: 0,
        studies_included: 0,
        ai_screened: 0,
        manual_reviewed: 0,
        exclusion_reasons: Vec::new(),
    }
}

// ── format_ai_summary_as_evidence (Shape A) ────────────────────────────────

#[test]
fn evidence_returns_none_for_missing_blob() {
    assert_eq!(format_ai_summary_as_evidence(None), None);
}

#[test]
fn evidence_returns_none_for_malformed_json() {
    // Malformed JSON must not panic; falls back to None.
    assert_eq!(format_ai_summary_as_evidence(Some("not json")), None);
}

#[test]
fn evidence_returns_none_for_empty_object() {
    // An empty object carries no facts -> None.
    assert_eq!(format_ai_summary_as_evidence(Some("{}")), None);
}

#[test]
fn evidence_returns_none_for_object_with_only_empty_fields() {
    // All fields empty -> no usable facts -> None.
    let blob = r#"{"field":"","summary_150_250_words":"","structured_extraction":{}}"#;
    assert_eq!(format_ai_summary_as_evidence(Some(blob)), None);
}

#[test]
fn evidence_extracts_field_and_subfield() {
    let blob = r#"{"field":"medicine","subfield":"public_health"}"#;
    let result = format_ai_summary_as_evidence(Some(blob)).expect("Some for non-empty blob");
    assert!(result.contains("field: medicine"), "result was: {result}");
    assert!(result.contains("subfield: public_health"), "result was: {result}");
}

#[test]
fn evidence_extracts_structured_extraction_known_keys_in_stable_order() {
    let blob = r#"{
        "structured_extraction": {
            "outcomes": "BMI z-score",
            "study_type": "RCT",
            "population": "N=1234 children"
        }
    }"#;
    let result = format_ai_summary_as_evidence(Some(blob)).expect("Some for facts");
    // Known keys appear in the canonical `known_order`, not insertion order:
    // study_type before population before outcomes.
    let study_idx = result.find("study_type:").expect("study_type present");
    let pop_idx = result.find("population:").expect("population present");
    let out_idx = result.find("outcomes:").expect("outcomes present");
    assert!(study_idx < pop_idx, "study_type before population");
    assert!(pop_idx < out_idx, "population before outcomes");
    assert!(result.contains("study_type: RCT"));
    assert!(result.contains("population: N=1234 children"));
    assert!(result.contains("outcomes: BMI z-score"));
}

#[test]
fn evidence_extracts_unknown_structured_extraction_keys() {
    // Forward-compatible: unknown string-valued keys are emitted after known.
    let blob = r#"{"structured_extraction":{"novel_metric":"0.42"}}"#;
    let result = format_ai_summary_as_evidence(Some(blob)).expect("Some for unknown key");
    assert!(result.contains("novel_metric: 0.42"), "result was: {result}");
}

#[test]
fn evidence_skips_empty_string_values_in_extraction() {
    let blob = r#"{"structured_extraction":{"study_type":"","population":"N=100"}}"#;
    let result = format_ai_summary_as_evidence(Some(blob)).expect("Some for non-empty pop");
    // Empty study_type is skipped; population is emitted.
    assert!(!result.contains("study_type:"), "empty value skipped");
    assert!(result.contains("population: N=100"));
}

#[test]
fn evidence_truncates_long_digest_to_600_chars() {
    let long_digest = "a".repeat(1000);
    let blob = format!(r#"{{"summary_150_250_words":"{long_digest}"}}"#);
    let result = format_ai_summary_as_evidence(Some(&blob)).expect("Some for digest");
    // The digest portion is truncated; the "digest: " prefix + 600 chars.
    assert!(result.contains("digest: "));
    // The full 1000-char digest must NOT appear (truncated).
    assert!(!result.contains(&"a".repeat(601)), "digest truncated at 600 chars");
}

#[test]
fn evidence_joins_all_parts_with_semicolons() {
    let blob = r#"{
        "field":"medicine",
        "structured_extraction":{"study_type":"RCT"},
        "summary_150_250_words":"A short digest."
    }"#;
    let result = format_ai_summary_as_evidence(Some(blob)).expect("Some");
    // All three parts present, semicolon-separated.
    assert!(result.contains("field: medicine"));
    assert!(result.contains("study_type: RCT"));
    assert!(result.contains("digest: A short digest."));
    assert!(result.contains("; "), "parts joined with semicolons");
}

// ── format_screening_summary (Shape 0 criteria rendering) ──────────────────

#[test]
fn screening_summary_omits_criteria_lines_when_lists_empty() {
    // Backward compat: empty criteria -> no criteria lines in the output.
    let data = empty_screening_data();
    let out = format_screening_summary(&data, &[], &[]);
    assert!(!out.contains("Inclusion criteria:"), "no inclusion section when empty");
    assert!(!out.contains("Exclusion criteria:"), "no exclusion section when empty");
}

#[test]
fn screening_summary_renders_inclusion_criteria() {
    let data = empty_screening_data();
    let inclusion = vec!["Studies of SSB taxes".to_string(), "Children aged 5-11".to_string()];
    let out = format_screening_summary(&data, &inclusion, &[]);
    assert!(out.contains("Inclusion criteria:"), "inclusion header present");
    assert!(out.contains("- Studies of SSB taxes"), "first inclusion rendered");
    assert!(out.contains("- Children aged 5-11"), "second inclusion rendered");
}

#[test]
fn screening_summary_renders_exclusion_criteria() {
    let data = empty_screening_data();
    let exclusion = vec!["Non-English papers".to_string()];
    let out = format_screening_summary(&data, &[], &exclusion);
    assert!(out.contains("Exclusion criteria:"), "exclusion header present");
    assert!(out.contains("- Non-English papers"), "exclusion rendered");
    assert!(!out.contains("Inclusion criteria:"), "no inclusion section when empty");
}

#[test]
fn screening_summary_renders_both_criteria_lists() {
    let data = empty_screening_data();
    let inclusion = vec!["Inclusion rule".to_string()];
    let exclusion = vec!["Exclusion rule".to_string()];
    let out = format_screening_summary(&data, &inclusion, &exclusion);
    assert!(out.contains("Inclusion criteria:"));
    assert!(out.contains("- Inclusion rule"));
    assert!(out.contains("Exclusion criteria:"));
    assert!(out.contains("- Exclusion rule"));
}

// ── ArticleSummary.evidence field round-trip ───────────────────────────────

#[test]
fn article_summary_evidence_field_defaults_none_when_not_set() {
    // Construct with evidence: None explicitly (the default for abstract-only).
    let a = ArticleSummary {
        title: "T".to_string(),
        authors: vec!["A".to_string()],
        year: Some(2024),
        abstract_text: "AB".to_string(),
        keywords: vec![],
        evidence: None,
    };
    assert!(a.evidence.is_none());
}

#[test]
fn article_summary_evidence_field_carries_distilled_string() {
    let a = ArticleSummary {
        title: "T".to_string(),
        authors: vec![],
        year: None,
        abstract_text: String::new(),
        keywords: vec![],
        evidence: Some("field: medicine; study_type: RCT".to_string()),
    };
    assert_eq!(a.evidence.as_deref(), Some("field: medicine; study_type: RCT"));
}
