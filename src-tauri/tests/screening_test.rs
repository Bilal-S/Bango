use bango_lib::models::criterion::{CriterionType, Priority};
use bango_lib::screening::prompt::{
    build_screening_prompt, AimEntry, ArticleEntry, CriterionEntry, ScreeningPromptInput,
    SYSTEM_PROMPT,
};
use bango_lib::screening::resolution::{resolve_decision, CriterionMatch, ScreeningInput};
use bango_lib::screening::token_estimation::estimate_tokens;

fn make_match(id: &str, ctype: CriterionType, priority: Priority) -> CriterionMatch {
    CriterionMatch {
        id: id.to_string(),
        criterion_type: ctype,
        priority,
    }
}

fn make_single_article_input() -> ScreeningPromptInput {
    ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        articles: vec![ArticleEntry {
            title: "Test".to_string(),
            authors: "Author".to_string(),
            year: None,
            abstract_text: "Abstract".to_string(),
        }],
    }
}

// --- Resolution tests ---

#[test]
fn test_inclusion_wins_higher_priority() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Critical)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_exclusion_wins_higher_priority() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::Critical)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_tied_priority_favors_inclusion() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::High)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_no_criteria_matches_exclude() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_only_inclusion_matches() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_only_exclusion_matches() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![make_match("1", CriterionType::Exclusion, Priority::Standard)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_multiple_inclusion_picks_highest() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("1", CriterionType::Inclusion, Priority::Low),
            make_match("2", CriterionType::Inclusion, Priority::Critical),
            make_match("3", CriterionType::Inclusion, Priority::Standard),
        ],
        exclusion_matches: vec![make_match("4", CriterionType::Exclusion, Priority::High)],
    };
    // Critical inclusion > High exclusion -> include
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_multiple_exclusion_picks_highest() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![
            make_match("2", CriterionType::Exclusion, Priority::Low),
            make_match("3", CriterionType::Exclusion, Priority::Critical),
        ],
    };
    // Critical exclusion > Standard inclusion -> exclude
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_realistic_screening_scenario() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("inc-1", CriterionType::Inclusion, Priority::Standard),
            make_match("inc-2", CriterionType::Inclusion, Priority::High),
        ],
        exclusion_matches: vec![make_match("exc-1", CriterionType::Exclusion, Priority::Standard)],
    };
    // High inclusion > Standard exclusion -> include
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_critical_exclusion_overrides_all() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("inc-1", CriterionType::Inclusion, Priority::High),
            make_match("inc-2", CriterionType::Inclusion, Priority::Standard),
        ],
        exclusion_matches: vec![make_match("exc-1", CriterionType::Exclusion, Priority::Critical)],
    };
    // Critical exclusion > everything -> exclude
    assert_eq!(resolve_decision(&input), "exclude");
}

// --- Prompt tests ---

