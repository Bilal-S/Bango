//! E2E integration tests for the screening engine with a mock LLM client.
//!
//! Scenarios:
//! 1. Happy path — bare array response, batch=2, 6 articles → 3 batches, 6 screened.
//! 2. Envelope format — `message.content` wrapper → same result.
//! 3. Partial error — one batch returns malformed JSON → those articles get error, rest succeed.
//! 4. Cancel mid-run — cancel after first batch → only first batch screened.
//! 5. Resume — after cancellation, re-run → remaining articles processed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::criterion::{Criterion, ResearchAim};
use bango_lib::screening::engine::ScreeningEngine;
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
            doi: None,
            journal: None,
            volume: None,
            issue: None,
            start_page: None,
            end_page: None,
            keywords: vec!["ml".to_string()],
            url: None,
            language: None,
            publisher: None,
            publisher_city: None,
            publisher_address: None,
            issn: None,
            reference_type: None,
            date: None,
            author_address: None,
            accession_number: None,
            custom_field3: None,
            journal_abbreviation: None,
            journal_iso_abbreviation: None,
            notes: None,
            web_of_science_db: None,
            ris_extras: None,
            import_source: Some("test".to_string()),
            data_length: None,
            token_estimate: None,
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
            let decision = if (start_index + i) % 2 == 0 {
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

/// Mock that returns good responses with a per-call delay — used for cancel tests.
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
    let conn = db.lock().unwrap();
    seed_articles(&conn, 6);
    let (criteria, aims) = seed_criteria(&conn);
    drop(conn);

    let responses =
        vec![make_batch_response(2, 0), make_batch_response(2, 2), make_batch_response(2, 4)];
    let mock = MockLlmClient::new(responses);
    let engine = ScreeningEngine::with_batch_size(2);

    engine.run_sync(&db, &mock, 0, criteria, aims, None).await.expect("run_sync");

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
    let conn = db.lock().unwrap();
    seed_articles(&conn, 4);
    let (criteria, aims) = seed_criteria(&conn);
    drop(conn);

    let responses = vec![make_envelope_response(2, 0), make_envelope_response(2, 2)];
    let mock = MockLlmClient::new(responses);
    let engine = ScreeningEngine::with_batch_size(2);

    engine.run_sync(&db, &mock, 0, criteria, aims, None).await.expect("run_sync");

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
    let conn = db.lock().unwrap();
    seed_articles(&conn, 6);
    let (criteria, aims) = seed_criteria(&conn);
    drop(conn);

    let mock = PartialErrorMock::new(vec![1]);
    let engine = ScreeningEngine::with_batch_size(2);

    engine.run_sync(&db, &mock, 0, criteria, aims, None).await.expect("run_sync");

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
    let conn = db.lock().unwrap();
    seed_articles(&conn, 6);
    let (criteria, aims) = seed_criteria(&conn);
    drop(conn);

    let engine = ScreeningEngine::with_batch_size(2);
    let engine_clone = Arc::new(engine);

    let cancel_engine = engine_clone.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_engine.cancel().await;
    });

    let mock = CancelAwareMock::new(6, 2, 100);
    engine_clone.run_sync(&db, &mock, 10, criteria, aims, None).await.expect("run_sync");

    let progress = engine_clone.get_progress().await;
    assert!(!progress.is_running);
    assert!(progress.completed >= 2, "At least first batch completed");
    assert!(progress.completed < 6, "Not all articles screened due to cancel");
}

#[tokio::test]
async fn test_resume_after_cancel() {
    let db = setup_db();
    let conn = db.lock().unwrap();
    seed_articles(&conn, 6);
    let (criteria, aims) = seed_criteria(&conn);
    drop(conn);

    // Run 1: cancel after first batch
    {
        let engine = ScreeningEngine::with_batch_size(2);
        let engine_arc = Arc::new(engine);
        let cancel_engine = engine_arc.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            cancel_engine.cancel().await;
        });

        let mock = CancelAwareMock::new(6, 2, 100);
        engine_arc
            .run_sync(&db, &mock, 10, criteria.clone(), aims.clone(), None)
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

        let responses: Vec<String> = (0..((remaining + 1) / 2))
            .map(|i| make_batch_response(2.min(remaining - i * 2), i * 2))
            .collect();
        let mock = MockLlmClient::new(responses);
        let engine = ScreeningEngine::with_batch_size(2);

        engine.run_sync(&db, &mock, 0, criteria, aims, None).await.expect("run 2");

        let progress = engine.get_progress().await;
        assert_eq!(progress.completed, remaining, "Remaining articles screened");
        assert!(!progress.is_running);
    }

    let conn = db.lock().unwrap();
    let unscreened = article_repo::count_unscreened_working(&conn).unwrap();
    assert_eq!(unscreened, 0, "No unscreened articles remain");
}
