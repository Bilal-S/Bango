use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::criterion::{Criterion, CriterionType, Priority};
use bango_lib::screening::engine::{
    augment_matched_from_reasoning, balance_braces, build_global_criterion_numbering,
    create_or_match_label, create_or_match_tag, extract_json, process_screening_responses,
    ScreeningEngine,
};
use rusqlite::Connection;

fn setup_test_db() -> Connection {
    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");
    conn
}

fn insert_test_article(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, 'Test Article', 'Author', 'Abstract text', 'working', 'test.ris')",
        rusqlite::params![id],
    )
    .expect("Insert article failed");
}

// ── extract_json tests ──

#[test]
fn test_extract_json_plain_array() {
    let input = r#"[{"decision":"include"}]"#;
    assert_eq!(extract_json(input), input.trim());
}

#[test]
fn test_extract_json_code_fence() {
    let inner = r#"[{"decision":"include"}]"#;
    let input = format!("```json\n{inner}\n```");
    assert_eq!(extract_json(&input), inner);
}

#[test]
fn test_extract_json_whitespace() {
    let inner = r#"[{"decision":"include"}]"#;
    let input = format!("  \n{inner}\n  ");
    assert_eq!(extract_json(&input), inner);
}

#[test]
fn test_extract_json_empty_string() {
    assert_eq!(extract_json(""), "");
}

// ── process_screening_responses tests ──

#[test]
fn test_parse_single_response() {
    let raw = r#"[{"decision":"include","reasoning":"ok","matchedInclusionCriteria":["c1"],"matchedExclusionCriteria":[],"suggestedTags":["ml"],"confidence":0.92}]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].decision, "include");
    assert_eq!(results[0].matched_inclusion_criteria, vec!["c1"]);
}

#[test]
fn test_parse_batch_of_three() {
    let raw = r#"[
        {"decision":"include","reasoning":"R1","matchedInclusionCriteria":["c1"],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
        {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":["c2"],"suggestedTags":[],"confidence":0.85},
        {"decision":"include","reasoning":"R3","matchedInclusionCriteria":["c1","c3"],"matchedExclusionCriteria":["c2"],"suggestedTags":["dl","medical"],"confidence":0.75}
    ]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].decision, "include");
    assert_eq!(results[1].decision, "exclude");
    assert_eq!(results[2].suggested_tags, vec!["dl", "medical"]);
}

#[test]
fn test_parse_error_decision() {
    let raw = r#"[{"decision":"error","reasoning":"Abstract too short","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.0}]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results[0].decision, "error");
}

#[test]
fn test_parse_snake_case_field_names() {
    let raw = r#"[{"decision":"include","reasoning":"ok","matched_inclusion_criteria":["c1"],"matched_exclusion_criteria":[],"suggested_tags":["ml"],"confidence":0.9}]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results[0].matched_inclusion_criteria, vec!["c1"]);
}

#[test]
fn test_parse_missing_optional_fields_default() {
    let raw = r#"[{"decision":"include","reasoning":"ok"}]"#;
    let results = process_screening_responses(raw).unwrap();
    assert!(results[0].matched_inclusion_criteria.is_empty());
    assert_eq!(results[0].confidence, 0.0);
}

#[test]
fn test_parse_empty_array() {
    let results = process_screening_responses("[]").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_parse_invalid_json_returns_error() {
    let result = process_screening_responses("this is not json");
    assert!(result.is_err());
}

#[test]
fn test_parse_json_object_instead_of_array_returns_error() {
    let raw = r#"{"decision":"include","reasoning":"ok"}"#;
    assert!(process_screening_responses(raw).is_err());
}

#[test]
fn test_parse_response_wrapped_in_code_fence() {
    let inner = r#"[{"decision":"include","reasoning":"ok","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}]"#;
    let raw = format!("```json\n{inner}\n```");
    let results = process_screening_responses(&raw).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_parse_unknown_decision_becomes_error() {
    let raw = r#"[{"decision":"maybe","reasoning":"unsure","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.5}]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results[0].decision, "error");
}

// ── ScreeningEngine construction ──

#[test]
fn test_with_batch_size_zero_does_not_panic() {
    let _engine = ScreeningEngine::with_batch_size(0);
}

#[test]
fn test_with_batch_size_five_does_not_panic() {
    let _engine = ScreeningEngine::with_batch_size(5);
}

#[test]
fn test_default_new_does_not_panic() {
    let _engine = ScreeningEngine::new();
}

// ── create_or_match_tag tests ──

#[test]
fn test_tag_matches_existing_case_insensitive() {
    let conn = setup_test_db();
    let article_id = "art-tag-match";
    insert_test_article(&conn, article_id);
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES ('t1', 'machine-learning', 'user_created')",
        [],
    )
    .unwrap();
    create_or_match_tag(&conn, "Machine-Learning", article_id).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_tag_creates_new_when_no_match() {
    let conn = setup_test_db();
    let article_id = "art-tag-new";
    insert_test_article(&conn, article_id);
    create_or_match_tag(&conn, "deep-learning", article_id).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "deep-learning");
}

#[test]
fn test_tag_trimmed_to_30_chars() {
    let conn = setup_test_db();
    let article_id = "art-tag-trim";
    insert_test_article(&conn, article_id);
    create_or_match_tag(
        &conn,
        "this-is-a-very-long-tag-name-that-exceeds-thirty-chars",
        article_id,
    )
    .unwrap();
    let name: String = conn
        .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name.len(), 30);
}

