//! Tests for the OpenAlex Smart Search LLM prompt builder + parser.

use bango_lib::models::criterion::{Criterion, CriterionType, Priority, ResearchAim};
use bango_lib::openalex::smart_search;

fn make_aim(text: &str) -> ResearchAim {
    ResearchAim {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.to_string(),
        created_at: String::new(),
    }
}

fn make_criterion(text: &str, ctype: CriterionType, priority: Priority) -> Criterion {
    Criterion {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.to_string(),
        criterion_type: ctype,
        priority,
        created_at: String::new(),
    }
}

#[test]
fn build_smart_search_prompt_includes_aims() {
    let aims = vec![
        make_aim("Assess the impact of sugar taxes on obesity rates"),
        make_aim("Evaluate policy effectiveness in public health"),
    ];
    let inclusion = vec![];
    let exclusion = vec![];

    let (system, user) = smart_search::build_smart_search_prompt(&aims, &inclusion, &exclusion);

    // The user prompt should embed both research aims
    assert!(user.contains("Assess the impact of sugar taxes on obesity rates"));
    assert!(user.contains("Evaluate policy effectiveness in public health"));
    // The system prompt should mention OpenAlex
    assert!(system.contains("OpenAlex"));
}

#[test]
fn build_smart_search_prompt_includes_criteria() {
    let aims = vec![make_aim("Study sugar tax effects")];
    let inclusion = vec![make_criterion(
        "Studies published in English",
        CriterionType::Inclusion,
        Priority::High,
    )];
    let exclusion =
        vec![make_criterion("Animal studies", CriterionType::Exclusion, Priority::Critical)];

    let (_system, user) = smart_search::build_smart_search_prompt(&aims, &inclusion, &exclusion);

    // The user prompt should embed both inclusion and exclusion criteria
    assert!(user.contains("Studies published in English"));
    assert!(user.contains("Animal studies"));
}

#[test]
fn parse_smart_search_response_valid_json() {
    let raw = r#"{"searchQuery":"(\"sugar tax\" OR \"soda tax\") AND obesity","suggestedFilters":{"publicationYear":"2010-2025","type":["article","review"]}}"#;

    let result = smart_search::parse_smart_search_response(raw);

    assert!(result.is_ok());
    let query = result.unwrap();
    assert!(query.search_query.contains("sugar tax"));
    assert!(query.search_query.contains("obesity"));
    assert_eq!(query.suggested_filters.publication_year, Some("2010-2025".to_string()));
    assert_eq!(query.suggested_filters.r#type, vec!["article", "review"]);
}

#[test]
fn parse_smart_search_response_malformed_json() {
    let raw = "this is not valid JSON at all";

    let result = smart_search::parse_smart_search_response(raw);

    assert!(result.is_err());
    let err = result.unwrap_err();
    // Should be an AppError, not a panic
    assert!(err.to_string().contains("Failed to parse"));
}

#[test]
fn parse_smart_search_response_with_code_fences() {
    let raw = r#"```json
{"searchQuery":"test query","suggestedFilters":{"publicationYear":null,"type":[]}}
```"#;

    let result = smart_search::parse_smart_search_response(raw);

    assert!(result.is_ok());
    let query = result.unwrap();
    assert_eq!(query.search_query, "test query");
}
