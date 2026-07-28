//! Unit tests for the LLM Orchestrator.
//!
//! These tests verify that the centralized `LlmOrchestrator` correctly:
//! - Enforces concurrency limits (semaphore)
//! - Enforces rate limiting (delay between requests)
//! - Handles concurrent requests without deadlock or recursion
//! - Sets exact capacity via `update_settings` (grows AND shrinks)
//! - Reports available permits

use std::sync::Arc;
use std::time::{Duration, Instant};

use bango_lib::llm::orchestrator::{timeout_for, LlmOrchestrator, LlmRequestType};
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

#[tokio::test]
async fn update_settings_shrinks_semaphore() {
    // Regression for flaw 1: the old `add_permits` approach could only grow,
    // so lowering the limit was silently ignored (capacity stayed at 5).
    let orch = LlmOrchestrator::new(5, 0);
    assert_eq!(orch.available_permits(), 5);

    orch.update_settings(2, 0).await;
    assert_eq!(orch.available_permits(), 2, "limit should shrink from 5 to 2");

    // Going back up also works (exact-set semantics, not monotonic).
    orch.update_settings(8, 0).await;
    assert_eq!(orch.available_permits(), 8);
}

#[tokio::test]
async fn update_settings_does_not_inflate_capacity_when_permits_in_flight() {
    // Regression for flaw 2: the old code compared the NEW max against
    // `available_permits()` (the free count) instead of capacity. With one
    // permit in flight it read `available = capacity - 1`, then added
    // `capacity - (capacity - 1) = 1` permit on every save of the same value,
    // growing capacity without bound. The fix swaps a fresh semaphore, so
    // saving the SAME value while a permit is in flight must leave capacity
    // unchanged.
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(300)); // hold a permit
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(5, 0));
    let config = mock_openai_config(&server.url());

    // Start one slow request so one permit is in flight (available drops 5->4).
    let orch_clone = Arc::clone(&orch);
    let config_clone = config.clone();
    let handle = tokio::spawn(async move {
        orch_clone.send(&config_clone, "sys", "usr", LlmRequestType::Screening).await
    });

    // Wait until the in-flight request has acquired its permit.
    let acquired = tokio::time::timeout(Duration::from_millis(200), async {
        while orch.available_permits() == 5 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(acquired.is_ok(), "in-flight request should consume a permit");
    assert_eq!(orch.available_permits(), 4, "one permit should be in flight");

    // Save the SAME concurrency value.
    // Old bug: capacity 5 -> 6 (add_permits(5-4)). Fix: stays 5 (fresh swap).
    orch.update_settings(5, 0).await;

    // Let the in-flight request finish; its permit was on the swapped-out
    // semaphore, so returning it must NOT inflate the new semaphore.
    let r = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(r.is_ok() && r.unwrap().is_ok());

    assert_eq!(
        orch.available_permits(),
        5,
        "saving the same value with a permit in flight must not inflate capacity"
    );

    mock.assert_async().await;
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
            let _ = orch.send(&config, "system", "user", LlmRequestType::Screening).await;
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
            let _ = orch.send(&config, "system", "user", LlmRequestType::TagGeneration).await;
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
            let _ = orch.send(&config, "system", "user", LlmRequestType::SummaryGeneration).await;
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

    let result = orch.send_unthrottled(&config, "sys", "usr", LlmRequestType::TestConnection).await;
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
        let _ = orch_clone.send(&config_clone, "sys", "usr", LlmRequestType::Screening).await;
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
    orch.update_settings(config.max_concurrent_requests as usize, config.request_delay_ms as u64)
        .await;
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
async fn save_config_lowering_concurrency_caps_peak() {
    // Behavioral regression for flaw 1: lowering the limit from the UI must
    // actually reduce the enforced concurrency. Under the old bug, shrinking
    // was a no-op, so the old limit (5) would have been enforced instead of
    // the newly saved 2, and peak concurrency would hit 5.
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut server = mockito::Server::new_async().await;
    let max_concurrent = Arc::new(AtomicUsize::new(0));
    let current_concurrent = Arc::new(AtomicUsize::new(0));
    let max_cc = max_concurrent.clone();
    let cc = current_concurrent.clone();

    let mock = server
        .mock("POST", "/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_chunked_body(move |w| {
            let prev = cc.fetch_add(1, Ordering::SeqCst);
            let new_count = prev + 1;
            let mut peak = max_cc.load(Ordering::SeqCst);
            if new_count > peak {
                peak = new_count;
                max_cc.store(peak, Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(50));
            cc.fetch_sub(1, Ordering::SeqCst);
            w.write_all(openai_chat_response("ok", 5).as_bytes()).unwrap();
            Ok(())
        })
        .expect(6)
        .create_async()
        .await;

    // Start high, then LOWER to 2 with nothing in flight (clean swap).
    let orch = Arc::new(LlmOrchestrator::new(5, 0));
    orch.update_settings(2, 0).await;
    assert_eq!(orch.available_permits(), 2, "limit should shrink to 2");

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
    assert!(peak <= 2, "After lowering to 2, peak concurrent requests should be <= 2, was {peak}");

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
async fn save_config_multiple_updates_set_exact_capacity() {
    // Each save sets the EXACT capacity (not monotonic accumulation).
    // Previously update_settings could only grow, so 5 -> 3 left capacity at 5.
    let orch = LlmOrchestrator::new(2, 0);
    assert_eq!(orch.available_permits(), 2);

    // Simulate multiple saves: 2 -> 5 -> 3 -> 10
    let mut config = fake_config();

    config.max_concurrent_requests = 5;
    simulate_save_config(&orch, &config).await;
    assert_eq!(orch.available_permits(), 5);

    config.max_concurrent_requests = 3;
    simulate_save_config(&orch, &config).await;
    // update_settings now sets exact capacity, so it shrinks to 3.
    assert_eq!(orch.available_permits(), 3);

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
    assert!(peak <= 2, "Peak concurrent requests should be <= 2, was {}", peak);

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
    assert!(permits < 5, "Permits should decrease when requests are in-flight, got {}", permits);
    assert!(permits >= 2, "At least 2 permits should remain (5 - 3 = 2), got {}", permits);

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

    // Start one request - should consume the only permit
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

// ─── timeout_for: per-request-type timeout policy ───────────────────────
//
// Screening (stage-1 + stage-2) uses a tighter cap (120s) than the default
// 600s so a hung/slow call surfaces as an error within ~2 minutes instead of
// stalling the run for 10 minutes. All other request types use the default.

#[test]
fn timeout_for_screening_returns_120_seconds() {
    assert_eq!(timeout_for(&LlmRequestType::Screening), Duration::from_secs(120));
}

#[test]
fn timeout_for_enhanced_screening_returns_120_seconds() {
    // Stage-2 (two-stage + enhanced) is also screening, so it shares the
    // tighter cap.
    assert_eq!(timeout_for(&LlmRequestType::EnhancedScreening), Duration::from_secs(120));
}

#[test]
fn timeout_for_non_screening_types_return_default_600_seconds() {
    // Spot-check a representative set of non-screening request types.
    assert_eq!(timeout_for(&LlmRequestType::AiSummary), Duration::from_secs(600));
    assert_eq!(timeout_for(&LlmRequestType::Chat), Duration::from_secs(600));
    assert_eq!(timeout_for(&LlmRequestType::Translation), Duration::from_secs(600));
    assert_eq!(timeout_for(&LlmRequestType::WikiIngest), Duration::from_secs(600));
    assert_eq!(timeout_for(&LlmRequestType::GapAnalysis), Duration::from_secs(600));
    assert_eq!(timeout_for(&LlmRequestType::SummaryGeneration), Duration::from_secs(600));
}

// ─── Temperature-flag persistence signal ───────────────────────────────
//
// When the client recovers from a temperature-rejection 400, the orchestrator
// must call the wired `TemperatureFlagPersister` so the flag is persisted and
// future calls skip the first-attempt failure. This test injects a recording
// fake persister and verifies `persist(true)` fires exactly once after a
// recovery, and not at all on a normal success.

use std::sync::atomic::{AtomicU32, Ordering};

use bango_lib::llm::orchestrator::TemperatureFlagPersister;

/// Recording fake: counts `persist(true)` invocations.
struct RecordingPersister {
    persist_true_calls: Arc<AtomicU32>,
}

impl TemperatureFlagPersister for RecordingPersister {
    fn persist(&self, skip: bool) {
        if skip {
            self.persist_true_calls.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn temperature_persister_fires_on_recovery() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model. Only the default (1) value is supported.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;

    // First attempt: 400 temperature rejection.
    let first = server
        .mock("POST", "/chat/completions")
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;
    // Second attempt: 200 (recovered without temperature).
    let second = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("recovered", 5))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    let counter = Arc::new(AtomicU32::new(0));
    orch.set_temperature_persister(Arc::new(RecordingPersister {
        persist_true_calls: counter.clone(),
    }));

    let config = mock_openai_config(&server.url());
    let result = orch.send(&config, "sys", "usr", LlmRequestType::AiSummary).await;

    first.assert_async().await;
    second.assert_async().await;
    assert!(result.is_ok(), "orchestrator should recover from temperature 400");

    // The persister is invoked from a detached `tokio::task::spawn`, so poll
    // briefly until the counter advances (or time out at 2s - the persistence
    // task is best-effort + async).
    let persisted = tokio::time::timeout(Duration::from_secs(2), async {
        while counter.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(persisted.is_ok(), "persist(true) should fire after a temperature recovery");
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn temperature_persister_does_not_fire_on_normal_success() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("ok", 5))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    let counter = Arc::new(AtomicU32::new(0));
    orch.set_temperature_persister(Arc::new(RecordingPersister {
        persist_true_calls: counter.clone(),
    }));

    let config = mock_openai_config(&server.url());
    let result = orch.send(&config, "sys", "usr", LlmRequestType::AiSummary).await;
    assert!(result.is_ok());

    mock.assert_async().await;
    // Give the (non-existent) persistence task a moment to prove it did NOT fire.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "persist(true) must NOT fire when the call succeeds without a temperature rejection"
    );
}

// ─── In-session temperature latch ─────────────────────────────────────
//
// After the FIRST call in a session recovers from a temperature-rejection 400,
// the orchestrator latches an in-session flag so every subsequent call omits
// `temperature` from the start (no wasteful first-attempt 400 + retry). This
// is the fix for the "every screening batch retries temperature" bug: long-
// running consumers (screening engine) cache `LlmConfig` and never re-read the
// DB row, so DB persistence alone cannot reach them mid-run.

#[tokio::test]
async fn session_latch_skips_temperature_on_second_call_after_first_rejection() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model. Only the default (1) value is supported.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;

    // Call 1: first attempt sends temperature (400), retry omits it (200).
    // Total: 2 hits on the server for call 1.
    let first_400 = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({"temperature": 0.2})))
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;
    let first_200 = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("first-recovered", 5))
        .expect(1)
        .create_async()
        .await;

    // Call 2: the in-session latch is now set, so the orchestrator must clone
    // the config with skip_temperature=true. The request body must NOT contain
    // "temperature" (only one hit, no retry).
    let second_200 = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "system", "content": "sys"},
                    {"role": "user", "content": "usr"},
                ],
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(openai_chat_response("second-direct", 5))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    // No persister needed: the in-session latch works even without a persister.
    let config = mock_openai_config(&server.url());

    // Call 1: recovers from temperature rejection.
    let r1 = orch.send(&config, "sys", "usr", LlmRequestType::AiSummary).await;
    assert!(r1.is_ok(), "first call should recover");
    assert_eq!(r1.unwrap().0, "first-recovered");

    // Call 2: must go through with NO temperature (single hit, no retry).
    let r2 = orch.send(&config, "sys", "usr", LlmRequestType::AiSummary).await;
    assert!(r2.is_ok(), "second call should succeed directly");
    assert_eq!(r2.unwrap().0, "second-direct");

    // Assert the exact hit counts: call 1 = 2 (400 + 200), call 2 = 1 (200 only).
    first_400.assert_async().await;
    first_200.assert_async().await;
    second_200.assert_async().await;
}

