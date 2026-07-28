//! E2E integration tests for the screening engine with a mock LLM client.
//!
//! Scenarios:
//! 1. Happy path - bare array response, batch=2, 6 articles → 3 batches, 6 screened.
//! 2. Envelope format - `message.content` wrapper → same result.
//! 3. Partial error - one batch returns malformed JSON → those articles get error, rest succeed.
//! 4. Cancel mid-run - cancel after first batch → only first batch screened.
//! 5. Resume - after cancellation, re-run → remaining articles processed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::criterion::{Criterion, ResearchAim};
use bango_lib::screening::engine::{RunSyncContext, ScreeningConfig, ScreeningEngine};
use bango_lib::screening::llm_client::LlmClient;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn setup_db() -> std::sync::Mutex<rusqlite::Connection> {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    std::sync::Mutex::new(conn)
}

fn seed_articles(conn: &rusqlite::Connection, count: usize) -> Vec<String> {
    let articles: Vec<NewArticle> = (0..count)
        .map(|i| NewArticle {
            title: format!("Article {i}"),
            abstract_text: format!(
                "Abstract for article {i} about machine learning in healthcare."
            ),
            authors: vec![format!("Author {i}")],
            publication_year: Some(2024),
            keywords: vec!["ml".to_string()],
            import_source: Some("test".to_string()),
            ..Default::default()
        })
        .collect();

    let inserted = article_repo::insert_articles_batch(conn, &articles, "test").expect("insert");
    let ids: Vec<String> = inserted.iter().map(|a| a.id.clone()).collect();

    // Move to 'working' status so they're eligible for screening
    article_repo::move_articles_to_working_batch(conn, &ids).expect("move to working");
    ids
}

fn seed_criteria(conn: &rusqlite::Connection) -> (Vec<Criterion>, Vec<ResearchAim>) {
    let aim = criteria_repo::create_aim(conn, "Study AI in healthcare").expect("aim");
    let inc = criteria_repo::create_criterion(conn, "inclusion", "Must be about ML", "standard")
        .expect("inc criterion");
    let exc =
        criteria_repo::create_criterion(conn, "exclusion", "Not about healthcare", "standard")
            .expect("exc criterion");
    (vec![inc, exc], vec![aim])
}

/// Build a bare-array JSON response for `n` articles, alternating include/exclude.
fn make_batch_response(n: usize, start_index: usize) -> String {
    let items: Vec<String> = (0..n)
        .map(|i| {
            let decision = if (start_index + i).is_multiple_of(2) {
                "include"
            } else {
                "exclude"
            };
            format!(
                r#"{{"decision":"{decision}","reasoning":"Reason for article {}","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}}"#,
                start_index + i
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Build an envelope-wrapped response (simulates z.ai format).
fn make_envelope_response(n: usize, start_index: usize) -> String {
    let array = make_batch_response(n, start_index);
    format!(r#"{{"message":{{"content":{array}}}}}"#,)
}

// ─── Mock LLM Client ────────────────────────────────────────────────────────

/// A mock LLM client that returns predetermined responses.
struct MockLlmClient {
    responses: Vec<String>,
    call_count: AtomicUsize,
}

impl MockLlmClient {
    fn new(responses: Vec<String>) -> Self {
        Self { responses, call_count: AtomicUsize::new(0) }
    }
}

#[async_trait::async_trait]
impl LlmClient for MockLlmClient {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        let resp = self.responses.get(idx).cloned().unwrap_or_default();
        Ok((resp, 100))
    }
}

/// Mock that returns malformed JSON for certain call indices.
struct PartialErrorMock {
    error_indices: Vec<usize>,
    call_count: AtomicUsize,
}

impl PartialErrorMock {
    fn new(error_indices: Vec<usize>) -> Self {
        Self { error_indices, call_count: AtomicUsize::new(0) }
    }
}

#[async_trait::async_trait]
impl LlmClient for PartialErrorMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.error_indices.contains(&idx) {
            Ok(("this is not valid json".to_string(), 50))
        } else {
            Ok((make_batch_response(2, idx * 2), 100))
        }
    }
}

/// Mock that returns good responses with a per-call delay - used for cancel tests.
struct CancelAwareMock {
    total_articles: usize,
    batch_size: usize,
    call_count: AtomicUsize,
    delay_ms: u64,
}

impl CancelAwareMock {
    fn new(total_articles: usize, batch_size: usize, delay_ms: u64) -> Self {
        Self { total_articles, batch_size, call_count: AtomicUsize::new(0), delay_ms }
    }
}

#[async_trait::async_trait]
impl LlmClient for CancelAwareMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        let start = idx * self.batch_size;
        let count = self.batch_size.min(self.total_articles - start);
        // Simulate LLM latency so cancel has time to fire
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        Ok((make_batch_response(count, start), 100))
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_happy_path_bare_array_batch2() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 6);
        seed_criteria(&conn)
    };

    let responses =
        vec![make_batch_response(2, 0), make_batch_response(2, 2), make_batch_response(2, 4)];
    let mock = MockLlmClient::new(responses);
    let engine = ScreeningEngine::with_batch_size(2);

    engine
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext { request_delay_ms: 0, ..Default::default() },
        )
        .await
        .expect("run_sync");

    let progress = engine.get_progress().await;
    assert_eq!(progress.total, 6);
    assert_eq!(progress.completed, 6);
    assert_eq!(progress.errors, 0);
    assert!(!progress.is_running);

    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 0, "All articles should be screened");
}

