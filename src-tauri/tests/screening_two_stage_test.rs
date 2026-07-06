//! Tier 3 §T3.7 binding inventory tests for the two-stage screening flow
//! (`screening::engine`) plus the Phase E budget-guard integration test.

use std::sync::atomic::{AtomicUsize, Ordering};

use bango_lib::db::article_repo;
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::criteria_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::criterion::{Criterion, CriterionType, ResearchAim};
use bango_lib::screening::chunk_retrieval::{rank_chunks_by_criteria, DEFAULT_MAX_CHUNK_WORDS};
use bango_lib::screening::engine::{ScreeningConfig, ScreeningEngine};
use bango_lib::screening::llm_client::LlmClient;
use bango_lib::utils::chunking::Chunk;

fn setup_db() -> std::sync::Mutex<rusqlite::Connection> {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    std::sync::Mutex::new(conn)
}

fn seed_article_with_full_text(conn: &rusqlite::Connection, title: &str) -> String {
    let article = NewArticle {
        title: title.to_string(),
        abstract_text: format!("Abstract for {title} about sugar taxes in children."),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        keywords: vec!["sugar".to_string()],
        import_source: Some("test".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    let id = inserted[0].id.clone();
    article_repo::move_articles_to_working_batch(conn, std::slice::from_ref(&id))
        .expect("move to working");
    conn.execute("UPDATE articles SET has_full_text = 1 WHERE id = ?1", rusqlite::params![id])
        .expect("set has_full_text");
    id
}

/// Like `seed_article_with_full_text` but leaves `has_full_text = 0` (the
/// default), so the engine must fall back to abstract-only screening even
/// when an advanced mode is configured.
fn seed_article_without_full_text(conn: &rusqlite::Connection, title: &str) -> String {
    let article = NewArticle {
        title: title.to_string(),
        abstract_text: format!("Abstract for {title} about sugar taxes in children."),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        keywords: vec!["sugar".to_string()],
        import_source: Some("test".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    let id = inserted[0].id.clone();
    article_repo::move_articles_to_working_batch(conn, std::slice::from_ref(&id))
        .expect("move to working");
    id
}

fn seed_chunks(conn: &rusqlite::Connection, article_id: &str, chunks: &[(&str, &str)]) {
    let chunk_structs: Vec<Chunk> = chunks
        .iter()
        .enumerate()
        .map(|(i, (section, text))| Chunk {
            chunk_index: i,
            section: Some(section.to_string()),
            text: text.to_string(),
            word_count: text.split_whitespace().count(),
        })
        .collect();
    chunk_repo::replace_chunks_for_article(conn, article_id, &chunk_structs)
        .expect("insert chunks");
}

fn seed_criteria(conn: &rusqlite::Connection) -> (Vec<Criterion>, Vec<ResearchAim>) {
    let aim = criteria_repo::create_aim(conn, "Study sugar taxes").expect("aim");
    let inc =
        criteria_repo::create_criterion(conn, "inclusion", "Must be about sugar taxes", "standard")
            .expect("inc");
    let exc = criteria_repo::create_criterion(conn, "exclusion", "Not about children", "standard")
        .expect("exc");
    (vec![inc, exc], vec![aim])
}

/// Return the inclusion criterion's UUID (used so the engine's match resolver
/// can find it; the test DB uses UUIDs, not literal ids).
fn inclusion_id(criteria: &[Criterion]) -> String {
    criteria
        .iter()
        .find(|c| matches!(c.criterion_type, CriterionType::Inclusion))
        .expect("inclusion criterion")
        .id
        .clone()
}

/// Build a single-article JSON response. `inc_id` is the real inclusion
/// criterion UUID so the resolver finds a match; pass a placeholder for
/// call-count-only tests (no match → resolves to "exclude").
fn response(decision: &str, confidence: f64, inc_id: &str) -> String {
    format!(
        r#"[{{"decision":"{decision}","reasoning":"R","matchedInclusionCriteria":["{inc_id}"],"matchedExclusionCriteria":[],"suggestedTags":[],"confidence":{confidence}}}]"#
    )
}

/// Counting mock that returns stage-1 for call 0 and stage-2 for later calls.
/// Records whether each call's prompt contained the evidence block.
struct CountingMock {
    stage1: String,
    stage2: String,
    call_count: AtomicUsize,
    evidence_seen: std::sync::Mutex<Vec<bool>>,
}

impl CountingMock {
    fn new(stage1: String, stage2: String) -> Self {
        Self {
            stage1,
            stage2,
            call_count: AtomicUsize::new(0),
            evidence_seen: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for CountingMock {
    async fn send(&self, _system: &str, user: &str) -> Result<(String, usize), AppError> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        self.evidence_seen
            .lock()
            .expect("evidence_seen")
            .push(user.contains("Supporting Evidence from Full Text"));
        let resp = if idx == 0 { self.stage1.clone() } else { self.stage2.clone() };
        Ok((resp, 100))
    }
}

fn two_stage_config() -> ScreeningConfig {
    ScreeningConfig {
        mode: bango_lib::db::app_settings_repo::ScreeningMode::TwoStage,
        two_stage_low: 0.4,
        two_stage_high: 0.7,
        ..ScreeningConfig::default()
    }
}

fn enhanced_config() -> ScreeningConfig {
    ScreeningConfig {
        mode: bango_lib::db::app_settings_repo::ScreeningMode::Enhanced,
        ..ScreeningConfig::default()
    }
}

// ── §T3.7 binding inventory ────────────────────────────────────────────────

#[tokio::test]
async fn two_stage_skips_clear_cut_include() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Clear Include");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // confidence 0.95 → clear-cut include, no stage 2.
    let mock = CountingMock::new(response("include", 0.95, &inc_id), String::new());
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");
    assert_eq!(
        mock.call_count.load(Ordering::SeqCst),
        1,
        "clear-cut include must NOT trigger stage 2"
    );
}

#[tokio::test]
async fn two_stage_skips_clear_cut_exclude() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Clear Exclude");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // confidence 0.2 → clear-cut exclude, no stage 2.
    let mock = CountingMock::new(response("exclude", 0.2, &inc_id), String::new());
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");
    assert_eq!(
        mock.call_count.load(Ordering::SeqCst),
        1,
        "clear-cut exclude must NOT trigger stage 2"
    );
}

#[tokio::test]
async fn two_stage_triggers_on_borderline() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Borderline");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // Stage 1: confidence 0.55 (borderline). Stage 2: override to include @ 0.9.
    let mock =
        CountingMock::new(response("exclude", 0.55, &inc_id), response("include", 0.9, &inc_id));
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");
    assert_eq!(
        mock.call_count.load(Ordering::SeqCst),
        2,
        "borderline article must trigger stage 2"
    );
    let conn = db.lock().unwrap();
    let status: String = conn
        .query_row("SELECT status FROM articles LIMIT 1", [], |r| r.get(0))
        .expect("read status");
    assert_eq!(status, "included", "stage-2 decision must override stage-1");
}

#[tokio::test]
async fn two_stage_logs_both_passes_to_audit() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Borderline Audit");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    let mock =
        CountingMock::new(response("exclude", 0.55, &inc_id), response("include", 0.9, &inc_id));
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT action FROM audit_entries WHERE action LIKE 'ai_screen%'")
        .expect("prepare");
    let actions: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query_map")
        .filter_map(|r| r.ok())
        .collect();
    assert!(actions.iter().any(|a| a == "ai_screen"), "stage-1 entry missing: {actions:?}");
    assert!(
        actions.iter().any(|a| a == "ai_screen_enhanced"),
        "stage-2 entry missing: {actions:?}"
    );
}

#[tokio::test]
async fn enhanced_mode_always_sends_evidence() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Enhanced High Conf");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // Enhanced mode: even at confidence 0.95, the single call must carry evidence.
    let mock = CountingMock::new(response("include", 0.95, &inc_id), String::new());
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, enhanced_config(), None)
        .await
        .expect("run_sync");
    let evidence_seen = mock.evidence_seen.lock().unwrap().clone();
    assert!(
        evidence_seen.first() == Some(&true),
        "enhanced mode must always send evidence: {evidence_seen:?}"
    );
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 1, "enhanced mode is single-stage");
}

