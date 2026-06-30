//! T1.3 tests for section-aware AI summary prompt helpers.
//!
//! Covers:
//! - `filter_high_value_sections`: keeps only Methods/Results/Discussion.
//! - `build_section_context`: renders delimited blocks, skips empty bodies.
//! - `ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT`: content guard for the
//!   section-aware variant (must request the `section_summaries` schema).
//! - `SectionKind::label`: stable display strings for prompt + UI rendering.
//! - JSON backward-compat: a v1 blob (no `section_summaries`) still parses as
//!   valid JSON through the same validation path used by the command.
//!
//! The `generate_article_ai_summary` command itself is an async Tauri command
//! with orchestrator + DB state dependencies and is exercised end-to-end via
//! tauri-pilot. These tests target the pure helpers and prompt content, which
//! is where regressions most commonly surface.

use bango_lib::summary::prompt::{
    build_figure_description_prompt, build_section_context, ensure_schema_version_v2,
    filter_high_value_sections, merge_figure_descriptions_into_blob,
    parse_figure_descriptions_response, FigureDescription,
    ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT,
};
use bango_lib::utils::sections::{classify_sections, Caption, CaptionKind, Section, SectionKind};

// ── filter_high_value_sections ───────────────────────────────────────────────

#[test]
fn filter_keeps_only_high_value_kinds() {
    let text = "\
Introduction

Some intro text.

Methods

We did a thing.

Results

We found a thing.

Discussion

We discuss the thing.

References

[1] A paper.
";
    let sections = classify_sections(text);
    assert!(!sections.is_empty(), "fixture should produce sections");

    let high = filter_high_value_sections(&sections);
    let kinds: Vec<SectionKind> = high.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, vec![SectionKind::Methods, SectionKind::Results, SectionKind::Discussion]);
}

#[test]
fn filter_returns_empty_when_only_text_sections() {
    // No headings at all -> classify_sections yields one Text section.
    let text = "Just a paragraph of body text with no headings.";
    let sections = classify_sections(text);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].kind, SectionKind::Text);

    let high = filter_high_value_sections(&sections);
    assert!(high.is_empty(), "no high-value sections when only Text present");
}

#[test]
fn filter_returns_empty_for_empty_input() {
    let high = filter_high_value_sections(&[]);
    assert!(high.is_empty());
}

#[test]
fn filter_excludes_introduction_conclusion_abstract() {
    let text = "\
Abstract

An abstract.

Introduction

An intro.

Conclusion

A conclusion.
";
    let sections = classify_sections(text);
    let high = filter_high_value_sections(&sections);
    assert!(high.is_empty(), "Abstract/Introduction/Conclusion are not high-value");
}

// ── build_section_context ────────────────────────────────────────────────────

#[test]
fn build_section_context_renders_each_section_with_delimiter() {
    let sections = vec![
        Section {
            kind: SectionKind::Methods,
            heading: Some("Methods".to_string()),
            body: "We recruited 100 participants.".to_string(),
            word_count: 4,
        },
        Section {
            kind: SectionKind::Results,
            heading: Some("Results".to_string()),
            body: "The intervention reduced BMI by 0.4.".to_string(),
            word_count: 6,
        },
    ];
    let ctx = build_section_context(&sections);
    assert!(ctx.contains("=== SECTION: Methods ==="), "Methods delimiter missing: {ctx}");
    assert!(ctx.contains("We recruited 100 participants."));
    assert!(ctx.contains("=== SECTION: Results ==="), "Results delimiter missing: {ctx}");
    assert!(ctx.contains("The intervention reduced BMI by 0.4."));
}

#[test]
fn build_section_context_skips_empty_bodies() {
    let sections = vec![
        Section {
            kind: SectionKind::Methods,
            heading: Some("Methods".to_string()),
            body: "   ".to_string(),
            word_count: 0,
        },
        Section {
            kind: SectionKind::Results,
            heading: Some("Results".to_string()),
            body: "Real content.".to_string(),
            word_count: 2,
        },
    ];
    let ctx = build_section_context(&sections);
    assert!(!ctx.contains("=== SECTION: Methods ==="), "empty Methods section should be skipped");
    assert!(ctx.contains("=== SECTION: Results ==="));
}