// ── create_or_match_label tests ──

#[test]
fn test_label_matches_existing_case_insensitive() {
    let conn = setup_test_db();
    let article_id = "art-label-match";
    insert_test_article(&conn, article_id);
    conn.execute(
        "INSERT INTO labels (id, name, source) VALUES ('l1', 'priority-read', 'user_created')",
        [],
    )
    .unwrap();
    create_or_match_label(&conn, "Priority-Read", article_id).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM labels", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_label_creates_new_when_no_match() {
    let conn = setup_test_db();
    let article_id = "art-label-new";
    insert_test_article(&conn, article_id);
    create_or_match_label(&conn, "strong-methodology", article_id).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM labels WHERE source = 'ai_generated'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "strong-methodology");
}

#[test]
fn test_label_trimmed_to_30_chars() {
    let conn = setup_test_db();
    let article_id = "art-label-trim";
    insert_test_article(&conn, article_id);
    let long_label = "Inclusion: this is a very long criterion text that exceeds limit";
    create_or_match_label(&conn, long_label, article_id).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM labels WHERE source = 'ai_generated'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name.len(), 30);
}

// ── balance_braces tests ──

#[test]
fn test_balance_braces_no_change_when_balanced() {
    let json = r#"{"key": "value"}"#;
    assert_eq!(balance_braces(json), json);
}

#[test]
fn test_balance_braces_prepends_missing_open() {
    let json = r#"  "key": "value"}"#;
    let result = balance_braces(json);
    assert_eq!(result, r#"{  "key": "value"}"#);
}

#[test]
fn test_balance_braces_appends_missing_close() {
    let json = r#"{"key": "value""#;
    let result = balance_braces(json);
    assert_eq!(result, r#"{"key": "value"}"#);
}

// ── extract_json: additional variants ──

#[test]
fn test_extract_json_plain_code_fence() {
    let inner = r#"[{"decision":"include"}]"#;
    let input = format!("```\n{inner}\n```");
    assert_eq!(extract_json(&input), inner);
}

#[test]
fn test_extract_json_missing_opening_brace_in_code_fence() {
    // Gemini sometimes omits the opening `{` after stripping markdown fences
    let inner = r#"  "field": "medicine",
  "subfield": "public_health_nutrition"
}"#;
    let input = format!("```json\n{inner}\n```");
    let result = extract_json(&input);
    assert!(
        result.starts_with('{'),
        "Should prepend missing opening brace, got: {}",
        &result[..result.len().min(80)]
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&result).is_ok(),
        "Result should be valid JSON: {result}"
    );
}

// ── process_screening_responses: additional coverage ──

#[test]
fn test_parse_batch_of_fifteen() {
    let items: Vec<String> = (0..15)
        .map(|i| {
            format!(
                r#"{{"decision":"{}","reasoning":"Article {}","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.{:02}}}"#,
                if i % 2 == 0 { "include" } else { "exclude" },
                i,
                50 + i
            )
        })
        .collect();
    let raw = format!("[{}]", items.join(","));
    let results = process_screening_responses(&raw).unwrap();
    assert_eq!(results.len(), 15);
    assert_eq!(results[0].decision, "include");
    assert_eq!(results[1].decision, "exclude");
    assert_eq!(results[14].decision, "include"); // 14 % 2 == 0 → include
}

#[test]
fn test_parse_response_with_all_fields_populated() {
    let raw = r#"[{
        "decision": "include",
        "reasoning": "Meets criteria c1 and c3.",
        "matchedInclusionCriteria": ["c1", "c3"],
        "matchedExclusionCriteria": ["c2"],
        "suggestedTags": ["machine-learning", "healthcare", "systematic-review"],
        "confidence": 0.95
    }]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results[0].matched_inclusion_criteria, vec!["c1", "c3"]);
    assert_eq!(results[0].matched_exclusion_criteria, vec!["c2"]);
    assert_eq!(
        results[0].suggested_tags,
        vec!["machine-learning", "healthcare", "systematic-review"]
    );
}

#[test]
fn test_parse_response_with_empty_arrays() {
    let raw = r#"[{
        "decision": "exclude",
        "reasoning": "No criteria matched",
        "matchedInclusionCriteria": [],
        "matchedExclusionCriteria": [],
        "suggestedTags": [],
        "confidence": 0.3
    }]"#;
    let results = process_screening_responses(raw).unwrap();
    assert!(results[0].matched_inclusion_criteria.is_empty());
    assert!(results[0].matched_exclusion_criteria.is_empty());
    assert!(results[0].suggested_tags.is_empty());
}

