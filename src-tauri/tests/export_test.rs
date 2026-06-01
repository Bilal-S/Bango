use bango_lib::export::ris_writer::{article_to_ris, articles_to_ris, RisExportArticle};
use bango_lib::ris::parser::parse_ris;

use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

fn make_export_article() -> RisExportArticle {
    RisExportArticle {
        reference_type: Some("JOUR".to_string()),
        title: "Machine Learning for Reviews".to_string(),
        abstract_text: "Abstract text about ML.".to_string(),
        authors: vec!["Smith, John".to_string(), "Doe, Jane".to_string()],
        publication_year: Some(2023),
        doi: Some("10.1234/test".to_string()),
        journal: Some("J Med Inform".to_string()),
        volume: Some("120".to_string()),
        issue: Some("3".to_string()),
        start_page: Some("45".to_string()),
        end_page: Some("58".to_string()),
        keywords: vec!["ml".to_string()],
        tags: vec!["machine-learning".to_string()],
        url: Some("https://example.com".to_string()),
        language: Some("English".to_string()),
        publisher: Some("Elsevier".to_string()),
        issn: Some("1234-5678".to_string()),
        ai_reasoning: Some("Article meets criteria.".to_string()),
        user_notes: None,
        ai_decision: Some("include".to_string()),
        labels: vec!["priority-read".to_string(), "strong-methodology".to_string()],
        matched_inclusion_criteria: vec!["uses RCT methodology".to_string()],
        matched_exclusion_criteria: vec!["non-English language".to_string()],
    }
}

#[test]
fn test_article_to_ris_basic_fields() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.starts_with("TY  - JOUR"));
    assert!(ris.contains("TI  - Machine Learning for Reviews"));
    assert!(ris.contains("AB  - Abstract text about ML."));
    assert!(ris.contains("AU  - Smith, John"));
    assert!(ris.contains("AU  - Doe, Jane"));
    assert!(ris.contains("PY  - 2023"));
    assert!(ris.contains("DO  - 10.1234/test"));
    assert!(ris.ends_with("ER  -\n"));
}

#[test]
fn test_ris_includes_tags_as_bango_keywords() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    // Original keywords are exported without prefix
    assert!(ris.contains("KW  - ml"));
    // Tags are exported with Bango: prefix
    assert!(ris.contains("KW  - Bango:machine-learning"));
    // Labels are also exported with Bango: prefix
    assert!(ris.contains("KW  - Bango:priority-read"));
    assert!(ris.contains("KW  - Bango:strong-methodology"));
}

#[test]
fn test_ris_includes_ai_reasoning_as_notes() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.contains("N1  - Article meets criteria."));
}

#[test]
fn test_ris_includes_matched_criteria_as_c1() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.contains("C1  -"));
    // C1 should contain resolved criterion text, not label names
    assert!(ris.contains("uses RCT methodology"));
    assert!(ris.contains("non-English language"));
}

#[test]
fn test_ris_skips_none_fields() {
    let mut article = make_export_article();
    article.doi = None;
    let ris = article_to_ris(&article);
    assert!(!ris.contains("DO  -"));
}

#[test]
fn test_multiple_articles_to_ris() {
    let article = make_export_article();
    let ris = article_to_ris(&article) + &article_to_ris(&article);
    assert_eq!(ris.matches("ER  -").count(), 2);
}

#[test]
fn test_ris_roundtrip_with_real_data() {
    let content = fs::read_to_string(asset_path("10-valid-Sugar.ris")).expect("fixture not found");
    let parsed = parse_ris(&content).expect("Parse failed");
    let record = &parsed.records[1]; // Using second record which has a DOI

    // Verify key fields exist before roundtrip
    assert!(record.title.is_some());
    assert!(record.doi.is_some());
    assert!(record.abstract_text.is_some());
    assert!(!record.authors.is_empty());

    // Convert to export article and back to RIS
    let export_article = RisExportArticle {
        reference_type: record.reference_type.clone(),
        title: record.title.clone().unwrap_or_default(),
        abstract_text: record.abstract_text.clone().unwrap_or_default(),
        authors: record.authors.clone(),
        publication_year: record.publication_year,
        doi: record.doi.clone(),
        journal: record.journal.clone(),
        volume: record.volume.clone(),
        issue: record.issue.clone(),
        start_page: record.start_page.clone(),
        end_page: record.end_page.clone(),
        keywords: record.keywords.clone(),
        tags: vec![],
        url: record.url.clone(),
        language: record.language.clone(),
        publisher: record.publisher.clone(),
        issn: record.issn.clone(),
        ai_reasoning: None,
        user_notes: None,
        ai_decision: None,
        labels: vec![],
        matched_inclusion_criteria: vec![],
        matched_exclusion_criteria: vec![],
    };

    let exported = article_to_ris(&export_article);

    // Re-parse the exported RIS
    let reparsed = parse_ris(&exported).expect("Re-parse failed");
    assert_eq!(reparsed.records.len(), 1);

    let rerecord = &reparsed.records[0];
    assert_eq!(rerecord.title, record.title);
    assert_eq!(rerecord.doi, record.doi);
    assert_eq!(rerecord.authors, record.authors);
    assert_eq!(rerecord.publication_year, record.publication_year);
}

#[test]
fn test_articles_to_ris_multiple() {
    let articles = vec![make_export_article(), make_export_article()];
    let ris = articles_to_ris(&articles);
    assert_eq!(ris.matches("TY  -").count(), 2);
    assert_eq!(ris.matches("ER  -").count(), 2);
}