// ── Phase E: budget guard integration (end-to-end) ─────────────────────────

#[test]
fn budget_guard_drops_lowest_chunk_when_over_budget() {
    // 3 chunks each ~300 words, top_k=3, budget=700 => 3x~302=906 > 700
    // → drop lowest → 2x~302=604 <= 700. Exercises `enforce_word_budget`
    // end-to-end through `rank_chunks_by_criteria` (the Phase-B guard).
    let words = "word ".repeat(300);
    let chunks: Vec<Chunk> = (0..3)
        .map(|i| Chunk {
            chunk_index: i,
            section: Some("Methods".to_string()),
            text: format!("sugar tax {words}{i}"),
            word_count: 302,
        })
        .collect();
    let inc = vec!["sugar tax".to_string()];
    let out = rank_chunks_by_criteria(&chunks, &inc, &[], 3, DEFAULT_MAX_CHUNK_WORDS, 700);
    assert_eq!(out.len(), 2, "budget guard drops 1 chunk: got {}", out.len());
}

// ── Tier 3 Gap 3: stage-2 progress sub-line must advance even when the ──────
//    borderline article's evidence is fully filtered out by the section
//    allow-list. Previously the `None => continue` arm skipped the progress
//    update, leaving the UI stuck at `Stage 2: 0/N borderline`.