#[tokio::test]
async fn test_envelope_format() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 4);
        seed_criteria(&conn)
    };

    let responses = vec![make_envelope_response(2, 0), make_envelope_response(2, 2)];
    let mock = MockLlmClient::new(responses);
    let engine = ScreeningEngine::with_batch_size(2);

    engine
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext { request_delay_ms: 0, ..Default::default() },
        )
        .await
        .expect("run_sync");

    let progress = engine.get_progress().await;
    assert_eq!(progress.completed, 4);
    assert_eq!(progress.errors, 0);

    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 0);
}

#[tokio::test]
async fn test_partial_error_one_batch_malformed() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 6);
        seed_criteria(&conn)
    };

    let mock = PartialErrorMock::new(vec![1]);
    let engine = ScreeningEngine::with_batch_size(2);

    engine
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext { request_delay_ms: 0, ..Default::default() },
        )
        .await
        .expect("run_sync");

    let progress = engine.get_progress().await;
    assert_eq!(progress.completed, 6, "All articles should be completed (success + error)");
    assert_eq!(progress.errors, 2, "Batch 1 (2 articles) should be errors");

    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 0, "Error articles should still have screened_at set");
}

#[tokio::test]
async fn test_cancel_mid_run() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 6);
        seed_criteria(&conn)
    };

    let engine = ScreeningEngine::with_batch_size(2);
    let engine_clone = Arc::new(engine);

    let cancel_engine = engine_clone.clone();
    tokio::spawn(async move {
        // The cancel must fire AFTER the first batch's LLM call completes
        // (100ms latency + 10ms inter-batch delay = ~110ms) but BEFORE the
        // second batch completes (~210ms). 150ms lands safely inside the
        // second batch's LLM call so the first batch is recorded as completed
        // and the cancel drops the in-flight second-batch response (v8.3
        // cancel-during-LLM-call contract). A 50ms delay would race the
        // first batch's 100ms LLM call and flake.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        cancel_engine.cancel().await;
    });

    let mock = CancelAwareMock::new(6, 2, 100);
    engine_clone
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext { request_delay_ms: 10, ..Default::default() },
        )
        .await
        .expect("run_sync");

    let progress = engine_clone.get_progress().await;
    assert!(!progress.is_running);
    assert!(progress.completed >= 2, "At least first batch completed");
    assert!(progress.completed < 6, "Not all articles screened due to cancel");
}

#[tokio::test]
async fn test_resume_after_cancel() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 6);
        seed_criteria(&conn)
    };

    // Run 1: cancel after first batch
    {
        let engine = ScreeningEngine::with_batch_size(2);
        let engine_arc = Arc::new(engine);
        let cancel_engine = engine_arc.clone();
        tokio::spawn(async move {
            // See `test_cancel_mid_run`: 150ms lands inside the second
            // batch's LLM call so the first batch completes (>= 2) and the
            // cancel drops the in-flight second-batch response (< 6).
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            cancel_engine.cancel().await;
        });

        let mock = CancelAwareMock::new(6, 2, 100);
        engine_arc
            .run_sync(
                &db,
                &mock,
                criteria.clone(),
                aims.clone(),
                ScreeningConfig::default(),
                &RunSyncContext {
                    request_delay_ms: 10,
                    // batch-screening mode: no targeted article ID
                    ..Default::default()
                },
            )
            .await
            .expect("run 1");

        let p = engine_arc.get_progress().await;
        assert!(p.completed >= 2 && p.completed < 6);
    }

    // Verify some articles remain unscreened
    {
        let conn = db.lock().unwrap();
        let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
        assert!(unscreened > 0, "Some articles should remain unscreened");
    }

    // Run 2: new engine screens remaining articles
    {
        let remaining = {
            let conn = db.lock().unwrap();
            article_repo::count_unscreened_working(&conn).unwrap()
        };

        let responses: Vec<String> = (0..remaining.div_ceil(2))
            .map(|i| make_batch_response(2.min(remaining - i * 2), i * 2))
            .collect();
        let mock = MockLlmClient::new(responses);
        let engine = ScreeningEngine::with_batch_size(2);

        engine
            .run_sync(
                &db,
                &mock,
                criteria,
                aims,
                ScreeningConfig::default(),
                &RunSyncContext { request_delay_ms: 0, ..Default::default() },
            )
            .await
            .expect("run 2");

        let progress = engine.get_progress().await;
        assert_eq!(progress.completed, remaining, "Remaining articles screened");
        assert!(!progress.is_running);
    }

    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 0, "No unscreened articles remain");
}

