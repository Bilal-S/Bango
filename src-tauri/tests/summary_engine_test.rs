//! Coverage for summary::engine (generate_summary single + batch paths).
use std::sync::Arc;
use std::time::Duration;

use bango_lib::llm::orchestrator::LlmOrchestrator;
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};
use bango_lib::summary::engine::{generate_summary, SummaryInput};
use bango_lib::summary::prompt::{ArticleSummary, ScreeningData};

fn mock_openai_config(server_url: &str, context_window: i32) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Openai,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: Some("test-key".to_string()),
        model_name: "gpt-4o".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: context_window,
    }
}

fn openai_chat_response(content: &str) -> String {
    format!(
        r#"{{"choices": [{{ "message": {{ "role": "assistant", "content": {} }} }}], "usage": {{ "total_tokens": 10 }}}}"#,
        serde_json::to_string(content).unwrap(),
    )
}

fn empty_screening() -> ScreeningData {
    ScreeningData {
        records_identified: 0,
        duplicates_removed: 0,
        records_screened: 0,
        records_excluded: 0,
        records_excluded_with_reasons: 0,
        records_assessed: 0,
        records_in_progress: 0,
        studies_included: 0,
        ai_screened: 0,
        manual_reviewed: 0,
        exclusion_reasons: vec![],
    }
}

fn sample_article(title: &str, abstract_text: &str) -> ArticleSummary {
    ArticleSummary {
        title: title.to_string(),
        authors: vec!["Smith, J.".to_string()],
        year: Some(2021),
        abstract_text: abstract_text.to_string(),
        keywords: vec!["machine learning".to_string()],
        evidence: None,
    }
}

#[tokio::test]
async fn generate_summary_errors_when_no_articles() {
    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let input = SummaryInput::new(
        mock_openai_config("http://unused", 50_000),
        vec!["aim".to_string()],
        vec![],
        empty_screening(),
        "apa".to_string(),
        vec![],
        vec![],
    );
    let result = generate_summary(&orch, input).await;
    assert!(result.is_err());
    let err = result.expect_err("should error");
    assert!(err.to_string().contains("No included articles"));
}

#[tokio::test]
async fn generate_summary_single_batch_under_context_limit() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(openai_chat_response("# Literature Review\n\nSingle batch output."))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let config = mock_openai_config(&server.url(), 50_000);
    // Small article => estimated tokens well under 80% of 50_000
    let input = SummaryInput::new(
        config,
        vec!["aim 1".to_string()],
        vec![sample_article("Paper One", "Short abstract.")],
        empty_screening(),
        "apa".to_string(),
        vec![],
        vec![],
    );

    let summary = generate_summary(&orch, input).await.expect("summary ok");
    assert!(summary.contains("Single batch output"));
    mock.assert_async().await;
}

#[tokio::test]
async fn generate_summary_batches_when_over_context_limit() {
    let mut server = mockito::Server::new_async().await;
    // Two batch summaries + one synthesis = 3 calls
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_body(openai_chat_response("synthesized section"))
        .expect(3)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(3, 0));
    // Very small context window forces batching (estimated tokens > 80% of 100)
    let config = mock_openai_config(&server.url(), 100);
    let big_abstract = "x".repeat(200);
    let input = SummaryInput::new(
        config,
        vec!["aim".to_string()],
        vec![sample_article("Paper A", &big_abstract), sample_article("Paper B", &big_abstract)],
        empty_screening(),
        "vancouver".to_string(),
        vec![],
        vec![],
    );

    let summary = generate_summary(&orch, input).await.expect("summary ok");
    assert_eq!(summary, "synthesized section");
    mock.assert_async().await;
}

#[tokio::test]
async fn generate_summary_trims_response() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("  trimmed output  "))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let config = mock_openai_config(&server.url(), 50_000);
    let input = SummaryInput::new(
        config,
        vec![],
        vec![sample_article("T", "a")],
        empty_screening(),
        "apa".to_string(),
        vec![],
        vec![],
    );

    let summary = generate_summary(&orch, input).await.expect("summary ok");
    assert_eq!(summary, "trimmed output");
    mock.assert_async().await;
}

#[tokio::test]
async fn generate_summary_propagates_llm_error() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/chat/completions")
        .with_status(500)
        .with_body("internal error")
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let config = mock_openai_config(&server.url(), 50_000);
    let input = SummaryInput::new(
        config,
        vec![],
        vec![sample_article("T", "a")],
        empty_screening(),
        "apa".to_string(),
        vec![],
        vec![],
    );

    // Give the failing request a moment to settle
    let result =
        tokio::time::timeout(Duration::from_secs(30), generate_summary(&orch, input)).await;
    assert!(result.is_ok(), "should not time out");
    assert!(result.unwrap().is_err(), "should propagate LLM error");
}
