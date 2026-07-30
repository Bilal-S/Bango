//! Integration tests for the v2 orchestrator batch primitives.
//!
//! Covers:
//! - [`send_batch_parallel`]: order preservation under out-of-order completion,
//!   mixed Ok/Err per-batch reporting, panic-in-one-task fills slot with Err
//!   without deadlocking.
//! - [`send_embedding_batch_parallel`]: mockito-backed cases for sub-batch
//!   splitting at limit boundaries, per-text overflow → pooled into one
//!   vector, dimension reporting, ordering when sub-batches complete out of
//!   order.
//! - [`embedding_limits`]: per-provider table (OpenAI 2048/8191/300K, Ollama
//!   nomic/mxbai/default, Google 1/2048, conservative Custom).

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use bango_lib::llm::embedding::embedding_limits;
use bango_lib::llm::orchestrator::{
    send_batch_parallel, send_embedding_batch_parallel, LlmOrchestrator,
};
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

// ── helpers ─────────────────────────────────────────────────────────────────

/// Build a minimal `LlmConfig` pointing at `endpoint` for `provider`.
fn config(provider: LlmProvider, endpoint: &str, model: &str) -> LlmConfig {
    LlmConfig {
        provider,
        endpoint_url: endpoint.to_string(),
        api_key_encrypted: Some("test-key".to_string()),
        model_name: model.to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

// ── send_batch_parallel: order preservation ─────────────────────────────────

#[tokio::test]
async fn batch_parallel_preserves_order_under_out_of_order_completion() {
    // Three batches: batch 1 sleeps the longest so it completes LAST, but its
    // result must still land in slot 1 (input order).
    let batches = vec![0_usize, 1, 2];

    let results = send_batch_parallel(batches, |idx| async move {
        let sleep_ms = match idx {
            0 => 10,
            1 => 50, // completes last
            2 => 20,
            _ => 0,
        };
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        Ok::<_, bango_lib::error::AppError>(idx)
    })
    .await;

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_ok());
    // Order preserved regardless of completion order.
    assert_eq!(results[0].as_ref().unwrap(), &0);
    assert_eq!(
        results[1].as_ref().unwrap(),
        &1,
        "slot 1 holds batch 1 even though it completed last"
    );
    assert_eq!(results[2].as_ref().unwrap(), &2);
}

// ── send_batch_parallel: mixed Ok/Err ───────────────────────────────────────

#[tokio::test]
async fn batch_parallel_reports_mixed_ok_err_per_batch() {
    let batches = vec!["ok-1", "fail", "ok-2"];

    let results = send_batch_parallel(batches, |batch| async move {
        if batch == "fail" {
            Err(bango_lib::error::AppError::Import("simulated failure".to_string()))
        } else {
            Ok(batch.to_string())
        }
    })
    .await;

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_err(), "middle batch failed");
    assert!(results[2].is_ok());
    assert_eq!(results[0].as_ref().unwrap(), "ok-1");
    assert_eq!(results[2].as_ref().unwrap(), "ok-2");
}

// ── send_batch_parallel: panic isolation ────────────────────────────────────

#[tokio::test]
async fn batch_parallel_panic_in_one_task_fills_slot_with_err() {
    let batches = vec![1_usize, 2, 3];

    let results = send_batch_parallel(batches, |idx| async move {
        if idx == 2 {
            panic!("simulated task panic");
        }
        Ok::<_, bango_lib::error::AppError>(idx)
    })
    .await;

    assert_eq!(results.len(), 3, "all slots filled even though one task panicked");
    // The panicking task's slot (idx 1, value 2) is an Err; the other two are Ok.
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count, 2, "two non-panicking tasks succeed");
    assert_eq!(err_count, 1, "one panicking task fills its slot with Err");
    // The non-panic slots hold their correct values.
    assert_eq!(results[0].as_ref().unwrap(), &1);
    assert_eq!(results[2].as_ref().unwrap(), &3);
}

// ── send_batch_parallel: empty input ────────────────────────────────────────

#[tokio::test]
async fn batch_parallel_empty_input_returns_empty_vec() {
    let results: Vec<Result<usize, bango_lib::error::AppError>> =
        send_batch_parallel(Vec::<usize>::new(), |idx| async move { Ok(idx) }).await;
    assert!(results.is_empty());
}

// ── send_embedding_batch_parallel: basic dispatch + dim reporting ───────────

#[tokio::test]
async fn embedding_batch_parallel_returns_one_vector_per_input_in_order() {
    let mut server = mockito::Server::new_async().await;
    // Mock returns 3 vectors of dim 4, in input order.
    let body = r#"{"data":[
        {"embedding":[0.1,0.2,0.3,0.4],"index":0},
        {"embedding":[0.5,0.6,0.7,0.8],"index":1},
        {"embedding":[0.9,1.0,1.1,1.2],"index":2}
    ]}"#;
    let _m = server
        .mock("POST", "/embeddings")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(3, 0));
    let cfg = config(LlmProvider::Openai, &server.url(), "text-embedding-3-small");
    let texts = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];

    let (vectors, dim) =
        send_embedding_batch_parallel(&orch, &cfg, &texts, "text-embedding-3-small")
            .await
            .expect("embedding batch succeeds");

    assert_eq!(vectors.len(), 3, "one vector per input");
    assert_eq!(dim, 4, "dimension from the first non-empty vector");
    assert_eq!(vectors[0], vec![0.1, 0.2, 0.3, 0.4]);
    assert_eq!(vectors[1], vec![0.5, 0.6, 0.7, 0.8]);
    assert_eq!(vectors[2], vec![0.9, 1.0, 1.1, 1.2]);
}