#[test]
fn test_build_prompt_contains_research_aims() {
    let input = ScreeningPromptInput {
        aims: vec![AimEntry {
            text: "Study ML in healthcare".to_string(),
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Study ML in healthcare"));
}

#[test]
fn test_build_prompt_contains_criteria() {
    let input = ScreeningPromptInput {
        inclusion_criteria: vec![CriterionEntry {
            id: "c1".to_string(),
            text: "Must be about ML".to_string(),
            priority: Priority::Critical,
        }],
        exclusion_criteria: vec![CriterionEntry {
            id: "c2".to_string(),
            text: "Not a review".to_string(),
            priority: Priority::High,
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("[c1]"));
    assert!(prompt.contains("Must be about ML"));
    assert!(prompt.contains("[c2]"));
    assert!(prompt.contains("Not a review"));
}

#[test]
fn test_build_prompt_contains_article_fields() {
    let input = ScreeningPromptInput {
        articles: vec![ArticleEntry {
            title: "Deep Learning for Medical Imaging".to_string(),
            authors: "Doe, Jane; Smith, John".to_string(),
            year: Some(2024),
            abstract_text: "This paper reviews deep learning methods.".to_string(),
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Deep Learning for Medical Imaging"));
    assert!(prompt.contains("Doe, Jane; Smith, John"));
    assert!(prompt.contains("2024"));
    assert!(prompt.contains("This paper reviews deep learning methods."));
}

#[test]
fn test_system_prompt_contains_response_format() {
    assert!(SYSTEM_PROMPT.contains("\"decision\""));
    assert!(SYSTEM_PROMPT.contains("\"reasoning\""));
    assert!(SYSTEM_PROMPT.contains("\"matched_inclusion_criteria\""));
    assert!(SYSTEM_PROMPT.contains("\"matched_exclusion_criteria\""));
    assert!(SYSTEM_PROMPT.contains("\"suggested_tags\""));
    assert!(SYSTEM_PROMPT.contains("\"confidence\""));
    assert!(SYSTEM_PROMPT.contains("\"error\""));
}

#[test]
fn test_build_prompt_no_response_format_in_user_prompt() {
    let input = make_single_article_input();
    let prompt = build_screening_prompt(&input);
    // User prompt should NOT contain response format schema
    assert!(!prompt.contains("## Response Format"));
    assert!(!prompt.contains("Return JSON exactly matching this schema"));
}

#[test]
fn test_build_prompt_simplified_priority_when_all_same() {
    let input = ScreeningPromptInput {
        inclusion_criteria: vec![
            CriterionEntry {
                id: "c1".to_string(),
                text: "Inc 1".to_string(),
                priority: Priority::Standard,
            },
            CriterionEntry {
                id: "c2".to_string(),
                text: "Inc 2".to_string(),
                priority: Priority::Standard,
            },
        ],
        exclusion_criteria: vec![CriterionEntry {
            id: "c3".to_string(),
            text: "Exc 1".to_string(),
            priority: Priority::Standard,
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    // Should use simplified priority rule
    assert!(prompt.contains("If conflict between criteria favor inclusion rules."));
    assert!(!prompt.contains("in order of priority"));
    // Headers should NOT mention priority
    assert!(prompt.contains("## Inclusion Criteria\n"));
    assert!(prompt.contains("## Exclusion Criteria\n"));
}

#[test]
fn test_build_prompt_priority_ordering_when_mixed() {
    let input = ScreeningPromptInput {
        inclusion_criteria: vec![
            CriterionEntry {
                id: "c1".to_string(),
                text: "Low inc".to_string(),
                priority: Priority::Low,
            },
            CriterionEntry {
                id: "c2".to_string(),
                text: "Critical inc".to_string(),
                priority: Priority::Critical,
            },
        ],
        exclusion_criteria: vec![CriterionEntry {
            id: "c3".to_string(),
            text: "High exc".to_string(),
            priority: Priority::High,
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    // Should use detailed priority rules
    assert!(prompt.contains("Higher priority rules always outweigh lower priority rules."));
    // Headers should mention priority
    assert!(prompt.contains("## Inclusion Criteria (in order of priority)"));
    assert!(prompt.contains("## Exclusion Criteria (in order of priority)"));
    // Critical inc should come before Low inc (sorted by priority descending)
    let critical_pos = prompt.find("Critical inc").expect("should contain critical");
    let low_pos = prompt.find("Low inc").expect("should contain low");
    assert!(
        critical_pos < low_pos,
        "Critical should appear before Low in prompt"
    );
}

#[test]
fn test_build_prompt_multiple_articles() {
    let input = ScreeningPromptInput {
        articles: vec![
            ArticleEntry {
                title: "Article One".to_string(),
                authors: "Author A".to_string(),
                year: Some(2023),
                abstract_text: "Abstract one.".to_string(),
            },
            ArticleEntry {
                title: "Article Two".to_string(),
                authors: "Author B".to_string(),
                year: Some(2024),
                abstract_text: "Abstract two.".to_string(),
            },
        ],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Article One"));
    assert!(prompt.contains("Article Two"));
    assert!(prompt.contains("Abstract one"));
    assert!(prompt.contains("Abstract two"));
}

// --- Token estimation tests ---

#[test]
fn test_estimate_tokens_empty_string() {
    assert_eq!(estimate_tokens(""), 0);
}

#[test]
fn test_estimate_tokens_basic() {
    let text = "a".repeat(100);
    assert_eq!(estimate_tokens(&text), 25);
}

#[test]
fn test_estimate_tokens_unicode() {
    let text = "\u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30b9}\u{30c8}"; // 6 chars
    assert_eq!(estimate_tokens(&text), 1);
}

#[test]
fn test_prompt_token_estimation() {
    let input = ScreeningPromptInput {
        aims: vec![AimEntry {
            text: "Study AI".to_string(),
        }],
        articles: vec![ArticleEntry {
            title: "Test Article Title".to_string(),
            authors: "Author".to_string(),
            year: Some(2023),
            abstract_text: "a".repeat(200),
        }],
        ..make_single_article_input()
    };
    let prompt = build_screening_prompt(&input);
    let tokens = estimate_tokens(&prompt);
    assert!(tokens > 0, "Should estimate some tokens");
    assert!(tokens < 500, "Should be reasonable: got {tokens}");
}