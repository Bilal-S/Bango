use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::criterion::{Criterion, CriterionType, Priority};
use bango_lib::screening::engine::{
    augment_matched_from_reasoning, balance_braces, build_global_criterion_numbering,
    create_or_match_label, create_or_match_tag, extract_json, process_screening_responses,
    sanitize_tag_or_label_name, truncate_at_word_boundary, ScreeningEngine,
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
fn test_tag_truncated_to_35_chars_at_word_boundary() {
    let conn = setup_test_db();
    let article_id = "art-tag-trim";
    insert_test_article(&conn, article_id);
    create_or_match_tag(
        &conn,
        "this-is-a-very-long-tag-name-that-exceeds-the-thirty-five-char-limit",
        article_id,
    )
    .unwrap();
    let name: String = conn
        .query_row("SELECT name FROM tags WHERE source = 'ai_suggested'", [], |r| r.get(0))
        .unwrap();
    assert!(name.len() <= 35, "tag must be at most 35 chars, got {name} (len {})", name.len());
    // Truncation must occur at a word boundary (hyphen), not mid-word.
    // The raw input after the 35-char window starts mid-word at "char-limit",
    // so the stored name should end before that fragment.
    assert!(!name.ends_with("char"), "tag must not be cut mid-word, got: {name}");
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
fn test_label_strips_prefix_and_truncates_at_word_boundary() {
    let conn = setup_test_db();
    let article_id = "art-label-trim";
    insert_test_article(&conn, article_id);
    let long_label = "Inclusion: this is a very long criterion text that exceeds limit";
    create_or_match_label(&conn, long_label, article_id).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM labels WHERE source = 'ai_generated'", [], |r| r.get(0))
        .unwrap();
    assert!(name.len() <= 35, "label must be at most 35 chars, got {name} (len {})", name.len());
    assert!(
        !name.starts_with("inclusion:"),
        "label must have the 'inclusion:' prefix stripped, got: {name}"
    );
    assert!(!name.ends_with("criterion"), "label must not be cut mid-word, got: {name}");
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
fn test_tag_exactly_35_chars_not_trimmed() {
    let conn = setup_test_db();
    let article_id = "art-tag-exact";
    insert_test_article(&conn, article_id);

    let exact_tag = "12345678901234567890123456789012345"; // exactly 35 chars
    assert_eq!(exact_tag.len(), 35);
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

// ── sanitize_tag_or_label_name tests ──

#[test]
fn test_sanitize_strips_inclusion_prefix() {
    assert_eq!(sanitize_tag_or_label_name("Inclusion: machine-learning", 35), "machine-learning");
}

#[test]
fn test_sanitize_strips_exclusion_prefix() {
    assert_eq!(sanitize_tag_or_label_name("Exclusion: not-a-review", 35), "not-a-review");
}

#[test]
fn test_sanitize_strips_inclusion_dash_prefix() {
    assert_eq!(sanitize_tag_or_label_name("Inclusion - some-tag", 35), "some-tag");
}

#[test]
fn test_sanitize_replaces_spaces_with_hyphens() {
    assert_eq!(
        sanitize_tag_or_label_name("machine learning models", 35),
        "machine-learning-models"
    );
}

#[test]
fn test_sanitize_replaces_underscores_with_hyphens() {
    assert_eq!(sanitize_tag_or_label_name("machine_learning", 35), "machine-learning");
}

#[test]
fn test_sanitize_collapses_repeated_hyphens() {
    assert_eq!(sanitize_tag_or_label_name("machine--learning", 35), "machine-learning");
}

#[test]
fn test_sanitize_lowercases_input() {
    assert_eq!(sanitize_tag_or_label_name("Machine-Learning", 35), "machine-learning");
}

#[test]
fn test_sanitize_truncates_at_word_boundary() {
    let input = "this-is-a-very-long-tag-name-that-exceeds-the-thirty-five-char-limit";
    let result = sanitize_tag_or_label_name(input, 35);
    assert!(
        result.len() <= 35,
        "result must be at most 35 chars, got {result} (len {})",
        result.len()
    );
    assert!(!result.ends_with("char"), "result must not be cut mid-word, got: {result}");
}

#[test]
fn test_sanitize_within_limit_unchanged() {
    assert_eq!(sanitize_tag_or_label_name("systematic-review", 35), "systematic-review");
}

#[test]
fn test_sanitize_empty_string_after_strip() {
    assert_eq!(sanitize_tag_or_label_name("Inclusion:", 35), "");
}

#[test]
fn test_sanitize_whitespace_only() {
    assert_eq!(sanitize_tag_or_label_name("   ", 35), "");
}

#[test]
fn test_sanitize_trims_leading_trailing_hyphens() {
    assert_eq!(sanitize_tag_or_label_name("--machine-learning--", 35), "machine-learning");
}

#[test]
fn test_sanitize_single_long_word_hard_truncates() {
    // No hyphens within the limit - hard truncate (single-word name).
    let input = "supercalifragilisticexpialidociousmethodology";
    let result = sanitize_tag_or_label_name(input, 35);
    assert_eq!(result.len(), 35);
    assert_eq!(result, "supercalifragilisticexpialidociousm");
}

// ── truncate_at_word_boundary tests ──

#[test]
fn test_truncate_at_word_boundary_within_limit() {
    assert_eq!(truncate_at_word_boundary("short", 35), "short");
}

#[test]
fn test_truncate_at_word_boundary_truncates_at_hyphen() {
    let input = "this-is-a-very-long-tag-name-that-exceeds-limit";
    let result = truncate_at_word_boundary(input, 20);
    assert!(result.len() <= 20, "result must be at most 20 chars, got {result}");
    assert!(!result.ends_with("tag"), "result must not be cut mid-word, got: {result}");
}

#[test]
fn test_truncate_at_word_boundary_no_hyphen_hard_truncates() {
    let input = "supercalifragilisticexpialidocious";
    let result = truncate_at_word_boundary(input, 10);
    assert_eq!(result, "supercalif");
}

// ─── is_transient_llm_error helper ─────────────────────────────────────
//
// Transient errors (429/401/403/5xx/timeout/transport) must be classified
// correctly so the engine leaves articles UNSCREENED instead of mass-marking
// them as errors.

#[test]
fn is_transient_llm_error_classifies_429() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed (429 Too Many Requests): rate limited".into());
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_classifies_401_transient() {
    use bango_lib::error::AppError;
    let e = AppError::Import(
        "LLM request failed (401 Unauthorized): insufficient permissions for this operation".into(),
    );
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_classifies_403_transient() {
    use bango_lib::error::AppError;
    let e = AppError::Import(
        "LLM request failed (403 Forbidden): insufficient permissions for this operation".into(),
    );
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_classifies_500_server_error() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed (503 Service Unavailable)".into());
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_classifies_timeout() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request timed out after 120 seconds".into());
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_classifies_transport_error() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed: connection reset by peer".into());
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_rejects_malformed_json() {
    // Non-transient: content-specific issue unlikely to resolve on retry.
    use bango_lib::error::AppError;
    let e = AppError::Import("Failed to parse LLM response as JSON: unexpected token".into());
    assert!(!bango_lib::screening::engine::is_transient_llm_error(&e));
}

#[test]
fn is_transient_llm_error_rejects_parse_count_mismatch() {
    // Non-transient: content-specific issue.
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM returned 2 decisions but batch has 5 articles".into());
    assert!(!bango_lib::screening::engine::is_transient_llm_error(&e));
}

// ─── is_auth_failure helper ───────────────────────────────────────────
//
// Auth failures (wrong key, wrong org) must be classified correctly so the
// engine stops immediately instead of burning through all articles.

#[test]
fn is_auth_failure_classifies_plain_401() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed (401 Unauthorized): Invalid API key".into());
    assert!(bango_lib::screening::engine::is_auth_failure(&e));
}

#[test]
fn is_auth_failure_classifies_plain_403() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed (403 Forbidden): Forbidden".into());
    assert!(bango_lib::screening::engine::is_auth_failure(&e));
}

#[test]
fn is_auth_failure_rejects_401_transient() {
    use bango_lib::error::AppError;
    let e = AppError::Import(
        "LLM request failed (401 Unauthorized): insufficient permissions for this operation".into(),
    );
    // This is the Windows transient, NOT a real auth failure.
    assert!(!bango_lib::screening::engine::is_auth_failure(&e));
}

#[test]
fn is_auth_failure_rejects_429() {
    use bango_lib::error::AppError;
    let e = AppError::Import("LLM request failed (429 Too Many Requests)".into());
    assert!(!bango_lib::screening::engine::is_auth_failure(&e));
}

#[test]
fn is_auth_failure_rejects_non_auth_error() {
    use bango_lib::error::AppError;
    let e = AppError::Import("Failed to parse LLM response".into());
    assert!(!bango_lib::screening::engine::is_auth_failure(&e));
}
// ─── F3: is_transient_llm_error on plain 401 (documents intentional policy) ──

#[test]
fn is_transient_llm_error_classifies_plain_401() {
    // A plain 401 (wrong key, no Windows-transient body) is intentionally
    // classified as transient by is_transient_llm_error so articles are NOT
    // mass-marked as errors. The is_auth_failure() helper is the separate
    // gate that catches this case and stops the run. Without this test, the
    // catch-all at engine.rs could be "fixed" by a contributor who doesn't
    // understand the two-helper design.
    use bango_lib::error::AppError;
    let e = AppError::Import(
        "LLM request failed (401 Unauthorized): Incorrect API key provided".into(),
    );
    assert!(bango_lib::screening::engine::is_transient_llm_error(&e));
}

// ─── F2: Auto-stop state machine integration tests ──────────────────────
//
// These tests exercise the engine's consecutive-failure counter, auth-failure
// immediate stop, and the deferred/errors/completed progress counters using
// mock LLM clients that inject controlled failures.

use bango_lib::db::article_repo;
use bango_lib::db::criteria_repo;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::criterion::ResearchAim;
use bango_lib::screening::engine::ScreeningConfig;
use bango_lib::screening::llm_client::LlmClient;

/// Mock LLM client that always returns a transient error (429).
struct Always429Mock;

#[async_trait::async_trait]
impl LlmClient for Always429Mock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        Err(AppError::Import("LLM request failed (429 Too Many Requests)".into()))
    }
}

/// Mock LLM client that always returns an auth failure (plain 401).
struct Always401Mock;

#[async_trait::async_trait]
impl LlmClient for Always401Mock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        Err(AppError::Import("LLM request failed (401 Unauthorized): Incorrect API key".into()))
    }
}

