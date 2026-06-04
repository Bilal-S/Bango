use bango_lib::summary::prompt::{
    build_summary_prompt, format_screening_summary, ArticleSummary, ScreeningData, SummaryPromptInput,
};

fn sample_screening_data() -> ScreeningData {
    ScreeningData {
        records_identified: 150,
        duplicates_removed: 30,
        records_screened: 120,
        records_excluded: 80,
        records_excluded_with_reasons: 60,
        records_assessed: 40,
        records_in_progress: 5,
        studies_included: 35,
        ai_screened: 100,
        manual_reviewed: 20,
        exclusion_reasons: vec![
            ("Wrong population".to_string(), 25),
            ("Not in English".to_string(), 15),
        ],
    }
}

fn sample_articles() -> Vec<ArticleSummary> {
    vec![
        ArticleSummary {
            title: "AI in Medicine".to_string(),
            authors: vec!["Smith J".to_string(), "Doe A".to_string()],
            year: Some(2024),
            abstract_text: "A survey of AI applications.".to_string(),
            keywords: vec!["ai".to_string(), "medicine".to_string()],
        },
        ArticleSummary {
            title: "Deep Learning for Diagnostics".to_string(),
            authors: vec!["Lee K".to_string()],
            year: Some(2023),
            abstract_text: "Novel DL approaches for medical imaging.".to_string(),
            keywords: vec![],
        },
    ]
}

// ── format_screening_summary tests ──

#[test]
fn test_format_basic_statistics() {
    let data = sample_screening_data();
    let summary = format_screening_summary(&data);
    assert!(summary.contains("Total records identified: 150"));
    assert!(summary.contains("Duplicates removed: 30"));
    assert!(summary.contains("Records screened: 120"));
    assert!(summary.contains("Records excluded: 80"));
    assert!(summary.contains("Records assessed for eligibility: 40"));
    assert!(summary.contains("Studies included in final review: 35"));
}

#[test]
fn test_format_screening_method() {
    let data = sample_screening_data();
    let summary = format_screening_summary(&data);
    assert!(summary.contains("100 articles were screened using AI-assisted review"));
    assert!(summary.contains("20 underwent manual review"));
}

#[test]
fn test_format_exclusion_reasons() {
    let data = sample_screening_data();
    let summary = format_screening_summary(&data);
    assert!(summary.contains("Top exclusion reasons:"));
    assert!(summary.contains("Wrong population (25 articles)"));
    assert!(summary.contains("Not in English (15 articles)"));
}

#[test]
fn test_format_excluded_with_specific_criteria() {
    let data = sample_screening_data();
    let summary = format_screening_summary(&data);
    assert!(summary.contains("Excluded with specific criteria: 60"));
}

#[test]
fn test_format_in_progress() {
    let data = sample_screening_data();
    let summary = format_screening_summary(&data);
    assert!(summary.contains("Records still in progress: 5"));
}

#[test]
fn test_format_minimal_data() {
    let data = ScreeningData {
        records_identified: 10,
        duplicates_removed: 0,
        records_screened: 10,
        records_excluded: 0,
        records_excluded_with_reasons: 0,
        records_assessed: 10,
        records_in_progress: 0,
        studies_included: 10,
        ai_screened: 0,
        manual_reviewed: 0,
        exclusion_reasons: vec![],
    };
    let summary = format_screening_summary(&data);
    assert!(summary.contains("Total records identified: 10"));
    assert!(summary.contains("Records screened: 10"));
    assert!(summary.contains("Studies included in final review: 10"));
    // Should NOT have optional sections
    assert!(!summary.contains("Duplicates removed"));
    assert!(!summary.contains("Screening method"));
    assert!(!summary.contains("Records excluded"));
    assert!(!summary.contains("Records still in progress"));
    assert!(!summary.contains("Top exclusion reasons"));
}

