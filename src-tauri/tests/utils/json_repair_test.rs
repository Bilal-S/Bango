// Integration tests for `utils::json_repair::escape_control_chars_in_json`.
//
// These cover the LLM-output control-character failure mode end-to-end:
// reproductions of the bug report where the LLM placed literal `\n\n` (decoded
// from `\n\n` in the OpenAI envelope) inside the inner summary JSON's string
// values, plus an integration check that `screening::engine::extract_json`
// (which wraps the sanitizer) now accepts payloads that would previously have
// failed `serde_json::from_str`.

use bango_lib::screening::engine::extract_json;
use bango_lib::utils::json_repair::{escape_control_chars_in_json, prepare_llm_json};

/// Reproduces the exact failure mode from the bug report: an otherwise-valid
/// article-summary JSON whose `summary_150_250_words` value contains a real
/// `0x0A` newline byte (the byte `serde_json` rejects with "control character
/// (\u0000-\u001F) found while parsing a string").
///
/// Pre-fix: `serde_json::from_str(raw)` errors.
/// Post-fix: the sanitizer escapes the newline to `\n` and the payload parses,
/// preserving the logical paragraph break in the parsed value (data fidelity).
#[test]
fn end_to_end_article_summary_with_literal_newline_parses() {
    let raw = format!(
        "{{\
         \n\"field\": \"business_economics_finance\",\
         \n\"subfield\": \"green supply chain finance\",\
         \n\"summary_150_250_words\": \"This paper examines how digital \
         technology platforms influence strategic behavior in green supply \
         chain finance.{nl}{nl}The results indicate that platform access makes \
         desirable strategies more likely to emerge.\",\
         \n\"keywords\": [\"a\", \"b\"]\
         \n}}",
        nl = '\n'
    );

    // Sanity: the raw payload is NOT valid JSON.
    assert!(
        serde_json::from_str::<serde_json::Value>(&raw).is_err(),
        "raw payload should be invalid JSON (literal newline inside string value)"
    );

    let sanitized = escape_control_chars_in_json(&raw);
    let parsed: serde_json::Value =
        serde_json::from_str(&sanitized).expect("sanitized payload must parse as valid JSON");
    assert_eq!(parsed["field"].as_str().unwrap(), "business_economics_finance");
    assert_eq!(parsed["subfield"].as_str().unwrap(), "green supply chain finance");
    // The logical newline survives (data fidelity - escape, not strip).
    assert!(
        parsed["summary_150_250_words"].as_str().unwrap().contains('\n'),
        "newline must be preserved in the parsed value"
    );
    assert_eq!(parsed["keywords"][1].as_str().unwrap(), "b");
}

/// `extract_json` (screening path) must accept a top-level array of screening
/// responses whose `reasoning` field contains a literal newline - the most
/// common real-world failure vector for screening runs. Pre-fix, this would
/// have hit "control character" on `serde_json::from_str` and marked the batch
/// as an error; post-fix the sanitizer escapes the newline and the array
/// deserializes cleanly.
#[test]
fn extract_json_accepts_screening_array_with_literal_newline_in_reasoning() {
    // The reasoning field contains a real `0x0A` byte between two sentences.
    let raw = format!(
        "[{{\
         \"decision\": \"include\",\
         \"reasoning\": \"Criterion 1 is satisfied.{nl}Also criterion 2 is \
         supported by the abstract.\",\
         \"matched_inclusion_criteria\": [\"inc-1\"],\
         \"matched_exclusion_criteria\": [],\
         \"suggested_tags\": [\"tag\"],\
         \"confidence\": 0.85,\
         \"extracted_terms\": [\"term\"]\
         }}]",
        nl = '\n'
    );

    // Sanity: the raw payload is NOT valid JSON.
    assert!(serde_json::from_str::<serde_json::Value>(&raw).is_err());

    // `extract_json` runs the sanitizer as its first step, so the returned
    // string should be valid JSON.
    let repaired = extract_json(&raw);
    let parsed: serde_json::Value = serde_json::from_str(&repaired)
        .expect("extract_json output must be valid JSON after sanitization");
    assert_eq!(parsed[0]["decision"].as_str().unwrap(), "include");
    // The logical newline survives in `reasoning` (escape, not strip).
    assert!(parsed[0]["reasoning"].as_str().unwrap().contains('\n'));
}

/// `prepare_llm_json` chains `strip_code_fences` + `escape_control_chars_in_json`
/// in the correct order. This is the helper used by
/// `LlmOrchestrator::send_json`, so it's the contract every JSON-returning
/// orchestrator call relies on.
#[test]
fn prepare_llm_json_strips_fences_then_escapes_control_chars() {
    // A code-fenced JSON payload whose string value contains a literal newline.
    // `strip_code_fences` MUST run before `escape_control_chars_in_json` so the
    // leading ``` is matched against the raw response.
    let raw = format!("```json\n{{\"summary\": \"line one{nl}line two\"}}\n```", nl = '\n');
    // Sanity: raw is not valid JSON (fence + literal newline).
    assert!(serde_json::from_str::<serde_json::Value>(&raw).is_err());

    let prepared = prepare_llm_json(&raw);
    let v: serde_json::Value =
        serde_json::from_str(&prepared).expect("prepare_llm_json output must be valid JSON");
    assert_eq!(
        v["summary"].as_str().unwrap(),
        "line one
line two"
    );
}

/// `prepare_llm_json` is a no-op for an already-clean JSON document.
#[test]
fn prepare_llm_json_is_noop_for_clean_json() {
    let valid = r#"{"field":"x","arr":[1,2,"y"]}"#;
    assert_eq!(prepare_llm_json(valid), valid);
}

/// Defense-in-depth: `escape_control_chars_in_json` is a strict no-op for
/// already-valid JSON. This guards against the regression where the sanitizer
/// might mangle clean LLM output.
#[test]
fn sanitizer_is_noop_for_valid_json() {
    let valid = r#"{"field":"x","arr":[1,2,"y"],"nested":{"k":"v\nw"}}"#;
    assert_eq!(escape_control_chars_in_json(valid), valid);
    // And it parses identically.
    let a: serde_json::Value = serde_json::from_str(valid).unwrap();
    let b: serde_json::Value = serde_json::from_str(&escape_control_chars_in_json(valid)).unwrap();
    assert_eq!(a, b);
}