#[test]
fn build_section_context_returns_empty_for_empty_input() {
    assert_eq!(build_section_context(&[]), "");
}

#[test]
fn build_section_context_uses_kind_label_not_heading_text() {
    // Even if the heading text is "2.1 Study Design", the delimiter uses the
    // stable SectionKind label ("Methods") so the prompt contract is stable.
    let sections = vec![Section {
        kind: SectionKind::Methods,
        heading: Some("2.1 Study Design".to_string()),
        body: "We did X.".to_string(),
        word_count: 3,
    }];
    let ctx = build_section_context(&sections);
    assert!(
        ctx.contains("=== SECTION: Methods ==="),
        "delimiter must use the canonical kind label, not the heading text: {ctx}"
    );
    assert!(!ctx.contains("2.1 Study Design"));
}

// ── SectionKind::label ───────────────────────────────────────────────────────

#[test]
fn section_kind_labels_are_stable_strings() {
    assert_eq!(SectionKind::Methods.label(), "Methods");
    assert_eq!(SectionKind::Results.label(), "Results");
    assert_eq!(SectionKind::Discussion.label(), "Discussion");
    assert_eq!(SectionKind::Introduction.label(), "Introduction");
    assert_eq!(SectionKind::Abstract.label(), "Abstract");
    assert_eq!(SectionKind::Conclusion.label(), "Conclusion");
    assert_eq!(SectionKind::References.label(), "References");
    assert_eq!(SectionKind::Heading.label(), "Heading");
    assert_eq!(SectionKind::Text.label(), "Text");
}

// ── Prompt content guard ─────────────────────────────────────────────────────

#[test]
fn section_aware_prompt_is_not_empty_or_placeholder() {
    assert!(
        !ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT.trim().is_empty(),
        "section-aware system prompt must not be empty"
    );
    let lower = ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT.to_lowercase();
    assert!(!lower.contains("placeholder"), "section-aware prompt must not be placeholder text");
}

#[test]
fn section_aware_prompt_documents_schema_keys() {
    let p = ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT;
    // Core keys from the v1 schema must still be present.
    for key in [
        "field",
        "subfield",
        "structured_extraction",
        "summary_150_250_words",
        "key_insights",
        "keywords",
    ] {
        assert!(p.contains(key), "section-aware prompt must document '{key}'");
    }
    // v2 section-summaries keys.
    assert!(p.contains("section_summaries"), "prompt must request section_summaries array");
    assert!(p.contains("schema_version"), "prompt must emit schema_version for versioning");
    assert!(
        p.contains("=== SECTION:"),
        "prompt must teach the model the delimiter format used by build_section_context"
    );
}

#[test]
fn section_aware_prompt_documents_optional_section_fields() {
    let p = ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT;
    // Methods-specific.
    assert!(p.contains("study_design"));
    // Results-specific.
    assert!(p.contains("effect_size"));
    assert!(p.contains("confidence_interval"));
    // Shared.
    assert!(p.contains("key_points"));
}

// ── JSON backward-compatibility ──────────────────────────────────────────────

#[test]
fn v1_summary_blob_without_section_summaries_is_valid_json() {
    // A pre-T1.3 blob has no schema_version and no section_summaries. The
    // command stores `parsed.to_string()`, so we only require that such a blob
    // round-trips through serde_json::Value.
    let v1 = r#"{
        "field": "medicine",
        "subfield": "public_health",
        "structured_extraction": {"population": "adults"},
        "summary_150_250_words": "A short summary.",
        "key_insights": ["insight one"],
        "keywords": ["sugar", "tax"]
    }"#;
    let parsed: serde_json::Value = serde_json::from_str(v1).expect("v1 blob must parse");
    assert!(parsed.get("section_summaries").is_none());
    assert!(parsed.get("schema_version").is_none());
}

