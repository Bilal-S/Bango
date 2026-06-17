//! Regression guard for the single-article AI summary system prompt.
//!
//! The prompt is loaded at compile time via `include_str!` and must:
//! - be non-empty,
//! - not be the stale "placeholder" text, and
//! - contain the JSON schema markers the frontend (`AiSummaryData`) depends on.
use bango_lib::summary::prompt::ARTICLE_SUMMARY_SYSTEM_PROMPT;

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