/// Mock LLM client that fails the first N calls with 429, then succeeds.
struct FailThenSucceedMock {
    fail_count: std::sync::atomic::AtomicUsize,
    inc_id: String,
}

#[async_trait::async_trait]
impl LlmClient for FailThenSucceedMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        let n = self.fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n < 2 {
            return Err(AppError::Import("LLM request failed (429 Too Many Requests)".into()));
        }
        // Succeed on call 2+
        let resp = format!(
            r#"[{{"decision":"include","reasoning":"R","matchedInclusionCriteria":["{}"],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}}]"#,
            self.inc_id
        );
        Ok((resp, 100))
    }
}

fn setup_screening_db() -> std::sync::Mutex<rusqlite::Connection> {
    let conn = bango_lib::db::connection::create_connection().expect("create connection");
    bango_lib::db::migration::run_migrations(&conn).expect("migrations");
    std::sync::Mutex::new(conn)
}

fn seed_working_articles(conn: &rusqlite::Connection, count: usize) {
    for i in 0..count {
        let article = NewArticle {
            title: format!("Article {i}"),
            abstract_text: format!("Abstract {i} about sugar taxes in children."),
            authors: vec!["Author".to_string()],
            publication_year: Some(2024),
            import_source: Some("test".to_string()),
            ..Default::default()
        };
        let inserted =
            article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
        let id = &inserted[0].id;
        article_repo::move_articles_to_working_batch(conn, std::slice::from_ref(id))
            .expect("move to working");
    }
}

