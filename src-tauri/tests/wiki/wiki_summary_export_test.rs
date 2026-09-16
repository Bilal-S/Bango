//! Binding inventory tests for the wiki summary-export change
//! (`.worktrees/wikifix-final.md` Change 1): article raw export renders the
//! full unified AI-summary blob as structured Markdown and never exports
//! article full text.
//!
//! Inventory: `docs/test-plans/wiki-output-budget-tests.md`.

use bango_lib::models::article::Article;
use bango_lib::wiki::raw_export::{article_content, write_article_exports};
use tempfile::TempDir;

/// Minimal included article (mirrors the inline `sample_article` helper).
fn sample_article() -> Article {
    Article {
        id: "art-1".to_string(),
        sequence_id: 1,
        status: bango_lib::models::article::ArticleStatus::Included,
        screening_error: false,
        title: "Sample".to_string(),
        abstract_text: "the abstract".to_string(),
        authors: vec!["Doe, J".to_string()],
        publication_year: Some(2024),
        doi: Some("10.1/x".to_string()),
        journal: Some("Nature".to_string()),
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec!["tag1".to_string()],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        eissn: None,
        journal_index_id: None,
        reference_type: None,
        date: None,
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        user_notes: None,
        ris_extras: None,
        duplicate_of: None,
        ai_decision: None,
        ai_reasoning: None,
        ai_confidence: None,
        matched_inclusion_criteria: Vec::new(),
        matched_exclusion_criteria: Vec::new(),
        tags: Vec::new(),
        labels: Vec::new(),
        manual_override: false,
        import_source: None,
        imported_at: "2024-01-01T00:00:00Z".to_string(),
        changed_at: "2024-01-01T00:00:00Z".to_string(),
        screened_at: None,
        data_length: None,
        token_estimate: None,
        actual_tokens: None,
        full_text: None,
        full_text_ai_summary: None,
        num_cited: None,
        num_references: None,
        has_citation_details: false,
        has_reference_details: false,
        has_full_text: false,
        full_text_file_name: None,
        has_figures_or_tables: false,
        is_translated: false,
        translation_status: "none".to_string(),
        translation_error: None,
        translated_at: None,
    }
}

#[test]
fn article_content_renders_full_summary_blob_and_never_full_text() {
    let mut a = sample_article();
    a.full_text = Some("FULL TEXT BODY THAT MUST NEVER LEAK".to_string());
    a.full_text_ai_summary = Some(
        r#"{"summary_150_250_words":"digest words","key_insights":["insight one"]}"#.to_string(),
    );
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "ai_summary");
    assert!(content.contains("## Summary\n\ndigest words"), "{content}");
    assert!(content.contains("- insight one"), "{content}");
    assert!(!content.contains("FULL TEXT BODY"), "full text leaked: {content}");
}

#[test]
fn article_content_renders_section_summaries_and_typed_facts() {
    let mut a = sample_article();
    a.full_text_ai_summary = Some(
        r#"{"summary_150_250_words":"digest","section_summaries":[{"section":"Methods","summary":"We ran a trial.","key_points":["randomised"],"study_design":"RCT","sample_size":"120","effect_size":"d=0.4","confidence_interval":"0.1 to 0.7"}]}"#
            .to_string(),
    );
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "ai_summary");
    assert!(content.contains("### Methods"), "{content}");
    assert!(content.contains("We ran a trial."), "{content}");
    assert!(content.contains("- randomised"), "{content}");
    assert!(content.contains("- Study design: RCT"), "{content}");
    assert!(content.contains("- Sample size: 120"), "{content}");
    assert!(content.contains("- Effect size: d=0.4"), "{content}");
    assert!(content.contains("- 95% CI: 0.1 to 0.7"), "{content}");
}

#[test]
fn article_content_falls_back_to_abstract_without_blob() {
    let mut a = sample_article();
    a.full_text = Some("full body".to_string());
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "abstract");
    assert_eq!(content, "the abstract");
}

#[test]
fn render_summary_blob_includes_theoretical_frameworks() {
    let mut a = sample_article();
    a.full_text_ai_summary = Some(
        r#"{"summary_150_250_words":"digest","theoretical_frameworks":[{"name":"TPB","usage":"Predicts intention."}]}"#
            .to_string(),
    );
    let (content, kind) = article_content(&a);
    assert_eq!(kind, "ai_summary");
    assert!(content.contains("## Theoretical Frameworks"), "{content}");
    assert!(content.contains("- TPB: Predicts intention."), "{content}");
}

#[test]
fn raw_export_body_carries_blob_content_not_full_text() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let mut a = sample_article();
    a.full_text = Some("FULL TEXT BODY THAT MUST NEVER LEAK".to_string());
    a.full_text_ai_summary = Some(r#"{"summary_150_250_words":"digest words"}"#.to_string());
    let report = write_article_exports(root, std::slice::from_ref(&a), None, None).unwrap();
    assert_eq!(report.articles_written, 1);
    let raw = std::fs::read_to_string(root.join("raw/art-1.md")).unwrap();
    assert!(raw.contains("content_source: ai_summary"), "{raw}");
    assert!(raw.contains("## Summary\n\ndigest words"), "{raw}");
    assert!(!raw.contains("FULL TEXT BODY"), "full text leaked into export: {raw}");
}
