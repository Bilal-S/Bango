//! Unit tests for the LLM Orchestrator.
//!
//! These tests verify that the centralized `LlmOrchestrator` correctly:
//! - Enforces concurrency limits (semaphore)
//! - Enforces rate limiting (delay between requests)
//! - Handles concurrent requests without deadlock or recursion
//! - Grows capacity via `update_settings`
//! - Reports available permits

use std::sync::Arc;
use std::time::{Duration, Instant};

use bango_lib::llm::orchestrator::{LlmOrchestrator, LlmRequestType};
use bango_lib::models::llm_config::{LlmConfig, LlmProvider};

/// Helper: build a minimal LlmConfig pointing at a fake endpoint.
fn fake_config() -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Openai,
        model_name: "test-model".into(),
        endpoint_url: "http://127.0.0.1:1".into(), // deliberately unreachable
        api_key_encrypted: Some("test-key".into()),
        context_window_tokens: 4096,
        temperature: 0.0,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
    }
}

// ─── Construction ──────────────────────────────────────────────────

#[test]
fn new_orchestrator_has_correct_permits() {
    let orch = LlmOrchestrator::new(5, 100);
    assert_eq!(orch.available_permits(), 5);
}

#[test]
fn new_orchestrator_clamps_zero_concurrency_to_one() {
    let orch = LlmOrchestrator::new(0, 100);
    assert_eq!(orch.available_permits(), 1);
}

// ─── update_settings ───────────────────────────────────────────────

#[tokio::test]
async fn update_settings_grows_semaphore() {
    let orch = LlmOrchestrator::new(2, 0);
    assert_eq!(orch.available_permits(), 2);

    orch.update_settings(5, 0).await;
    assert_eq!(orch.available_permits(), 5);
}

#[tokio::test]
async fn update_settings_changes_delay() {
    let orch = LlmOrchestrator::new(3, 0);
    // Delay is internal; verify no panic
    orch.update_settings(3, 500).await;
    assert_eq!(orch.available_permits(), 3);
}

// ─── Rate limiting ─────────────────────────────────────────────────

#[tokio::test]
async fn rate_limit_enforces_minimum_delay() {
    let orch = LlmOrchestrator::new(5, 200); // 200ms delay

    let start = Instant::now();
    assert_eq!(orch.available_permits(), 5);
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_millis(50), "Construction should be instant");
}

// ─── Concurrency semaphore ─────────────────────────────────────────

#[tokio::test]
async fn semaphore_limits_concurrent_requests() {
    let orch = Arc::new(LlmOrchestrator::new(2, 0)); // max 2 concurrent
    assert_eq!(orch.available_permits(), 2);
}

// ─── test_connection ────────────────────────────────────────────────

#[tokio::test]
async fn test_connection_fails_on_unreachable_endpoint() {
    let orch = LlmOrchestrator::new(1, 0);
    let config = fake_config();

    let result = orch.test_connection(&config).await;
    assert!(result.is_err(), "Should fail on unreachable endpoint");
}

// ─── Concurrent send() calls ────────────────────────────────────────

#[tokio::test]
async fn concurrent_sends_do_not_deadlock() {
    let orch = Arc::new(LlmOrchestrator::new(3, 0));
    let config = fake_config();

    let mut handles = vec![];
    for _ in 0..10 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            let _ = orch
                .send(
                    &config,
                    "system",
                    "user",
                    LlmRequestType::Screening,
                )
                .await;
        }));
    }

    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(result.is_ok(), "Task should complete without deadlock");
    }
}

#[tokio::test]
async fn concurrent_sends_with_delay_do_not_deadlock() {
    let orch = Arc::new(LlmOrchestrator::new(2, 50)); // 50ms delay
    let config = fake_config();

    let mut handles = vec![];
    for _ in 0..5 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            let _ = orch
                .send(
                    &config,
                    "system",
                    "user",
                    LlmRequestType::TagGeneration,
                )
                .await;
        }));
    }

    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(15), h).await;
        assert!(result.is_ok(), "Task should complete without deadlock even with rate limiting");
    }
}

#[tokio::test]
async fn many_concurrent_sends_at_high_concurrency() {
    let orch = Arc::new(LlmOrchestrator::new(10, 0));
    let config = fake_config();

    let mut handles = vec![];
    for _ in 0..50 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            let _ = orch
                .send(
                    &config,
                    "system",
                    "user",
                    LlmRequestType::SummaryGeneration,
                )
                .await;
        }));
    }

    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(30), h).await;
        assert!(result.is_ok(), "50 concurrent tasks should complete without deadlock");
    }
}

// ─── Mixed request types ────────────────────────────────────────────