#[tokio::test]
async fn two_stage_progress_updates_when_evidence_filtered_out() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Borderline No Evidence");
        // Seed a chunk whose section is "Discussion" - outside the default
        // allow-list `[Methods, Results]`, so `rank_and_format_evidence`
        // returns `None` and stage 2 hits the `None => continue` arm.
        seed_chunks(&conn, &id, &[("Discussion", "sugar tax discussion limitations")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // Stage 1: borderline confidence 0.55 (triggers stage 2).
    let mock = CountingMock::new(
        response("exclude", 0.55, &inc_id),
        // Stage 2 response won't actually be consumed because evidence is None
        // and the loop continues before the LLM call.
        response("include", 0.9, &inc_id),
    );
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");

    // The progress stage sub-line must have advanced to 1/1, not stalled at 0/1.
    let progress = engine.get_progress().await;
    assert!(
        progress.stage.as_deref().is_some_and(|s| s.contains("1/1")),
        "stage-2 progress must advance when evidence filtered out: got {:?}",
        progress.stage
    );
}

// ── Tier 3 Gap 6: two-stage screening must accumulate `actual_tokens` ───────
//    across both stages, not overwrite stage-1 with stage-2. The UPDATE
//    now uses `COALESCE(actual_tokens, 0) + ?` so the column reflects the
//    combined LLM cost for borderline articles.

#[tokio::test]
async fn two_stage_accumulates_actual_tokens() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Borderline Tokens");
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // Both stages return 100 tokens; the article should end up with 200
    // (100 stage 1 + 100 stage 2), not 100 (stage 2 overwriting stage 1).
    let mock =
        CountingMock::new(response("exclude", 0.55, &inc_id), response("include", 0.9, &inc_id));
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");

    let conn = db.lock().unwrap();
    let tokens: Option<i64> = conn
        .query_row("SELECT actual_tokens FROM articles LIMIT 1", [], |r| r.get(0))
        .expect("read actual_tokens");
    // 100 (stage 1) + 100 (stage 2) = 200. The old flat-write code returned 100.
    assert_eq!(tokens, Some(200), "two-stage must accumulate actual_tokens across stages");
}

// ── Tier 3 Gap 7: enhanced-mode audit label must name the section(s) that ───
//    actually matched, not the configured allow-list. Seeds an article with
//    only a Methods chunk (no Results) and asserts the audit `details` line
//    contains `§Methods` and does NOT contain `§Results`.

#[tokio::test]
async fn enhanced_audit_label_names_matched_section_only() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        let id = seed_article_with_full_text(&conn, "Enhanced Methods Only");
        // Only a Methods chunk - no Results chunk exists for this article.
        seed_chunks(&conn, &id, &[("Methods", "sugar tax study design rct children")]);
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    let mock = CountingMock::new(response("include", 0.95, &inc_id), String::new());
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, enhanced_config(), None)
        .await
        .expect("run_sync");

    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT details FROM audit_entries WHERE action = 'ai_screen_enhanced'")
        .expect("prepare");
    let details: String = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query_map")
        .next()
        .expect("at least one ai_screen_enhanced entry")
        .expect("row ok");
    assert!(details.contains("§Methods"), "audit must name the matched Methods section: {details}");
    assert!(
        !details.contains("§Results"),
        "audit must NOT claim §Results when no Results chunk was sent: {details}"
    );
}

// ── Always-selectable mode: Enhanced / Two-stage configured but the article
//    has NO full text attached (`has_full_text = 0`). The engine must fall
//    back to abstract-only screening for that article - no evidence block in
//    the prompt, and (for two-stage) no stage-2 call even at borderline conf.
//    This locks in the contract that all three modes are always selectable in
//    the Settings UI while the engine degrades per-article.

#[tokio::test]
async fn enhanced_mode_falls_back_to_abstract_when_no_full_text() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        // No full text, no chunks - pure abstract article.
        seed_article_without_full_text(&conn, "Abstract Only Enhanced");
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    let mock = CountingMock::new(response("include", 0.95, &inc_id), String::new());
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, enhanced_config(), None)
        .await
        .expect("run_sync");

    // Single LLM call (enhanced is single-stage).
    assert_eq!(mock.call_count.load(Ordering::SeqCst), 1);
    // The prompt must NOT carry the evidence block since there is no full text.
    let evidence_seen = mock.evidence_seen.lock().unwrap().clone();
    assert!(
        evidence_seen.first() == Some(&false),
        "enhanced mode must fall back to abstract-only when no full text: {evidence_seen:?}"
    );
}

#[tokio::test]
async fn two_stage_mode_falls_back_to_abstract_when_no_full_text() {
    let db = setup_db();
    let (criteria, aims) = {
        let conn = db.lock().unwrap();
        // No full text, no chunks - pure abstract article.
        seed_article_without_full_text(&conn, "Abstract Only Two-Stage");
        seed_criteria(&conn)
    };
    let inc_id = inclusion_id(&criteria);
    // Stage-1 confidence 0.55 is borderline, but without full text the engine
    // must NOT trigger stage 2 (the borderline filter requires has_full_text).
    let mock =
        CountingMock::new(response("exclude", 0.55, &inc_id), response("include", 0.9, &inc_id));
    let engine = ScreeningEngine::with_batch_size(1);
    engine
        .run_sync(&db, &mock, 0, criteria, aims, two_stage_config(), None)
        .await
        .expect("run_sync");

    assert_eq!(
        mock.call_count.load(Ordering::SeqCst),
        1,
        "two-stage must NOT trigger stage 2 for articles without full text"
    );
    let evidence_seen = mock.evidence_seen.lock().unwrap().clone();
    assert!(
        evidence_seen.first() == Some(&false),
        "two-stage stage 1 must be abstract-only when no full text: {evidence_seen:?}"
    );
}
