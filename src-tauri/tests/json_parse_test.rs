// Direct unit tests for `screening::json_parse` repair + extract helpers (Gap 6).
//
// `extract_json` has 2 integration tests via `json_repair_test.rs`, but
// `repair_truncated_json_array` and `balance_braces` had no direct unit tests
// before this file - they were covered only transitively through end-to-end
// screening tests that happened to exercise truncated/malformed LLM output.

use bango_lib::screening::json_parse::{
    balance_braces, extract_json, process_screening_responses, repair_truncated_json_array,
};

// ── repair_truncated_json_array ─────────────────────────────────────────────

#[test]
fn repair_truncated_json_array_closes_incomplete_array() {
    // A valid prefix of an array (two complete objects + a trailing partial)
    // must be closed at the last complete `}` and have `]` appended.
    let truncated = "[{\"decision\":\"include\"},{\"decision\":\"exclude\"},{\"decision\":\"in";
    let repaired =
        repair_truncated_json_array(truncated).expect("truncated array should be repairable");

    // The repaired string must now parse as a valid JSON array.
    let parsed: serde_json::Value =
        serde_json::from_str(&repaired).expect("repaired output must be valid JSON");
    assert!(parsed.is_array(), "repaired output must be an array");
    let arr = parsed.as_array().expect("array");
    assert_eq!(arr.len(), 2, "only the 2 complete objects should survive");
    assert_eq!(arr[0]["decision"].as_str().unwrap(), "include");
    assert_eq!(arr[1]["decision"].as_str().unwrap(), "exclude");
}

#[test]
fn repair_truncated_json_array_none_for_already_complete() {
    // An array that already ends with `]` is not truncated -> None.
    let complete = "[{\"decision\":\"include\"}]";
    assert!(
        repair_truncated_json_array(complete).is_none(),
        "already-complete array must return None"
    );
}

#[test]
fn repair_truncated_json_array_none_for_non_array() {
    // A JSON object (not an array) is out of scope -> None.
    let obj = "{\"decision\":\"include\"}";
    assert!(repair_truncated_json_array(obj).is_none(), "non-array input must return None");

    // Bare prose also returns None.
    assert!(
        repair_truncated_json_array("not json at all").is_none(),
        "non-array prose must return None"
    );
}

// ── balance_braces ──────────────────────────────────────────────────────────

#[test]
fn balance_braces_appends_missing_closing() {
    // One opening brace, zero closing -> one `}` appended.
    let input = "{\"key\":\"value\"";
    let balanced = balance_braces(input);
    let parsed: serde_json::Value =
        serde_json::from_str(&balanced).expect("balanced output must be valid JSON");
    assert_eq!(parsed["key"].as_str().unwrap(), "value");
    assert!(balanced.ends_with('}'), "missing closing brace must be appended: {balanced}");
}

#[test]
fn balance_braces_prepends_missing_opening() {
    // One closing brace, zero opening -> one `{` prepended.
    let input = "\"key\":\"value\"}";
    let balanced = balance_braces(input);
    // The prepended `{` makes it a complete object.
    assert!(balanced.starts_with('{'), "missing opening brace must be prepended: {balanced}");
    // It should now parse (the structural braces are balanced).
    assert!(
        serde_json::from_str::<serde_json::Value>(&balanced).is_ok(),
        "balanced output must be valid JSON: {balanced}"
    );
}

#[test]
fn balance_braces_noop_for_balanced() {
    // Already-balanced input passes through unchanged.
    let balanced_input = "{\"a\":{\"b\":1}}";
    assert_eq!(balance_braces(balanced_input), balanced_input, "balanced input must be a no-op");
}

#[test]
fn balance_braces_ignores_braces_inside_string_literals() {
    // A `{` or `}` inside a JSON string value must NOT count toward the
    // structural balance. Here the only structural braces are the outer pair;
    // the `{` inside the string is decoration.
    let input = "{\"text\":\"a {nested} brace\"";
    let balanced = balance_braces(input);
    // The structural count: open=1 (the leading `{`), close=0. The `{` and `}`
    // inside the string are ignored. So one `}` is appended.
    assert!(balanced.ends_with('}'), "string-literal braces must not affect balance: {balanced}");
    // Verify the in-string braces survived (data fidelity).
    assert!(balanced.contains("{nested}"), "in-string braces must be preserved");
}

// ── extract_json ────────────────────────────────────────────────────────────

#[test]
fn extract_json_strips_code_fence() {
    // A ```json fence wrapping a bare array must be stripped, returning the
    // inner array.
    let fenced = "```json\n[{\"decision\":\"include\"}]\n```";
    let extracted = extract_json(fenced);
    let parsed: serde_json::Value =
        serde_json::from_str(&extracted).expect("fence-stripped output must parse");
    assert!(parsed.is_array());
    assert_eq!(parsed[0]["decision"].as_str().unwrap(), "include");
}

#[test]
fn extract_json_passes_through_bare_array() {
    // A bare array (no fence, no wrapping object) is already correct.
    let bare = "[{\"decision\":\"include\"}]";
    let extracted = extract_json(bare);
    // The extracted string should itself parse as the same array.
    let parsed: serde_json::Value =
        serde_json::from_str(&extracted).expect("bare array must parse after extract_json");
    assert_eq!(parsed[0]["decision"].as_str().unwrap(), "include");
}

#[test]
fn extract_json_extracts_array_from_wrapping_object() {
    // Some LLMs wrap the array in a top-level object like
    // `{"results": [...]}`. extract_json must pull out the inner array.
    let wrapped = "{\"results\": [{\"decision\":\"include\"}]}";
    let extracted = extract_json(wrapped);
    let parsed: serde_json::Value =
        serde_json::from_str(&extracted).expect("extracted inner array must parse");
    assert!(parsed.is_array(), "wrapping object must be stripped, leaving the array");
    assert_eq!(parsed[0]["decision"].as_str().unwrap(), "include");
}

// ── process_screening_responses ─────────────────────────────────────────────

#[test]
fn process_screening_responses_normalizes_decision_case() {
    // The LLM may emit "INCLUDE" / "Include"; the parser must lowercase to
    // "include". An unexpected decision value must be coerced to "error".
    let raw = "[\
        {\"decision\":\"INCLUDE\",\"reasoning\":\"r\",\"matched_inclusion_criteria\":[],\"matched_exclusion_criteria\":[]},\
        {\"decision\":\"Exclude\",\"reasoning\":\"r\",\"matched_inclusion_criteria\":[],\"matched_exclusion_criteria\":[]},\
        {\"decision\":\"maybe\",\"reasoning\":\"r\",\"matched_inclusion_criteria\":[],\"matched_exclusion_criteria\":[]}\
    ]";

    let results =
        process_screening_responses(raw).expect("valid array with mixed-case decisions must parse");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].decision, "include", "INCLUDE must normalize to include");
    assert_eq!(results[1].decision, "exclude", "Exclude must normalize to exclude");
    assert_eq!(results[2].decision, "error", "unexpected decision value must coerce to error");
    assert!(
        results[2].reasoning.contains("maybe"),
        "error-coerced reasoning must preserve the original value: {}",
        results[2].reasoning
    );
}
