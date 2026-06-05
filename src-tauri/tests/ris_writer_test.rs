use bango_lib::export::ris_writer::{article_to_ris, articles_to_ris, RisExportArticle};

fn sample_article() -> RisExportArticle {
    RisExportArticle {
        reference_type: Some("JOUR".to_string()),
        title: "Machine Learning in Healthcare".to_string(),
        abstract_text: "A survey of ML applications.".to_string(),
        authors: vec!["Smith J".to_string(), "Doe A".to_string()],
        publication_year: Some(2024),
        doi: Some("10.1234/test".to_string()),
        journal: Some("Nature ML".to_string()),
        volume: Some("5".to_string()),
        issue: Some("2".to_string()),
        start_page: Some("100".to_string()),
        end_page: Some("120".to_string()),
        keywords: vec!["machine-learning".to_string()],
        tags: vec!["clinical-trial".to_string()],
        url: Some("https://example.com".to_string()),
        language: Some("English".to_string()),
        publisher: Some("Springer".to_string()),
        issn: Some("1234-5678".to_string()),
        notes: Some("Original imported note from database".to_string()),
        ai_reasoning: Some("Relevant to inclusion criteria".to_string()),
        user_notes: Some("Important article".to_string()),
        ai_decision: Some("include".to_string()),
        labels: vec!["priority-read".to_string()],
        matched_inclusion_criteria: vec!["criterion-1".to_string()],
        matched_exclusion_criteria: vec![],
    }
}

fn minimal_article() -> RisExportArticle {
    RisExportArticle {
        reference_type: None,
        title: "Minimal Article".to_string(),
        abstract_text: "Abstract.".to_string(),
        authors: vec![],
        publication_year: None,
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec![],
        tags: vec![],
        url: None,
        language: None,
        publisher: None,
        issn: None,
        notes: None,
        ai_reasoning: None,
        user_notes: None,
        ai_decision: None,
        labels: vec![],
        matched_inclusion_criteria: vec![],
        matched_exclusion_criteria: vec![],
    }
}

// ── article_to_ris tests ──

#[test]
fn test_ris_output_starts_with_type() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.starts_with("TY  - JOUR\n"));
}

#[test]
fn test_ris_default_type_is_jour() {
    let ris = article_to_ris(&minimal_article());
    assert!(ris.starts_with("TY  - JOUR\n"), "Default reference type should be JOUR");
}

#[test]
fn test_ris_includes_title() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("TI  - Machine Learning in Healthcare\n"));
}

#[test]
fn test_ris_includes_abstract() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("AB  - A survey of ML applications.\n"));
}

#[test]
fn test_ris_includes_all_authors() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("AU  - Smith J\n"));
    assert!(ris.contains("AU  - Doe A\n"));
}

#[test]
fn test_ris_includes_publication_year() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("PY  - 2024\n"));
}

#[test]
fn test_ris_omits_year_when_none() {
    let ris = article_to_ris(&minimal_article());
    assert!(!ris.contains("PY  -"));
}

#[test]
fn test_ris_includes_doi() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("DO  - 10.1234/test\n"));
}

#[test]
fn test_ris_includes_journal() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("T2  - Nature ML\n"));
}

#[test]
fn test_ris_includes_volume_and_issue() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("VL  - 5\n"));
    assert!(ris.contains("IS  - 2\n"));
}

#[test]
fn test_ris_includes_page_range() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("SP  - 100\n"));
    assert!(ris.contains("EP  - 120\n"));
}

#[test]
fn test_ris_includes_keywords() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("KW  - machine-learning\n"));
}

#[test]
fn test_ris_tags_prefixed_with_bango() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("KW  - Bango:clinical-trial\n"));
}

#[test]
fn test_ris_labels_prefixed_with_bango() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("KW  - Bango:priority-read\n"));
}

#[test]
fn test_ris_includes_url() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("UR  - https://example.com\n"));
}

#[test]
fn test_ris_includes_language() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("LA  - English\n"));
}

#[test]
fn test_ris_includes_publisher() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("PB  - Springer\n"));
}

#[test]
fn test_ris_includes_issn() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("SN  - 1234-5678\n"));
}

#[test]
fn test_ris_includes_imported_notes_in_n1() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("N1  - Original imported note from database\n"));
}

#[test]
fn test_ris_imported_notes_appears_before_ai_reasoning() {
    let ris = article_to_ris(&sample_article());
    let notes_pos = ris
        .find("N1  - Original imported note from database")
        .expect("imported notes N1 not found");
    let reasoning_pos =
        ris.find("N1  - Relevant to inclusion criteria").expect("AI reasoning N1 not found");
    assert!(notes_pos < reasoning_pos, "Imported notes N1 should appear before AI reasoning N1");
}

#[test]
fn test_ris_both_notes_and_reasoning_produce_two_n1_lines() {
    let ris = article_to_ris(&sample_article());
    assert_eq!(
        ris.matches("N1  -").count(),
        2,
        "Should have exactly 2 N1 lines (imported notes + AI reasoning)"
    );
}

#[test]
fn test_ris_no_n1_when_neither_notes_nor_reasoning() {
    let ris = article_to_ris(&minimal_article());
    assert!(
        !ris.contains("N1  -"),
        "Should have no N1 lines when notes and ai_reasoning are both None"
    );
}

#[test]
fn test_ris_includes_ai_reasoning_in_n1() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("N1  - Relevant to inclusion criteria\n"));
}

#[test]
fn test_ris_includes_user_notes_in_no() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("NO  - Important article\n"));
}

#[test]
fn test_ris_includes_matched_criteria_in_c1() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.contains("C1  - "));
    assert!(ris.contains(r#""inc":["#));
    assert!(ris.contains("criterion-1"));
}

#[test]
fn test_ris_omits_c1_when_no_criteria_matched() {
    let ris = article_to_ris(&minimal_article());
    assert!(!ris.contains("C1  -"));
}

#[test]
fn test_ris_ends_with_er() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.trim_end().ends_with("ER  -"));
}

#[test]
fn test_ris_ends_with_newline() {
    let ris = article_to_ris(&sample_article());
    assert!(ris.ends_with('\n'));
}

#[test]
fn test_minimal_article_only_required_fields() {
    let ris = article_to_ris(&minimal_article());
    // Should have TY, TI, AB, ER and nothing else optional
    assert!(ris.contains("TY  - JOUR"));
    assert!(ris.contains("TI  - Minimal Article"));
    assert!(ris.contains("AB  - Abstract."));
    assert!(ris.contains("ER  -"));
    // Should NOT have any optional fields
    assert!(!ris.contains("AU  -"));
    assert!(!ris.contains("PY  -"));
    assert!(!ris.contains("DO  -"));
    assert!(!ris.contains("T2  -"));
    assert!(!ris.contains("KW  -"));
}

// ── articles_to_ris tests ──

#[test]
fn test_articles_to_ris_concatenates() {
    let articles = vec![sample_article(), minimal_article()];
    let ris = articles_to_ris(&articles);
    assert!(ris.contains("Machine Learning in Healthcare"));
    assert!(ris.contains("Minimal Article"));
    // Should have two ER markers
    assert_eq!(ris.matches("ER  -").count(), 2);
}

#[test]
fn test_articles_to_ris_empty() {
    let ris = articles_to_ris(&[]);
    assert!(ris.is_empty());
}

#[test]
fn test_articles_to_ris_single() {
    let ris = articles_to_ris(&[sample_article()]);
    assert_eq!(ris.matches("ER  -").count(), 1);
}