#[test]
fn v2_summary_blob_with_section_summaries_is_valid_json() {
    let v2 = r#"{
        "schema_version": 2,
        "field": "medicine",
        "subfield": "public_health",
        "structured_extraction": {},
        "summary_150_250_words": "A short summary.",
        "key_insights": [],
        "keywords": [],
        "section_summaries": [
            {"section": "Methods", "summary": "RCT N=1000.", "key_points": [], "study_design": "Randomized Controlled Trial"},
            {"section": "Results", "summary": "BMI fell 0.4.", "key_points": [], "effect_size": "d=0.2", "confidence_interval": "95% CI [0.1, 0.3]"},
            {"section": "Discussion", "summary": "Policy relevant.", "key_points": []}
        ]
    }"#;
    let parsed: serde_json::Value = serde_json::from_str(v2).expect("v2 blob must parse");
    assert_eq!(parsed.get("schema_version").and_then(|v| v.as_i64()), Some(2));
    let ss = parsed
        .get("section_summaries")
        .and_then(|v| v.as_array())
        .expect("section_summaries must be an array");
    assert_eq!(ss.len(), 3);
    assert_eq!(
        ss[0].get("study_design").and_then(|v| v.as_str()),
        Some("Randomized Controlled Trial")
    );
    assert_eq!(ss[1].get("effect_size").and_then(|v| v.as_str()), Some("d=0.2"));
}

// ── ensure_schema_version_v2 (T1.3 contract: backend guarantees v2) ─────────