// ── send_embedding_batch_parallel: empty input rejected ─────────────────────

#[tokio::test]
async fn embedding_batch_parallel_rejects_empty_input() {
    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let cfg = config(LlmProvider::Openai, "http://unused", "text-embedding-3-small");
    let result = send_embedding_batch_parallel(&orch, &cfg, &[], "text-embedding-3-small").await;
    assert!(result.is_err(), "empty input must be rejected before any HTTP call");
}

// ── send_embedding_batch_parallel: per-text overflow pools into one vector ──
//
// This case exercises the split → group → pool pipeline end-to-end. We use a
// Custom provider (OpenAI-compatible limits: 2048/8191/300K) with a text that
// fits comfortably under the limit (so no splitting), confirming the common
// path returns one vector per input. The over-long split path is covered by
// the pure-helper unit tests in `embedding/text.rs` + `embedding/batching.rs`.

#[tokio::test]
async fn embedding_batch_parallel_custom_provider_one_vector_per_input() {
    let mut server = mockito::Server::new_async().await;
    let body = r#"{"data":[
        {"embedding":[1.0,2.0],"index":0},
        {"embedding":[3.0,4.0],"index":1}
    ]}"#;
    let _m =
        server.mock("POST", "/embeddings").with_status(200).with_body(body).create_async().await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    let cfg = config(LlmProvider::Custom, &server.url(), "custom-embed");
    let texts = vec!["short text one".to_string(), "short text two".to_string()];

    let (vectors, dim) =
        send_embedding_batch_parallel(&orch, &cfg, &texts, "custom-embed").await.expect("succeeds");

    assert_eq!(vectors.len(), 2);
    assert_eq!(dim, 2);
    assert_eq!(vectors[0], vec![1.0, 2.0]);
    assert_eq!(vectors[1], vec![3.0, 4.0]);
}

// ── embedding_limits per-provider table ─────────────────────────────────────

#[test]
fn limits_openai_8191_2048_300k() {
    let l = embedding_limits(&LlmProvider::Openai, "text-embedding-3-small");
    assert_eq!(l.max_inputs_per_batch, 2048);
    assert_eq!(l.max_tokens_per_input, 8191);
    assert_eq!(l.max_tokens_per_batch, 300_000);
}

#[test]
fn limits_openai_ada_002_same_cap() {
    // ada-002 shares the 8191 per-input cap.
    let l = embedding_limits(&LlmProvider::Openai, "text-embedding-ada-002");
    assert_eq!(l.max_tokens_per_input, 8191);
}

#[test]
fn limits_mistral_4096() {
    let l = embedding_limits(&LlmProvider::MistralAi, "mistral-embed");
    assert_eq!(l.max_inputs_per_batch, 2048);
    assert_eq!(l.max_tokens_per_input, 4096);
    assert_eq!(l.max_tokens_per_batch, 300_000);
}

#[test]
fn limits_google_one_input_per_call() {
    let l = embedding_limits(&LlmProvider::Google, "text-embedding-004");
    assert_eq!(l.max_inputs_per_batch, 1, "embedContent is one-text-per-call");
    assert_eq!(l.max_tokens_per_input, 2048);
    assert_eq!(l.max_tokens_per_batch, 2048);
}

#[test]
fn limits_ollama_nomic_8192() {
    let l = embedding_limits(&LlmProvider::Ollama, "nomic-embed-text");
    assert_eq!(l.max_inputs_per_batch, 1);
    assert_eq!(l.max_tokens_per_input, 8192);
    assert_eq!(l.max_tokens_per_batch, 8192);
}

#[test]
fn limits_ollama_mxbai_512() {
    let l = embedding_limits(&LlmProvider::Ollama, "mxbai-embed-large");
    assert_eq!(l.max_inputs_per_batch, 1);
    assert_eq!(l.max_tokens_per_input, 512);
    assert_eq!(l.max_tokens_per_batch, 512);
}

#[test]
fn limits_ollama_unknown_default_2048() {
    let l = embedding_limits(&LlmProvider::Ollama, "some-other-model");
    assert_eq!(l.max_inputs_per_batch, 1);
    assert_eq!(l.max_tokens_per_input, 2048, "conservative default for unknown local models");
    assert_eq!(l.max_tokens_per_batch, 2048);
}

#[test]
fn limits_custom_openai_compatible() {
    let l = embedding_limits(&LlmProvider::Custom, "anything");
    assert_eq!(l.max_inputs_per_batch, 2048);
    assert_eq!(l.max_tokens_per_input, 8191);
    assert_eq!(l.max_tokens_per_batch, 300_000);
}

#[test]
fn limits_anthropic_conservative() {
    // Unsupported, but returns conservative defaults so a misconfigured call
    // still splits conservatively.
    let l = embedding_limits(&LlmProvider::Anthropic, "claude-3");
    assert_eq!(l.max_inputs_per_batch, 32);
    assert_eq!(l.max_tokens_per_input, 512);
    assert_eq!(l.max_tokens_per_batch, 16_384);
}

#[test]
fn limits_lm_studio_and_llama_cpp_match_ollama_defaults() {
    // LM Studio / llama.cpp route through the same local-server branch as Ollama.
    let l1 = embedding_limits(&LlmProvider::LmStudio, "nomic-embed-text");
    let l2 = embedding_limits(&LlmProvider::LlamaCpp, "mxbai-embed-large");
    assert_eq!(l1.max_tokens_per_input, 8192, "LM Studio nomic matches Ollama nomic");
    assert_eq!(l2.max_tokens_per_input, 512, "llama.cpp mxbai matches Ollama mxbai");
}