#[test]
fn test_parse_response_with_surrounding_text() {
    // LLM sometimes wraps JSON in explanatory text - extract_json handles this
    let raw = r#"Here are the screening results:
[
{"decision":"include","reasoning":"ok","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}
]
Hope this helps!"#;
    let result = process_screening_responses(raw);
    assert!(result.is_ok(), "Should extract JSON from surrounding text");
    let responses = result.unwrap();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].decision, "include");
}

#[test]
fn test_parse_missing_required_field_returns_error() {
    let raw = r#"[{"decision":"include"}]"#;
    let result = process_screening_responses(raw);
    assert!(result.is_err(), "Missing fields should fail deserialization");
}

#[test]
fn test_parse_extra_unknown_fields_ignored() {
    let raw = r#"[{
        "decision": "include",
        "reasoning": "ok",
        "matchedInclusionCriteria": [],
        "matchedExclusionCriteria": [],
        "suggestedTags": [],
        "confidence": 0.9,
        "extra_field": "should be ignored"
    }]"#;
    let results = process_screening_responses(raw).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Response count mismatch validation ──

#[test]
fn test_response_count_mismatch_detected() {
    // Simulate: 3 articles fetched, but LLM returns 2 results
    let raw = r#"[
        {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
        {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8}
    ]"#;
    let results = process_screening_responses(raw).unwrap();
    let batch_len = 3;
    assert_ne!(results.len(), batch_len, "Should detect mismatch: 2 results for 3 articles");
}

#[test]
fn test_response_count_matches_batch() {
    let raw = r#"[
        {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
        {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8},
        {"decision":"include","reasoning":"R3","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.7}
    ]"#;
    let results = process_screening_responses(raw).unwrap();
    let batch_len = 3;
    assert_eq!(results.len(), batch_len, "Count should match");
}

#[test]
fn test_response_more_results_than_articles() {
    // 1 article but 2 results → mismatch
    let raw = r#"[
        {"decision":"include","reasoning":"R1","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9},
        {"decision":"exclude","reasoning":"R2","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.8}
    ]"#;
    let results = process_screening_responses(raw).unwrap();
    let batch_len = 1;
    assert_ne!(results.len(), batch_len, "Should detect: more results than articles");
}

// ── create_or_match_tag: additional edge cases ──

#[test]
fn test_tag_exactly_30_chars_not_trimmed() {
    let conn = setup_test_db();
    let article_id = "art-tag-exact";
    insert_test_article(&conn, article_id);

    let exact_tag = "123456789012345678901234567890"; // exactly 30 chars
    assert_eq!(exact_tag.len(), 30);
    create_or_match_tag(&conn, exact_tag, article_id).unwrap();

    let name: String = conn
        .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, exact_tag);
}

#[test]
fn test_tag_short_name_unchanged() {
    let conn = setup_test_db();
    let article_id = "art-tag-short";
    insert_test_article(&conn, article_id);

    create_or_match_tag(&conn, "ml", article_id).unwrap();

    let name: String = conn
        .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "ml");
}

// ── augment_matched_from_reasoning tests ──

#[test]
fn test_augment_adds_missing_uuid_from_reasoning() {
    let mut global_map = std::collections::HashMap::new();
    global_map.insert("uuid-1".to_string(), 1);
    global_map.insert("uuid-2".to_string(), 2);

    let (inc, exc) = augment_matched_from_reasoning(
        "Matched uuid-1 based on criteria",
        &[], // no inclusion matched
        &[], // no exclusion matched
        &global_map,
        1, // 1 inclusion criterion
    );

    assert!(inc.contains(&"uuid-1".to_string()), "uuid-1 should be augmented into inclusion");
    assert!(!exc.contains(&"uuid-2".to_string()), "uuid-2 not in reasoning, should not appear");
}

#[test]
fn test_augment_no_duplicates() {
    let mut global_map = std::collections::HashMap::new();
    global_map.insert("uuid-1".to_string(), 1);

    let (inc, _exc) = augment_matched_from_reasoning(
        "uuid-1 mentioned",
        &["uuid-1".to_string()], // already matched
        &[],
        &global_map,
        1,
    );

    assert_eq!(inc.len(), 1, "Should not duplicate already-matched criterion");
}

// ── build_global_criterion_numbering tests ──

#[test]
fn test_global_numbering_sequential() {
    let inc1 = Criterion {
        id: "a".into(),
        text: "I1".into(),
        criterion_type: CriterionType::Inclusion,
        priority: Priority::Standard,
        created_at: String::new(),
    };
    let inc2 = Criterion {
        id: "b".into(),
        text: "I2".into(),
        criterion_type: CriterionType::Inclusion,
        priority: Priority::Standard,
        created_at: String::new(),
    };
    let exc1 = Criterion {
        id: "c".into(),
        text: "E1".into(),
        criterion_type: CriterionType::Exclusion,
        priority: Priority::Standard,
        created_at: String::new(),
    };

    let map = build_global_criterion_numbering(&[&inc1, &inc2], &[&exc1]);
    assert_eq!(map.get("a"), Some(&1));
    assert_eq!(map.get("b"), Some(&2));
    assert_eq!(map.get("c"), Some(&3)); // exclusion continues after inclusion
}