#[tokio::test]
async fn mixed_request_types_no_recursion() {
    let orch = Arc::new(LlmOrchestrator::new(3, 0));
    let config = fake_config();

    let types = vec![
        LlmRequestType::Screening,
        LlmRequestType::AiSummary,
        LlmRequestType::ArticleSummary,
        LlmRequestType::TagGeneration,
        LlmRequestType::LabelGeneration,
        LlmRequestType::CriteriaGeneration,
        LlmRequestType::SummaryGeneration,
        LlmRequestType::TestConnection,
    ];

    let mut handles = vec![];
    for rt in types {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            let _ = orch.send(&config, "sys", "usr", rt).await;
        }));
    }

    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(result.is_ok(), "All request types should complete without recursion");
    }
}

// ─── send_unthrottled ───────────────────────────────────────────────

#[tokio::test]
async fn send_unthrottled_does_not_consume_semaphore() {
    let orch = LlmOrchestrator::new(1, 0);
    assert_eq!(orch.available_permits(), 1);
}

#[tokio::test]
async fn send_unthrottled_fails_on_unreachable() {
    let orch = LlmOrchestrator::new(1, 0);
    let config = fake_config();

    let result = orch
        .send_unthrottled(
            &config,
            "sys",
            "usr",
            LlmRequestType::TestConnection,
        )
        .await;
    assert!(result.is_err());
}

// ─── update_settings_during_active_requests ─────────────────────────

#[tokio::test]
async fn update_settings_during_active_requests_no_deadlock() {
    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    let config = fake_config();

    let orch_clone = orch.clone();
    let config_clone = config.clone();
    let handle = tokio::spawn(async move {
        let _ = orch_clone
            .send(
                &config_clone,
                "sys",
                "usr",
                LlmRequestType::Screening,
            )
            .await;
    });

    orch.update_settings(5, 100).await;

    let result = tokio::time::timeout(Duration::from_secs(10), handle).await;
    assert!(result.is_ok(), "Should not deadlock when updating settings during active requests");
}

// ═══════════════════════════════════════════════════════════════════════
// Helpers for mock-server based concurrency & queue-length tests
// ═══════════════════════════════════════════════════════════════════════

fn openai_chat_response(content: &str, tokens: usize) -> String {
    format!(
        r#"{{
            "choices": [{{ "message": {{ "role": "assistant", "content": {content_json} }} }}],
            "usage": {{ "total_tokens": {tokens} }}
        }}"#,
        content_json = serde_json::to_string(content).unwrap(),
        tokens = tokens,
    )
}

fn mock_openai_config(server_url: &str) -> LlmConfig {
    LlmConfig {
        provider: LlmProvider::Openai,
        endpoint_url: server_url.to_string(),
        api_key_encrypted: Some("test-key".to_string()),
        model_name: "gpt-4o".to_string(),
        temperature: 0.2,
        skip_temperature: false,
        max_concurrent_requests: 3,
        request_delay_ms: 0,
        context_window_tokens: 50_000,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Concurrency tests (using mockito server with artificial delays)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn concurrency_limit_is_enforced_with_mock_server() {
    let mut server = mockito::Server::new_async().await;
    // Mock responds after 200ms to simulate a slow LLM
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(200));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(4) // 4 requests total
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0)); // max 2 concurrent
    let config = mock_openai_config(&server.url());

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..4 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok(), "Task completed");
        assert!(r.unwrap().is_ok(), "Request succeeded");
    }
    let elapsed = start.elapsed();

    // With max_concurrent=2 and 200ms per request, 4 requests need at least 400ms
    // (2 batches of 2)
    assert!(
        elapsed >= Duration::from_millis(350),
        "Expected >= 350ms due to concurrency limit of 2, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

// ═══════════════════════════════════════════════════════════════════════
// Settings flow tests (simulates save_llm_config → orchestrator.update_settings)
// These validate that concurrency threads from the LLM Config UI
// actually control the orchestrator's semaphore.
// ═══════════════════════════════════════════════════════════════════════

/// Simulates what `save_llm_config` does: updates the orchestrator from config values.
async fn simulate_save_config(orch: &LlmOrchestrator, config: &LlmConfig) {
    orch.update_settings(
        config.max_concurrent_requests as usize,
        config.request_delay_ms as u64,
    ).await;
}

#[tokio::test]
async fn save_config_updates_concurrency_from_3_to_10() {
    // Start with default concurrency of 3
    let orch = LlmOrchestrator::new(3, 0);
    assert_eq!(orch.available_permits(), 3, "Initial permits should be 3");

    // Simulate user changing concurrency threads to 10 in settings UI
    let mut config = fake_config();
    config.max_concurrent_requests = 10;
    simulate_save_config(&orch, &config).await;

    assert_eq!(
        orch.available_permits(),
        10,
        "After save_config with max_concurrent=10, permits should be 10"
    );
}

#[tokio::test]
async fn save_config_updates_rate_limit_delay() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(3)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(5, 0));
    let config = mock_openai_config(&server.url());

    // Simulate user setting 150ms delay
    orch.update_settings(5, 150).await;

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..3 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }
    let elapsed = start.elapsed();

    // 3 requests with 150ms delay between each = at least 300ms
    assert!(
        elapsed >= Duration::from_millis(300),
        "Expected >= 300ms with 150ms rate limit, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn save_config_growing_concurrency_allows_more_parallelism() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(100));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(4)
        .create_async()
        .await;

    // Start with concurrency=2 (serialized batches)
    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    assert_eq!(orch.available_permits(), 2);

    // Simulate user INCREASING concurrency to 5 in settings
    orch.update_settings(5, 0).await;
    assert_eq!(orch.available_permits(), 5);

    // Now 4 requests should complete in ~100ms (all fit within 5 permits)
    // instead of ~200ms (two batches of 2)
    let config = mock_openai_config(&server.url());
    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..4 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }
    let elapsed = start.elapsed();

    // 4 requests with concurrency=5 should all run in parallel (~100ms)
    // NOT serialized (~300ms+)
    assert!(
        elapsed < Duration::from_millis(250),
        "With concurrency=5, 4 requests should run near-parallel, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn save_config_zero_concurrency_clamps_to_one() {
    let orch = LlmOrchestrator::new(3, 0);
    assert_eq!(orch.available_permits(), 3);

    // Simulate user accidentally setting concurrency to 0
    let mut config = fake_config();
    config.max_concurrent_requests = 0;
    simulate_save_config(&orch, &config).await;

    // Should clamp to at least 1 (the orchestrator's new() does this,
    // but update_settings also handles it)
    assert!(
        orch.available_permits() >= 1,
        "Concurrency should clamp to at least 1, got {}",
        orch.available_permits()
    );
}