#[test]
fn ensure_schema_version_v2_sets_2_when_missing() {
    let mut value = serde_json::json!({
        "summary_150_250_words": "A summary.",
        "section_summaries": []
    });
    ensure_schema_version_v2(&mut value);
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn ensure_schema_version_v2_sets_2_when_older_version_present() {
    let mut value = serde_json::json!({
        "schema_version": 1,
        "summary_150_250_words": "A summary."
    });
    ensure_schema_version_v2(&mut value);
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn ensure_schema_version_v2_leaves_existing_2_alone() {
    let mut value = serde_json::json!({
        "schema_version": 2,
        "summary_150_250_words": "A summary."
    });
    ensure_schema_version_v2(&mut value);
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn ensure_schema_version_v2_noop_on_non_object() {
    // Defensive: a top-level array/string is left untouched (no panic).
    let mut value = serde_json::json!(["not", "an", "object"]);
    ensure_schema_version_v2(&mut value);
    assert!(value.is_array(), "non-object value must be unchanged");
    assert!(value.get("schema_version").is_none());
}

// ── Tier 2 Phase 4: figure/table description pure helpers ───────────────────

fn cap(kind: CaptionKind, number: &str, caption: &str) -> Caption {
    Caption {
        kind,
        number: number.to_string(),
        caption: caption.to_string(),
        following_sentence: None,
    }
}

#[test]
fn build_figure_description_prompt_embeds_title_and_captions() {
    let captions = vec![
        cap(CaptionKind::Figure, "1", "Bar chart of BMI by age group."),
        cap(CaptionKind::Table, "2", "Study characteristics for the sample."),
    ];
    let prompt = build_figure_description_prompt("Sugar Tax Impact Study", &captions);
    assert!(prompt.contains("Sugar Tax Impact Study"), "title must be in prompt: {prompt}");
    assert!(prompt.contains("[Figure 1]"), "figure caption must be labeled: {prompt}");
    assert!(prompt.contains("Bar chart of BMI by age group."));
    assert!(prompt.contains("[Table 2]"), "table caption must be labeled: {prompt}");
    assert!(prompt.contains("Study characteristics for the sample."));
}

#[test]
fn build_figure_description_prompt_uses_placeholder_for_empty_caption() {
    let captions = vec![cap(CaptionKind::Figure, "3", "   ")];
    let prompt = build_figure_description_prompt("Study", &captions);
    assert!(prompt.contains("(no caption text)"), "empty caption should get placeholder");
}

#[test]
fn parse_figure_descriptions_response_extracts_number_description() {
    let response = r#"[
        {"number": "1", "description": "Shows BMI trends over time."},
        {"number": "2a", "description": "Forest plot of effect sizes."}
    ]"#;
    let parsed = parse_figure_descriptions_response(response).expect("valid JSON must parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].number, "1");
    assert_eq!(parsed[0].description, "Shows BMI trends over time.");
    assert_eq!(parsed[1].number, "2a");
}

#[test]
fn parse_figure_descriptions_response_tolerates_code_fences() {
    let response = "```json\n[{\"number\": \"1\", \"description\": \"A chart.\"}]\n```";
    let parsed = parse_figure_descriptions_response(response).expect("fenced JSON must parse");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].number, "1");
}

#[test]
fn parse_figure_descriptions_response_returns_err_on_malformed_json() {
    let result = parse_figure_descriptions_response("not json at all");
    assert!(result.is_err(), "malformed JSON must error, not panic");
}

#[test]
fn parse_figure_descriptions_response_returns_err_on_non_array() {
    let result = parse_figure_descriptions_response(r#"{"number": "1"}"#);
    assert!(result.is_err(), "non-array must error");
}

#[test]
fn parse_figure_descriptions_response_defaults_missing_description() {
    let response = r#"[{"number": "1"}]"#;
    let parsed = parse_figure_descriptions_response(response).expect("must parse");
    assert_eq!(parsed[0].number, "1");
    assert_eq!(parsed[0].description, "", "missing description defaults to empty");
}

#[test]
fn merge_figure_descriptions_into_blob_preserves_existing_fields() {
    let existing = r#"{
        "schema_version": 2,
        "summary_150_250_words": "A summary.",
        "section_summaries": [{"section": "Methods", "summary": "RCT."}]
    }"#;
    let figures = vec![FigureDescription {
        number: "1".to_string(),
        caption: "A chart.".to_string(),
        description: "Shows trends.".to_string(),
    }];
    let merged = merge_figure_descriptions_into_blob(Some(existing), figures, vec![]);
    let value: serde_json::Value = serde_json::from_str(&merged).expect("merged blob must parse");
    // Existing fields preserved.
    assert_eq!(value.get("summary_150_250_words").and_then(|v| v.as_str()), Some("A summary."));
    assert!(value.get("section_summaries").is_some(), "section_summaries must survive the merge");
    // New fields added.
    let figures_arr = value.get("figures").and_then(|v| v.as_array()).expect("figures added");
    assert_eq!(figures_arr.len(), 1);
    assert_eq!(figures_arr[0].get("number").and_then(|v| v.as_str()), Some("1"));
    // Schema version stamped.
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn merge_figure_descriptions_into_blob_adds_figures_and_tables_keys() {
    let existing = r#"{"summary_150_250_words": "x."}"#;
    let figures = vec![FigureDescription {
        number: "1".to_string(),
        caption: "f".to_string(),
        description: "d".to_string(),
    }];
    let tables = vec![FigureDescription {
        number: "2".to_string(),
        caption: "t".to_string(),
        description: "td".to_string(),
    }];
    let merged = merge_figure_descriptions_into_blob(Some(existing), figures, tables);
    let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
    assert!(value.get("figures").is_some());
    assert!(value.get("tables").is_some());
    assert_eq!(value["figures"].as_array().unwrap().len(), 1);
    assert_eq!(value["tables"].as_array().unwrap().len(), 1);
}

#[test]
fn merge_figure_descriptions_into_blob_handles_none_existing() {
    let figures = vec![FigureDescription {
        number: "1".to_string(),
        caption: String::new(),
        description: "desc".to_string(),
    }];
    let merged = merge_figure_descriptions_into_blob(None, figures, vec![]);
    let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
    assert!(value.get("figures").is_some());
}

#[test]
fn merge_figure_descriptions_into_blob_handles_malformed_existing() {
    // A malformed existing blob must NOT panic; it's treated as empty.
    let merged = merge_figure_descriptions_into_blob(Some("not json"), vec![], vec![]);
    let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
    assert_eq!(value.get("schema_version").and_then(|v| v.as_i64()), Some(2));
}