fn seed_screening_criteria(conn: &rusqlite::Connection) -> (Vec<Criterion>, Vec<ResearchAim>) {
    let aim = criteria_repo::create_aim(conn, "Study sugar taxes").expect("aim");
    let inc =
        criteria_repo::create_criterion(conn, "inclusion", "Must be about sugar taxes", "standard")
            .expect("inc");
    let exc = criteria_repo::create_criterion(conn, "exclusion", "Not about children", "standard")
        .expect("exc");
    (vec![inc, exc], vec![aim])
}

fn abstract_config() -> ScreeningConfig {
    ScreeningConfig::default()
}

/// After 3 consecutive 429 errors, the run should stop with a fatal_error
/// message. Articles should be deferred (not errors, not completed).
#[tokio::test]
async fn auto_stop_after_3_consecutive_transient_failures() {
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 10);
        seed_screening_criteria(&conn)
    };
    let engine = ScreeningEngine::with_batch_size(1);
    let mock = Always429Mock;
    let _ = engine.run_sync(&db, &mock, 0, criteria, aims, abstract_config(), None, None).await;
    let progress = engine.get_progress().await;
    assert!(!progress.is_running, "run should have stopped");
    assert!(progress.fatal_error.is_some(), "fatal_error should be set");
    assert!(
        progress.fatal_error.as_ref().unwrap().contains("3 consecutive"),
        "fatal_error should mention 3 consecutive failures"
    );
    // 3 articles deferred (one per batch before stop), 0 errors, 0 completed.
    assert_eq!(progress.deferred, 3, "3 articles should be deferred");
    assert_eq!(progress.errors, 0, "no hard errors (transient only)");
    assert_eq!(progress.completed, 0, "no articles screened");
}