// ─── target_article_id tests (per-article "Screen" button path) ─────────────
//
// These cover the `RunSyncContext.target_article_id` branch of `run_sync`
// (engine.rs): when `Some(id)` is set, the engine fetches that specific
// article by UUID via `get_unscreened_working_article_by_id` instead of the
// next-by-`sequence_id` batch. The article must be in `working` status and
// unscreened; otherwise the lookup returns `None` and the engine exits
// immediately with `Ok(())` (no-op, not an error).

#[tokio::test]
async fn test_target_article_id_screens_only_that_article() {
    let db = setup_db();
    let (criteria, aims, ids) = {
        let conn = db.lock().unwrap();
        let ids = seed_articles(&conn, 4);
        let (criteria, aims) = seed_criteria(&conn);
        (criteria, aims, ids)
    };

    // Target the second article specifically (per-article "Screen" button).
    let target_id = ids[1].clone();
    let responses = vec![make_batch_response(1, 0)];
    let mock = MockLlmClient::new(responses);
    let engine = ScreeningEngine::with_batch_size(2);

    engine
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext {
                request_delay_ms: 0,
                target_article_id: Some(target_id),
                ..Default::default()
            },
        )
        .await
        .expect("run_sync");

    let progress = engine.get_progress().await;
    assert_eq!(progress.completed, 1, "Only the targeted article should be screened");
    assert_eq!(progress.errors, 0);
    assert!(!progress.is_running);

    // The other 3 articles should remain unscreened.
    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 3, "Other articles should remain unscreened");
}

#[tokio::test]
async fn test_target_article_id_nonexistent_is_noop() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        seed_articles(&conn, 4);
        seed_criteria(&conn)
    };

    // A random UUID that does not match any article.
    let mock = MockLlmClient::new(Vec::new());
    let engine = ScreeningEngine::with_batch_size(2);

    let result = engine
        .run_sync(
            &db,
            &mock,
            criteria,
            aims,
            ScreeningConfig::default(),
            &RunSyncContext {
                request_delay_ms: 0,
                target_article_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_ok(), "Nonexistent target should be a clean no-op, not an error");

    let progress = engine.get_progress().await;
    assert_eq!(progress.completed, 0, "No articles should be screened");
    assert!(!progress.is_running);

    // No LLM call should have been made.
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 0, "No LLM call for nonexistent target");

    // All 4 articles remain unscreened.
    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 4);
}

#[tokio::test]
async fn test_target_article_id_already_screened_is_noop() {
    let db = setup_db();
    let (criteria, aims, ids) = {
        let conn = db.lock().unwrap();
        let ids = seed_articles(&conn, 4);
        let (criteria, aims) = seed_criteria(&conn);
        (criteria, aims, ids)
    };

    let target_id = ids[0].clone();

    // Run 1: screen the target article normally via targeted screening.
    {
        let mock = MockLlmClient::new(vec![make_batch_response(1, 0)]);
        let engine = ScreeningEngine::with_batch_size(2);
        engine
            .run_sync(
                &db,
                &mock,
                criteria.clone(),
                aims.clone(),
                ScreeningConfig::default(),
                &RunSyncContext {
                    request_delay_ms: 0,
                    target_article_id: Some(target_id.clone()),
                    ..Default::default()
                },
            )
            .await
            .expect("run 1");

        let progress = engine.get_progress().await;
        assert_eq!(progress.completed, 1, "First run screens the target");
    }

    // Run 2: target the same article again — it's already screened, so the
    // lookup returns `None` and the engine exits immediately.
    {
        let mock = MockLlmClient::new(Vec::new());
        let engine = ScreeningEngine::with_batch_size(2);
        let result = engine
            .run_sync(
                &db,
                &mock,
                criteria,
                aims,
                ScreeningConfig::default(),
                &RunSyncContext {
                    request_delay_ms: 0,
                    target_article_id: Some(target_id),
                    ..Default::default()
                },
            )
            .await;

        assert!(result.is_ok(), "Already-screened target should be a clean no-op");

        let progress = engine.get_progress().await;
        assert_eq!(progress.completed, 0, "Already-screened article should not be re-screened");
        assert!(!progress.is_running);
        assert_eq!(
            mock.call_count.load(Ordering::SeqCst),
            0,
            "No LLM call for already-screened target"
        );
    }

    // The other 3 articles should still be unscreened.
    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 3, "Other articles should remain unscreened");
}
