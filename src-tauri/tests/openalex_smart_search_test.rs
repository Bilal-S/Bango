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

// ── Prompt: 1500-char budget + OpenAlex best practices ───────────────────

#[test]
fn build_smart_search_prompt_states_char_limit() {
    let aims = vec![make_aim("Study sugar tax effects")];
    let (system, user) = smart_search::build_smart_search_prompt(&aims, &[], &[]);

    assert!(system.contains("1500"), "system prompt must state the 1500-char limit");
    assert!(user.contains("1500"), "user prompt must state the 1500-char budget");
}

#[test]
fn build_smart_search_prompt_leverages_stemming() {
    let aims = vec![make_aim("Study sugar tax effects")];
    let (system, user) = smart_search::build_smart_search_prompt(&aims, &[], &[]);

    // The prompt must tell the LLM NOT to enumerate redundant synonyms/stems.
    assert!(system.to_lowercase().contains("stems"), "system prompt must reference stemming");
    assert!(
        user.to_lowercase().contains("redundant"),
        "user prompt must warn against redundant synonyms"
    );
}

#[test]
fn build_smart_search_prompt_wildcard_discipline() {
    let aims = vec![make_aim("Study sugar tax effects")];
    let (system, _user) = smart_search::build_smart_search_prompt(&aims, &[], &[]);

    // The prompt must restrict wildcard usage to quoted multi-word phrases.
    assert!(
        system.to_lowercase().contains("wildcard"),
        "system prompt must give wildcard guidance"
    );
}

// ── truncate_search_query: pure helper ───────────────────────────────────

#[test]
fn truncate_search_query_under_limit_unchanged() {
    let q = "(sugar OR levy) AND tax";
    let out = smart_search::truncate_search_query(q, 1500);
    assert_eq!(out, q);
}

#[test]
fn truncate_search_query_truncates_at_top_level_operator() {
    // Build: "(a OR b) AND (c OR d) AND " + padding so the third group is the
    // latest top-level close that still fits under the small budget.
    let q = "(alpha OR beta) AND (gamma OR delta) AND (epsilon OR zeta)";
    // Budget chosen so "(alpha OR beta) AND (gamma OR delta)" fits but the
    // trailing " AND (epsilon OR zeta)" does not.
    let out = smart_search::truncate_search_query(q, 44);
    assert!(out.len() <= 44);
    assert!(out.contains("(alpha OR beta)"));
    assert!(out.contains("(gamma OR delta)"));
    assert!(!out.contains("epsilon"), "must drop the third group");
}

#[test]
fn truncate_search_query_keeps_parens_balanced() {
    // Many nested groups; truncate to a small budget and assert paren balance.
    let q = "(a OR (b AND c)) AND (d OR e) AND (f OR g) AND (h OR i)";
    let out = smart_search::truncate_search_query(q, 30);
    let open = out.matches('(').count();
    let close = out.matches(')').count();
    assert_eq!(open, close, "parentheses must stay balanced: {out}");
}

#[test]
fn truncate_search_query_does_not_split_inside_phrase() {
    // Single over-long group with a quoted phrase that straddles the budget.
    let q = "(\"long exact phrase here that should not be split mid quote\" OR x) AND (y OR z)";
    let out = smart_search::truncate_search_query(q, 40);
    // The cut must not land inside the quoted phrase: count double-quotes in
    // the result must be even (balanced), never odd.
    assert_eq!(
        out.matches('"').count() % 2,
        0,
        "quotes must be balanced (never split inside a phrase): {out}"
    );
}

#[test]
fn truncate_search_query_falls_back_to_whitespace() {
    // No parentheses at all: cut at the last whitespace boundary so the result
    // ends on a complete word, never mid-word.
    let q = "alpha beta gamma delta epsilon";
    let out = smart_search::truncate_search_query(q, 16);
    assert!(out.len() <= 16, "must respect budget: {out}");
    assert_eq!(out, "alpha beta gamma", "must cut at the last word boundary <= 16: {out}");
}

#[test]
fn truncate_search_query_zero_max_returns_empty() {
    let out = smart_search::truncate_search_query("(a OR b) AND c", 0);
    assert_eq!(out, "");
}

// ── parse_smart_search_response: over-long query is capped ───────────────

#[test]
fn parse_smart_search_response_truncates_overlong_query() {
    // 2000+ char query (well over the 1500 limit). serde needs a valid JSON
    // string, so build it programmatically.
    let long_term = "obesity".repeat(300); // ~2100 chars, no quotes/parens
    let raw = format!(
        r#"{{"searchQuery":"{long_term}","suggestedFilters":{{"publicationYear":null,"type":[]}}}}"#
    );
    assert!(long_term.len() > 1500);

    let result = smart_search::parse_smart_search_response(&raw);
    assert!(result.is_ok());
    let query = result.unwrap();
    assert!(
        query.search_query.len() <= smart_search::MAX_SEARCH_QUERY_LEN,
        "parsed query must be capped to MAX_SEARCH_QUERY_LEN"
    );
}