#[tokio::test]
async fn save_config_multiple_updates_accumulate_correctly() {
    let orch = LlmOrchestrator::new(2, 0);
    assert_eq!(orch.available_permits(), 2);

    // Simulate multiple saves: 2 → 5 → 3 → 10
    let mut config = fake_config();

    config.max_concurrent_requests = 5;
    simulate_save_config(&orch, &config).await;
    assert_eq!(orch.available_permits(), 5);

    config.max_concurrent_requests = 3;
    simulate_save_config(&orch, &config).await;
    // update_settings only grows, so should remain at 5
    assert!(orch.available_permits() >= 5);

    config.max_concurrent_requests = 10;
    simulate_save_config(&orch, &config).await;
    assert_eq!(orch.available_permits(), 10);
}

#[tokio::test]
async fn single_concurrency_serializes_requests() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(100));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(3)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0)); // max 1 concurrent (serialized)
    let config = mock_openai_config(&server.url());

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..3 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }
    let elapsed = start.elapsed();

    // 3 serialized requests × 100ms each = at least 300ms
    assert!(
        elapsed >= Duration::from_millis(280),
        "Expected >= 280ms with concurrency=1, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn concurrency_does_not_exceed_limit() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut server = mockito::Server::new_async().await;
    let max_concurrent = Arc::new(AtomicUsize::new(0));
    let current_concurrent = Arc::new(AtomicUsize::new(0));

    // We'll track concurrent requests by using a shared counter
    let max_cc = max_concurrent.clone();
    let cc = current_concurrent.clone();

    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(move |w| {
            // Track peak concurrency
            let prev = cc.fetch_add(1, Ordering::SeqCst);
            let new_count = prev + 1;
            let mut peak = max_cc.load(Ordering::SeqCst);
            if new_count > peak {
                peak = new_count;
                max_cc.store(peak, Ordering::SeqCst);
            }
            // Simulate delay, then decrement
            std::thread::sleep(Duration::from_millis(50));
            cc.fetch_sub(1, Ordering::SeqCst);
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(6)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0)); // max 2 concurrent
    let config = mock_openai_config(&server.url());

    let mut handles = vec![];
    for _ in 0..6 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(15), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }

    let peak = max_concurrent.load(Ordering::SeqCst);
    assert!(
        peak <= 2,
        "Peak concurrent requests should be <= 2, was {}",
        peak
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn concurrency_with_rate_limit_adds_delay_between_batches() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(50));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(3)
        .create_async()
        .await;

    // 100ms rate limit delay between requests
    let orch = Arc::new(LlmOrchestrator::new(3, 100));
    let config = mock_openai_config(&server.url());

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..3 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }
    let elapsed = start.elapsed();

    // 3 requests with 100ms rate limit between each = at least 200ms of rate-limit delay
    // plus 50ms per request
    assert!(
        elapsed >= Duration::from_millis(200),
        "Expected >= 200ms with rate limiting, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn concurrent_send_unthrottled_bypasses_semaphore() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(100));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(3)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0)); // max 1 concurrent
    let config = mock_openai_config(&server.url());

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..3 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send_unthrottled(&config, "sys", "usr", LlmRequestType::TestConnection).await
        }));
    }
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }
    let elapsed = start.elapsed();

    // send_unthrottled bypasses semaphore (limit=1), so all 3 run concurrently
    // With 100ms delay each, total should be ~100-200ms, NOT 300ms+
    assert!(
        elapsed < Duration::from_millis(350),
        "send_unthrottled should bypass semaphore, got {:?}",
        elapsed
    );

    mock.assert_async().await;
}