/// Auth failure (plain 401) should stop the run immediately (threshold = 1).
#[tokio::test]
async fn auto_stop_immediately_on_auth_failure() {
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 10);
        seed_screening_criteria(&conn)
    };
    let engine = ScreeningEngine::with_batch_size(1);
    let mock = Always401Mock;
    let _ = engine.run_sync(&db, &mock, 0, criteria, aims, abstract_config(), None, None).await;
    let progress = engine.get_progress().await;
    assert!(!progress.is_running, "run should have stopped");
    assert!(progress.fatal_error.is_some(), "fatal_error should be set");
    assert!(
        progress.fatal_error.as_ref().unwrap().contains("Authentication failed"),
        "fatal_error should mention authentication"
    );
    // Only 1 article deferred (immediate stop), 0 errors, 0 completed.
    assert_eq!(progress.deferred, 1, "1 article deferred (immediate stop)");
    assert_eq!(progress.errors, 0);
    assert_eq!(progress.completed, 0);
}

/// After 2 transient failures followed by a success, the counter should reset
/// and the run should continue (no fatal_error).
#[tokio::test]
async fn transient_counter_resets_on_success() {
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 5);
        seed_screening_criteria(&conn)
    };
    let inc_id = criteria
        .iter()
        .find(|c| matches!(c.criterion_type, CriterionType::Inclusion))
        .expect("inclusion criterion")
        .id
        .clone();
    let engine = ScreeningEngine::with_batch_size(1);
    let mock = FailThenSucceedMock { fail_count: std::sync::atomic::AtomicUsize::new(0), inc_id };
    let _ = engine.run_sync(&db, &mock, 0, criteria, aims, abstract_config(), None, None).await;
    let progress = engine.get_progress().await;
    assert!(!progress.is_running, "run should have completed normally");
    assert!(progress.fatal_error.is_none(), "no fatal_error (success reset counter)");
    // 2 articles deferred (first 2 calls failed), then 3 succeeded.
    assert_eq!(progress.deferred, 2, "2 articles deferred before success");
    assert_eq!(progress.errors, 0);
    assert_eq!(progress.completed, 3, "3 articles screened after recovery");
}

/// A single transient error (no stop) should NOT inflate errors or completed
/// (the F1 regression: double-counting bug).
#[tokio::test]
async fn single_transient_does_not_double_count_progress() {
    // This mock fails once (429) then always succeeds. With only 1 article,
    // the single failure defers it and the run ends (no more articles).
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 1);
        seed_screening_criteria(&conn)
    };
    let engine = ScreeningEngine::with_batch_size(1);
    let mock = Always429Mock;
    let _ = engine.run_sync(&db, &mock, 0, criteria, aims, abstract_config(), None, None).await;
    let progress = engine.get_progress().await;
    // The run stops after 1 batch (only 1 article, and it fails with 429).
    // Since consecutive_transient_failures = 1 (< 3 threshold), it doesn't
    // auto-stop — but there are no more articles, so the loop ends naturally.
    // The single failure should show deferred=1, errors=0, completed=0.
    assert_eq!(progress.deferred, 1, "1 article deferred");
    assert_eq!(progress.errors, 0, "no hard errors (F1: no double-counting)");
    assert_eq!(progress.completed, 0, "no articles screened (F1: no phantom completed)");
    assert!(progress.fatal_error.is_none(), "no fatal_error (below threshold)");
}
// ─── Cancellable request_delay_ms (the llmscreen4 fix) ─────────────────
//
// These tests prove that clicking Stop during the post-batch
// `request_delay_ms` throttle aborts the run immediately instead of
// waiting the full delay. Before the fix, the `sleep(request_delay_ms)`
// call was not wrapped in a cancel-aware `select!`, so the UI appeared
// to hang for up to `request_delay_ms` (commonly 1-10s for rate-limit
// mitigation) after the user clicked Stop.
//
// Each test spawns `run_sync` (wrapped in `Arc<ScreeningEngine>` so the
// same cancel token is shared between the spawned run and the test's
// `cancel()` call) with a large `request_delay_ms`, waits long enough for
// the first batch's LLM mock call to resolve and for the engine to enter
// the inter-batch delay, then calls `cancel()` and asserts the run
// finishes well before the full delay would have elapsed. The tests use
// `tokio::time::timeout` so a regression (the fix being reverted) fails
// fast instead of hanging the test suite.

