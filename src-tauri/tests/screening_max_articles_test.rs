use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::criterion::{Criterion, ResearchAim};
use bango_lib::screening::engine::{ScreeningConfig, ScreeningEngine};
use bango_lib::screening::llm_client::LlmClient;

fn setup_db() -> std::sync::Mutex<rusqlite::Connection> {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    std::sync::Mutex::new(conn)
}

fn seed_working_articles(conn: &rusqlite::Connection, count: usize) {
    let records: Vec<NewArticle> = (0..count)
        .map(|i| NewArticle {
            title: format!("Article {}", i + 1),
            abstract_text: format!("Abstract {}", i + 1),
            authors: vec!["Author".to_string()],
            publication_year: Some(2024),
            import_source: Some("test".to_string()),
            ..Default::default()
        })
        .collect();

    let inserted = article_repo::insert_articles_batch(conn, &records, "test").expect("insert");
    let ids = inserted.into_iter().map(|a| a.id).collect::<Vec<_>>();
    article_repo::move_articles_to_working_batch(conn, &ids).expect("move to working");
}

fn seed_criteria(conn: &rusqlite::Connection) -> (Vec<Criterion>, Vec<ResearchAim>) {
    let aim = criteria_repo::create_aim(conn, "Study outcomes").expect("aim");
    let inc =
        criteria_repo::create_criterion(conn, "inclusion", "Relevant to outcomes", "standard")
            .expect("inc");
    let exc = criteria_repo::create_criterion(conn, "exclusion", "Not relevant", "standard")
        .expect("exc");
    (vec![inc, exc], vec![aim])
}

use std::sync::atomic::{AtomicUsize, Ordering};

struct AlwaysIncludeMock {
    calls: AtomicUsize,
}

impl AlwaysIncludeMock {
    fn new() -> Self {
        Self { calls: AtomicUsize::new(0) }
    }
}

#[async_trait::async_trait]
impl LlmClient for AlwaysIncludeMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((
            r#"[{"decision":"include","reasoning":"R","matchedInclusionCriteria":[],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":0.9}]"#.to_string(),
            100,
        ))
    }
}

#[tokio::test]
async fn max_articles_cap_processes_only_requested_count() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().expect("db lock");
        seed_working_articles(&conn, 5);
        seed_criteria(&conn)
    };

    let config = ScreeningConfig { max_articles: Some(2), ..ScreeningConfig::default() };

    let engine = ScreeningEngine::with_batch_size(1);
    let llm = AlwaysIncludeMock::new();

    engine.run_sync(&db, &llm, 0, criteria, aims, config, None, None).await.expect("run_sync");

    let progress = engine.get_progress().await;
    assert_eq!(progress.total, 2, "progress total should match cap");
    assert_eq!(progress.completed, 2, "engine should stop at cap");

    {
        let conn = db.lock().expect("db lock");
        let screened: i64 = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE screened_at IS NOT NULL", [], |row| {
                row.get(0)
            })
            .expect("screened count");
        assert_eq!(screened, 2, "only capped number of articles should be screened");

        let unscreened: i64 = conn
            .query_row("SELECT COUNT(*) FROM articles WHERE screened_at IS NULL", [], |row| {
                row.get(0)
            })
            .expect("unscreened count");
        assert_eq!(unscreened, 3, "remaining articles should stay unscreened");
    }

    assert_eq!(
        llm.calls.load(Ordering::SeqCst),
        2,
        "LLM should be called only for capped articles"
    );
}
