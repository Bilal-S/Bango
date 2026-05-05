use bango_lib::models::criterion::{CriterionType, Priority};
use bango_lib::screening::prompt::{
    build_screening_prompt, AimEntry, CriterionEntry, ScreeningPromptInput,
};
use bango_lib::screening::resolution::{resolve_decision, CriterionMatch, ScreeningInput};
use bango_lib::screening::token_estimation::estimate_tokens;

fn make_match(id: &str, ctype: CriterionType, priority: Priority) -> CriterionMatch {
    CriterionMatch { id: id.to_string(), criterion_type: ctype, priority }
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
    let input = ScreeningInput { inclusion_matches: vec![], exclusion_matches: vec![] };
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
        aims: vec![AimEntry { text: "Study ML in healthcare".to_string() }],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test".to_string(),
        article_authors: "Smith, John".to_string(),
        article_year: Some(2023),
        article_abstract: "Abstract text".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Study ML in healthcare"));
}

#[test]
fn test_build_prompt_contains_criteria_with_priority() {
    let input = ScreeningPromptInput {
        aims: vec![],
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
        article_title: "Test".to_string(),
        article_authors: "Author".to_string(),
        article_year: None,
        article_abstract: "Abstract".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("c1"));
    assert!(prompt.contains("Must be about ML"));
    assert!(prompt.contains("critical"));
    assert!(prompt.contains("c2"));
    assert!(prompt.contains("Not a review"));
    assert!(prompt.contains("high"));
}

#[test]
fn test_build_prompt_contains_article_fields() {
    let input = ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Deep Learning for Medical Imaging".to_string(),
        article_authors: "Doe, Jane; Smith, John".to_string(),
        article_year: Some(2024),
        article_abstract: "This paper reviews deep learning methods.".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Deep Learning for Medical Imaging"));
    assert!(prompt.contains("Doe, Jane; Smith, John"));
    assert!(prompt.contains("2024"));
    assert!(prompt.contains("This paper reviews deep learning methods."));
}

#[test]
fn test_build_prompt_response_format() {
    let input = ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test".to_string(),
        article_authors: "Author".to_string(),
        article_year: None,
        article_abstract: "Abstract".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("\"decision\""));
    assert!(prompt.contains("\"reasoning\""));
    assert!(prompt.contains("\"matched_inclusion_criteria\""));
    assert!(prompt.contains("\"matched_exclusion_criteria\""));
    assert!(prompt.contains("\"suggested_tags\""));
    assert!(prompt.contains("\"confidence\""));
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
        aims: vec![AimEntry { text: "Study AI".to_string() }],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test Article Title".to_string(),
        article_authors: "Author".to_string(),
        article_year: Some(2023),
        article_abstract: "a".repeat(200),
    };
    let prompt = build_screening_prompt(&input);
    let tokens = estimate_tokens(&prompt);
    assert!(tokens > 0, "Should estimate some tokens");
    assert!(tokens < 500, "Should be reasonable: got {tokens}");
}