#[test]
fn test_format_no_duplicates_no_method() {
    let data = ScreeningData {
        records_identified: 50,
        duplicates_removed: 0,
        records_screened: 50,
        records_excluded: 10,
        records_excluded_with_reasons: 5,
        records_assessed: 40,
        records_in_progress: 0,
        studies_included: 40,
        ai_screened: 0,
        manual_reviewed: 0,
        exclusion_reasons: vec![],
    };
    let summary = format_screening_summary(&data);
    assert!(!summary.contains("Duplicates removed"));
    assert!(!summary.contains("Screening method"));
}

// ── build_summary_prompt tests ──

#[test]
fn test_prompt_contains_research_aims() {
    let input = SummaryPromptInput {
        aims: vec!["Understand AI in healthcare".to_string(), "Evaluate outcomes".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: sample_articles(),
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("1. Understand AI in healthcare"));
    assert!(prompt.contains("2. Evaluate outcomes"));
}

#[test]
fn test_prompt_with_empty_aims() {
    let input = SummaryPromptInput {
        aims: vec![],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: sample_articles(),
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("None defined."));
}

#[test]
fn test_prompt_contains_screening_summary() {
    let input = SummaryPromptInput {
        aims: vec!["Test aim".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: sample_articles(),
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("Total records identified: 150"));
}

#[test]
fn test_prompt_contains_citation_style() {
    let input = SummaryPromptInput {
        aims: vec!["Test aim".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "Vancouver".to_string(),
        articles: sample_articles(),
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("**Vancouver**"));
}

#[test]
fn test_prompt_contains_article_details() {
    let input = SummaryPromptInput {
        aims: vec!["Test aim".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: sample_articles(),
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("AI in Medicine"));
    assert!(prompt.contains("Smith J; Doe A"));
    assert!(prompt.contains("2024"));
    assert!(prompt.contains("A survey of AI applications."));
    assert!(prompt.contains("ai, medicine"));
    assert!(prompt.contains("Deep Learning for Diagnostics"));
    assert!(prompt.contains("Lee K"));
    assert!(prompt.contains("2023"));
}

#[test]
fn test_prompt_article_unknown_year() {
    let input = SummaryPromptInput {
        aims: vec!["Test aim".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: vec![ArticleSummary {
            title: "No Year Article".to_string(),
            authors: vec!["Author".to_string()],
            year: None,
            abstract_text: "Abstract text.".to_string(),
            keywords: vec![],
        }],
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("Year: Unknown"));
}

#[test]
fn test_prompt_article_no_keywords() {
    let input = SummaryPromptInput {
        aims: vec!["Test aim".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: vec![ArticleSummary {
            title: "No Keywords".to_string(),
            authors: vec!["Author".to_string()],
            year: Some(2024),
            abstract_text: "Abstract.".to_string(),
            keywords: vec![],
        }],
    };
    let prompt = build_summary_prompt(&input);
    assert!(!prompt.contains("Keywords:"));
}

#[test]
fn test_prompt_contains_section_instructions() {
    let input = SummaryPromptInput {
        aims: vec!["Test".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: vec![],
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("## Introduction"));
    assert!(prompt.contains("## Methodology"));
    assert!(prompt.contains("## Results"));
    assert!(prompt.contains("## Discussion"));
    assert!(prompt.contains("## Conclusion"));
    assert!(prompt.contains("## References"));
}

#[test]
fn test_prompt_no_em_dashes_rule() {
    let input = SummaryPromptInput {
        aims: vec!["Test".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: vec![],
    };
    let prompt = build_summary_prompt(&input);
    assert!(prompt.contains("em dash"));
}

#[test]
fn test_prompt_with_empty_articles() {
    let input = SummaryPromptInput {
        aims: vec!["Test".to_string()],
        screening_data: sample_screening_data(),
        citation_style: "APA".to_string(),
        articles: vec![],
    };
    let prompt = build_summary_prompt(&input);
    // Should still produce a valid prompt
    assert!(prompt.contains("## Task"));
    assert!(prompt.contains("## Research Aims"));
}