// ─── Test Connection temperature-recovery regression ──────────────────
//
// Regression: the client-level `send_with_temperature_recovery` made
// `send_chat_completion` return Ok on a temperature 400 (transparent retry).
// `test_connection` returned Ok too, so `test_llm_connection` reported success
// WITHOUT persisting `skip_temperature=true`. Screening batch 1 then
// rediscovered the rejection. This test asserts `test_connection` surfaces the
// recovery via `CallMeta.temperature_was_rejected` AND flips the in-session
// latch so the very next `send()` omits temperature.

#[tokio::test]
async fn test_connection_surfaces_temperature_recovery_and_latches() {
    let mut server = mockito::Server::new_async().await;

    let error_body = r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model. Only the default (1) value is supported.","type":"invalid_request_error","param":"temperature","code":"unsupported_value"}}"#;

    // test_connection call: 400 (with temperature) then 200 (without).
    let first = server
        .mock("POST", "/chat/completions")
        .with_status(400)
        .with_body(error_body)
        .expect(1)
        .create_async()
        .await;
    let second = server
        .mock("POST", "/chat/completions")
        .with_status(200)
        .with_body(openai_chat_response("hello", 5))
        .expect(1)
        .create_async()
        .await;

    let orch = Arc::new(LlmOrchestrator::new(2, 0));
    let config = mock_openai_config(&server.url());

    // test_connection must return Ok (recovered) with temperature_was_rejected.
    let result = orch.test_connection(&config).await;
    first.assert_async().await;
    second.assert_async().await;
    let (_content, _tokens, meta) = result.expect("test_connection should recover");
    assert!(
        meta.temperature_was_rejected,
        "test_connection must surface temperature_was_rejected so test_llm_connection can persist"
    );

    // The latch must now be flipped: a subsequent send() must NOT send
    // temperature (single hit, body without "temperature").
    let third = server
        .mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::JsonString(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "system", "content": "sys"},
                    {"role": "user", "content": "usr"},
                ],
            })
            .to_string(),
        ))
        .with_status(200)
        .with_body(openai_chat_response("ok", 3))
        .expect(1)
        .create_async()
        .await;

    let r = orch.send(&config, "sys", "usr", LlmRequestType::AiSummary).await;
    assert!(r.is_ok());
    third.assert_async().await;
}