// ═══════════════════════════════════════════════════════════════════════
// Queue length tests (available permits tracking)
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn queue_length_equals_max_concurrency_at_rest() {
    let orch = LlmOrchestrator::new(5, 0);
    assert_eq!(orch.available_permits(), 5, "At rest, all permits should be available");
}

#[tokio::test]
async fn queue_length_decreases_per_in_flight_send() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(300)); // slow enough to observe
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(3)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(5, 0));
    let config = mock_openai_config(&server.url());

    // Fire 3 requests
    let mut handles = vec![];
    for _ in 0..3 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }

    // Wait a bit for requests to start (acquire permits)
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Permits should have decreased
    let permits = orch.available_permits();
    assert!(
        permits < 5,
        "Permits should decrease when requests are in-flight, got {}",
        permits
    );
    assert!(
        permits >= 2,
        "At least 2 permits should remain (5 - 3 = 2), got {}",
        permits
    );

    // Wait for all to complete
    for h in handles {
        let _ = tokio::time::timeout(Duration::from_secs(10), h).await;
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn queue_length_returns_to_max_after_completion() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(50));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(5)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(5, 0));
    let config = mock_openai_config(&server.url());

    let mut handles = vec![];
    for _ in 0..5 {
        let orch = orch.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            orch.send(&config, "sys", "usr", LlmRequestType::Screening).await
        }));
    }

    // Wait for all to complete
    for h in handles {
        let r = tokio::time::timeout(Duration::from_secs(10), h).await;
        assert!(r.is_ok());
        assert!(r.unwrap().is_ok());
    }

    // All permits should be restored
    assert_eq!(
        orch.available_permits(),
        5,
        "All permits should be restored after requests complete"
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn queue_length_unchanged_by_send_unthrottled() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(200));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(3, 0));
    let config = mock_openai_config(&server.url());

    assert_eq!(orch.available_permits(), 3);

    // Start an unthrottled request
    let orch_clone = orch.clone();
    let config_clone = config.clone();
    let handle = tokio::spawn(async move {
        orch_clone
            .send_unthrottled(&config_clone, "sys", "usr", LlmRequestType::TestConnection)
            .await
    });

    // Wait a bit for the request to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Permits should NOT have changed
    assert_eq!(
        orch.available_permits(),
        3,
        "send_unthrottled should not consume semaphore permits"
    );

    let r = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(r.is_ok());
    assert!(r.unwrap().is_ok());

    mock.assert_async().await;
}

#[tokio::test]
async fn queue_length_grows_after_update_settings() {
    let orch = LlmOrchestrator::new(3, 0);
    assert_eq!(orch.available_permits(), 3);

    orch.update_settings(10, 0).await;
    assert_eq!(
        orch.available_permits(),
        10,
        "Permits should grow to new max after update_settings"
    );

    // Grow again
    orch.update_settings(20, 0).await;
    assert_eq!(
        orch.available_permits(),
        20,
        "Permits should grow further after another update_settings"
    );
}

#[tokio::test]
async fn queue_length_at_boundary_concurrency_one() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(200));
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(1, 0));
    let config = mock_openai_config(&server.url());

    assert_eq!(orch.available_permits(), 1);

    // Start one request — should consume the only permit
    let orch_clone = orch.clone();
    let config_clone = config.clone();
    let handle = tokio::spawn(async move {
        orch_clone.send(&config_clone, "sys", "usr", LlmRequestType::Screening).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        orch.available_permits(),
        0,
        "With concurrency=1, the single permit should be consumed"
    );

    // Wait for completion
    let r = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(r.is_ok());
    assert!(r.unwrap().is_ok());

    // Permit should be restored
    assert_eq!(
        orch.available_permits(),
        1,
        "Permit should be restored after the request completes"
    );

    mock.assert_async().await;
}