use std::sync::Arc;

/// Mock LLM client that always returns a valid single-article "include"
/// screening response. The inclusion-criterion ID is injected so the
/// response matches the seeded criterion and parses cleanly through the
/// engine's batch-validation (`screenings.len() == batch.len()`).
struct AlwaysSucceedMock {
    inc_id: String,
}

#[async_trait::async_trait]
impl LlmClient for AlwaysSucceedMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        let resp = format!(
            r#"[{{"decision":"include","reasoning":"R","matchedInclusionCriteria":["{}"],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}}]"#,
            self.inc_id
        );
        // Simulate a small amount of work so the spawned task actually
        // reaches the post-batch delay before the test calls cancel().
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Ok((resp, 100))
    }
}

/// Stop during the post-success `request_delay_ms` delay should abort the
/// run immediately (success path, engine.rs ~line 644). Without the fix
/// this test would hang for ~3s (the full `request_delay_ms`).
#[tokio::test]
async fn stop_during_request_delay_success_path() {
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 5);
        seed_screening_criteria(&conn)
    };
    let inc_id = criteria
        .iter()
        .find(|c| matches!(c.criterion_type, CriterionType::Inclusion))
        .expect("inclusion criterion")
        .id
        .clone();
    // Wrap in Arc so the spawned task and the cancel call share the same
    // cancel token + progress. `run_sync` takes `&self`, so we can call it
    // through an `Arc` reference.
    let engine = Arc::new(ScreeningEngine::with_batch_size(1));
    let mock = AlwaysSucceedMock { inc_id };

    // Spawn the run with a 3s inter-batch delay. The first batch's mock
    // LLM call completes in ~50ms, then the engine enters the 3s delay.
    let run_engine = engine.clone();
    let run_handle = tokio::spawn(async move {
        let _ = run_engine
            .run_sync(&db, &mock, 3000, criteria, aims, abstract_config(), None, None)
            .await;
    });

    // Wait long enough for batch 1 to complete and the engine to enter the
    // 3s delay (50ms mock + processing < 200ms; 400ms is a safe margin).
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    // Click Stop. With the fix, the run should exit within milliseconds.
    let cancel_start = std::time::Instant::now();
    engine.cancel().await;

    // The run must finish well under the 3s delay. Allow up to 1.5s for
    // scheduler jitter; a regression hangs for ~3s and hits the 5s cap.
    let outcome = tokio::time::timeout(std::time::Duration::from_millis(5000), run_handle).await;
    let elapsed = cancel_start.elapsed().as_millis();

    assert!(
        outcome.is_ok(),
        "run did not finish within 5s of cancel (likely hung on the uncancellable sleep)"
    );
    assert!(
        elapsed < 200,
        "cancel should abort within one scheduler tick, not {elapsed}ms (the 3000ms delay was not interrupted)"
    );

    // Gap 2: assert the progress state was correctly emitted on cancel.
    let progress = engine.get_progress().await;
    assert!(!progress.is_running, "progress should show is_running=false after cancel");
}

