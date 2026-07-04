//! Regression guard for the single-article AI summary system prompt.
//!
//! The prompt is loaded at compile time via `include_str!` and must:
//! - be non-empty,
//! - not be the stale "placeholder" text, and
//! - contain the JSON schema markers the frontend (`AiSummaryData`) depends on.
//!
//! Also regression-guards `strip_code_fences` against the screening
//! `extract_json` corruption bug (see `strip_code_fences_preserves_object_with_section_summaries`).
use bango_lib::screening::engine::extract_json;
use bango_lib::summary::prompt::{strip_code_fences, ARTICLE_SUMMARY_SYSTEM_PROMPT};

/// The prompt must never regress to the placeholder that caused
/// "Invalid JSON response from LLM: expected value at line 1 column 1".
#[test]
fn article_summary_prompt_is_not_placeholder() {
    assert!(
        !ARTICLE_SUMMARY_SYSTEM_PROMPT.trim().is_empty(),
        "article summary system prompt must not be empty"
    );
    let lower = ARTICLE_SUMMARY_SYSTEM_PROMPT.to_lowercase();
    assert!(
        !lower.contains("placeholder"),
        "article summary prompt must not be the placeholder text: {ARTICLE_SUMMARY_SYSTEM_PROMPT}"
    );
    assert!(
        !lower.contains("restore it from git history"),
        "article summary prompt must not contain stale placeholder instructions"
    );
}

/// The prompt must instruct the LLM to emit JSON and include the schema keys
/// the frontend parses in `parseAiSummary` (`src/composables/use-ai-summary.ts`).
#[test]
fn article_summary_prompt_requires_json_schema() {
    let lower = ARTICLE_SUMMARY_SYSTEM_PROMPT.to_lowercase();
    assert!(lower.contains("json"), "prompt must request JSON output");

    // Schema keys consumed by the frontend `AiSummaryData` interface.
    for key in [
        "field",
        "subfield",
        "structured_extraction",
        "summary_150_250_words",
        "key_insights",
        "keywords",
    ] {
        assert!(
            ARTICLE_SUMMARY_SYSTEM_PROMPT.contains(key),
            "prompt must document the '{key}' schema key"
        );
    }
}

// ── strip_code_fences regression tests ─────────────────────────────────────
//
// These guard against the bug where `screening_engine::extract_json` was used
// to clean summary responses. `extract_json` assumes a top-level JSON array
// (the screening shape) and unwraps the first nested array-of-objects out of
// a JSON object. The article-summary schema is a top-level object whose
// `section_summaries` field is an array-of-objects, so feeding a valid summary
// response through `extract_json` silently corrupted it into just the
// `section_summaries` array - breaking all top-level field access and
// triggering a spurious markdown-fallback retry. `strip_code_fences` is the
// summary-path replacement.

/// A minimal object response that mirrors the Cobiac GPT-5.4-mini shape: a
/// top-level object with a nested `section_summaries` array-of-objects. If a
/// cleaner corrupts this into the inner array, the assertions below fail.
fn sample_summary_object_response() -> &'static str {
    r#"{
        "schema_version": 2,
        "field": "medicine",
        "subfield": "public_health",
        "structured_extraction": {"study_type": "ITS"},
        "summary_150_250_words": "The SDIL reduced sugar purchasing.",
        "key_insights": ["15g/week reduction"],
        "keywords": ["SDIL"],
        "section_summaries": [
            {"section": "Methods", "summary": "ITS analysis.", "study_design": "ITS"},
            {"section": "Results", "summary": "15g reduction.", "effect_size": "15g"}
        ]
    }"#
}

/// `strip_code_fences` MUST return the object unchanged (trimmed). This is the
/// regression assertion: `extract_json` would corrupt this into the inner
/// `section_summaries` array.
#[test]
fn strip_code_fences_preserves_object_with_section_summaries() {
    let raw = sample_summary_object_response();
    let cleaned = strip_code_fences(raw);
    let parsed: serde_json::Value =
        serde_json::from_str(&cleaned).expect("strip_code_fences must yield valid JSON");
    // Top-level must remain an OBJECT, not an array.
    assert!(parsed.is_object(), "strip_code_fences must not unwrap the object into an array");
    // Top-level fields must be intact.
    assert_eq!(parsed.get("field").and_then(|v| v.as_str()), Some("medicine"));
    assert_eq!(
        parsed.get("summary_150_250_words").and_then(|v| v.as_str()),
        Some("The SDIL reduced sugar purchasing.")
    );
    // The nested array must still be nested (not hoisted to top level).
    let sections = parsed
        .get("section_summaries")
        .and_then(|v| v.as_array())
        .expect("section_summaries array must survive inside the object");
    assert_eq!(sections.len(), 2, "section_summaries must keep both entries");
}

/// Prove the contrast: `extract_json` (the screening helper) DOES corrupt the
/// same response by unwrapping the inner `section_summaries` array to the top
/// level. This test documents the bug we fixed and prevents re-introducing
/// `extract_json` on the summary path. If `extract_json` is ever fixed to be
/// shape-aware, this test can be updated or removed; until then it encodes the
/// known-bad behavior so reviewers understand why `strip_code_fences` exists.
#[test]
fn extract_json_corrupts_object_summary_into_inner_array() {
    let raw = sample_summary_object_response();
    let cleaned = extract_json(raw);
    let parsed: serde_json::Value =
        serde_json::from_str(&cleaned).expect("extract_json must still emit valid JSON");
    // Known-bad: top-level becomes an ARRAY (the inner section_summaries), not
    // the original object. All top-level summary fields (field, summary_150_250_words,
    // etc.) are lost. This is exactly the corruption that caused the empty-content
    // symptom misdiagnosed in findings.md.
    assert!(parsed.is_array(), "extract_json unwraps the inner array-of-objects");
    assert!(parsed.get("field").is_none(), "top-level field key is gone after corruption");
}

/// `strip_code_fences` must also handle code-fenced object responses (some
/// models wrap JSON in ```json fences despite the prompt asking them not to).
#[test]
fn strip_code_fences_strips_json_fenced_object() {
    let raw = format!("```json\n{}\n```", sample_summary_object_response().trim());
    let cleaned = strip_code_fences(&raw);
    let parsed: serde_json::Value =
        serde_json::from_str(&cleaned).expect("fenced object must parse after stripping");
    assert!(parsed.is_object());
    assert_eq!(parsed.get("field").and_then(|v| v.as_str()), Some("medicine"));
}

/// Bare-text whitespace trimming (no fence) passes through cleanly.
#[test]
fn strip_code_fences_trims_unfenced_object() {
    let raw = format!("   {}   ", sample_summary_object_response().trim());
    let cleaned = strip_code_fences(&raw);
    let parsed: serde_json::Value =
        serde_json::from_str(&cleaned).expect("unfenced object must parse after trimming");
    assert!(parsed.is_object());
}