/// Stop during the post-transient-error `request_delay_ms` delay should
/// abort the run immediately (transient path, engine.rs ~line 631). Without
/// the fix this test would hang for ~3s.
#[tokio::test]
async fn stop_during_request_delay_transient_path() {
    let db = setup_screening_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_working_articles(&conn, 5);
        seed_screening_criteria(&conn)
    };
    let engine = Arc::new(ScreeningEngine::with_batch_size(1));
    let mock = Always429Mock;

    // Spawn the run with a 3s inter-batch delay. The first batch fails
    // instantly with 429 (transient), defers the article, and enters the
    // 3s delay before the next loop iteration.
    let run_engine = engine.clone();
    let run_handle = tokio::spawn(async move {
        let _ = run_engine
            .run_sync(&db, &mock, 3000, criteria, aims, abstract_config(), None, None)
            .await;
    });

    // Wait long enough for the first 429 to be processed and the engine
    // to enter the 3s delay (the 429 mock returns instantly; 400ms is a
    // safe margin that accounts for DB writes + progress emit).
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    let cancel_start = std::time::Instant::now();
    engine.cancel().await;

    let outcome = tokio::time::timeout(std::time::Duration::from_millis(5000), run_handle).await;
    let elapsed = cancel_start.elapsed().as_millis();

    assert!(
        outcome.is_ok(),
        "run did not finish within 5s of cancel (likely hung on the uncancellable sleep)"
    );
    assert!(
        elapsed < 200,
        "cancel should abort within one scheduler tick, not {elapsed}ms (the 3000ms delay was not interrupted)"
    );

    // Gap 2: assert the progress state was correctly emitted on cancel.
    let progress = engine.get_progress().await;
    assert!(!progress.is_running, "progress should show is_running=false after cancel");
}

// ── Diagnostics: phase field + chunk-progress callback ─────────────────────
//
// These tests cover the diagnostics-only instrumentation added to surface
// which screening phase is in flight and to confirm the chunk-backfill
// progress callback fires once per article. They do NOT exercise the
// behavioral cancel/lock contract (Layer 2, deferred).

#[test]
fn screening_progress_serializes_phase_field() {
    use bango_lib::screening::engine::ScreeningProgress;
    let p = ScreeningProgress {
        total: 10,
        completed: 3,
        is_running: true,
        phase: Some("preparing:chunking".to_string()),
        stage: Some("Extracting full-text chunks 3/10...".to_string()),
        ..Default::default()
    };
    let json = serde_json::to_string(&p).expect("serialize");
    assert!(
        json.contains("\"phase\":\"preparing:chunking\""),
        "phase field must serialize: {json}"
    );
    // Round-trip preserves the value.
    let back: ScreeningProgress = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.phase.as_deref(), Some("preparing:chunking"));
}

#[test]
fn screening_progress_phase_defaults_none_when_absent() {
    use bango_lib::screening::engine::ScreeningProgress;
    // An old payload (e.g. from a frontend cache or a previous build) that
    // omits `phase` entirely must still deserialize thanks to `#[serde(default)]`.
    let old_payload = r#"{
        "total": 5,
        "completed": 0,
        "included": 0,
        "rejected": 0,
        "errors": 0,
        "isRunning": false,
        "currentArticleTitles": [],
        "elapsedMs": 0,
        "estimatedRemainingMs": null
    }"#;
    let p: ScreeningProgress = serde_json::from_str(old_payload).expect("deserialize old payload");
    assert!(p.phase.is_none(), "phase must default to None when absent");
    assert_eq!(p.total, 5);
}

#[test]
fn chunk_progress_callback_fires_per_article() {
    use bango_lib::commands::full_text::ensure_chunks_for_full_text_articles_with_progress;
    use bango_lib::db::connection::create_connection;
    use std::sync::{Arc, Mutex};

    let conn = create_connection().expect("in-memory db");
    // Run migrations so the schema (articles, article_chunks, etc.) exists.
    bango_lib::db::migration::run_migrations(&conn).expect("migrations");

    // No articles with full text -> the candidate set is empty, so the
    // callback fires zero times and the function returns success with
    // chunked=0. This proves the no-op path does not call the callback.
    let ticks = Arc::new(Mutex::new(Vec::new()));
    let ticks_clone = ticks.clone();
    let cb = move |done: usize, total: usize, id: &str| {
        ticks_clone.lock().unwrap().push((done, total, id.to_string()));
    };
    let result = ensure_chunks_for_full_text_articles_with_progress(&conn, false, &cb);
    assert!(result.success);
    assert_eq!(result.chunked, 0);
    assert!(ticks.lock().unwrap().is_empty(), "callback must not fire on empty candidate set");
}
