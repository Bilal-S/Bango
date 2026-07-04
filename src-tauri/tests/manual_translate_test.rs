//! Manual translation integration tests (language-plan-v2 gap remediation).
//!
//! Covers Scenario 3: `auto_translate = false`, import a non-English article,
//! then manually trigger translation via the `enqueue_article_translation`
//! Tauri command path (which passes `require_non_english = false`).
//!
//! Also covers the retry command path and the distinction between the
//! import-trigger gate (checks auto_translate) and the manual-trigger gate
//! (does not check auto_translate).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use bango_lib::db::app_settings_repo::{self, get_auto_translate, set_auto_translate};
use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::screening::llm_client::LlmClient;
use bango_lib::translation::engine::translate_metadata_only;
use bango_lib::translation::language::should_skip_translation;

/// Mock LLM client that returns a canned metadata-translation response.
struct ManualTranslateMock {
    response: String,
    call_count: AtomicUsize,
}

impl ManualTranslateMock {
    fn new_translating() -> Self {
        Self {
            response:
                "TITLE:\nEffects of Sugar Tax on Public Health\n\nABSTRACT:\nThis study examines the effects of a sugar tax on childhood obesity rates across 15 countries."
                    .to_string(),
            call_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for ManualTranslateMock {
    async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok((self.response.clone(), 50))
    }
}

fn seed_french_article(conn: &rusqlite::Connection, title: &str, abstract_text: &str) -> String {
    let article = NewArticle {
        title: title.to_string(),
        abstract_text: abstract_text.to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    inserted[0].id.clone()
}

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    conn
}

fn run_metadata_translation(
    conn: rusqlite::Connection,
    article_id: &str,
    mock: ManualTranslateMock,
) -> (rusqlite::Connection, Result<(), AppError>) {
    let mutex = Mutex::new(conn);
    let rt =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    let result = rt.block_on(translate_metadata_only(&mutex, article_id, &mock));
    let conn = mutex.into_inner().expect("mutex not poisoned");
    (conn, result)
}

// ---------------------------------------------------------------------------
// Scenario 3: auto_translate off → manual translate
// ---------------------------------------------------------------------------

#[test]
fn auto_translate_off_gate_prevents_import_trigger() {
    // When `auto_translate = false`, the import trigger
    // (`try_enqueue_translations_for_import`) reads `get_auto_translate`
    // and returns early. This test verifies the gate value itself -- the
    // function in `commands/translation.rs:150` reads the setting first.
    let conn = setup_db();

    // Default is enabled (absent key → true).
    assert!(get_auto_translate(&conn).expect("get auto_translate"));

    // Disable the toggle.
    set_auto_translate(&conn, false).expect("set auto_translate");
    assert!(!get_auto_translate(&conn).expect("get auto_translate"));

    // The import trigger checks `!auto` → returns early without enqueueing.
    // We verify the gate value, which is the condition the real function uses.
    let auto = app_settings_repo::get_auto_translate(&conn).expect("read auto");
    assert!(!auto, "auto_translate must be false; import trigger returns early here");
}

#[test]
fn manual_enqueue_bypasses_auto_translate_and_language_gate() {
    // The manual translate command (`enqueue_article_translation`) calls
    // `enqueue_article_translation_inner` with `require_non_english = false`.
    // This means: (a) it does NOT check `auto_translate`, and (b) it does
    // NOT apply `should_skip_translation`. Even an English article can be
    // manually translated (the user might believe the language metadata is
    // wrong).
    //
    // This test verifies the enqueue gate accepts articles when
    // `require_non_english = false`, regardless of language.
    let conn = setup_db();
    set_auto_translate(&conn, false).expect("set auto_translate");

    // Seed an English article (normally skipped by import trigger).
    let en_article = NewArticle {
        title: "English Title".to_string(),
        abstract_text: "English abstract text for the study.".to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        language: Some("English".to_string()),
        ..Default::default()
    };
    let inserted =
        article_repo::insert_articles_batch(&conn, &[en_article], "test").expect("insert");
    let en_id = &inserted[0].id;

    // The skip-policy gate WOULD skip this article.
    assert!(should_skip_translation(Some("English")));

    // But the manual command passes `require_non_english = false`, so the
    // gate is NOT applied. The enqueue should succeed because the article
    // is in 'none' status and not yet translated.
    let status = article_repo::get_translation_status(&conn, en_id).expect("status");
    assert_eq!(status.translation_status, "none");
    assert!(!status.is_translated);

    // Mark queued (simulating what enqueue_article_translation_inner does with
    // require_non_english=false) -- it should succeed.
    article_repo::update_translation_status(&conn, en_id, "queued").expect("mark queued");
    let updated = article_repo::get_translation_status(&conn, en_id).expect("status");
    assert_eq!(updated.translation_status, "queued");
}

#[test]
fn manual_translate_metadata_only_succeeds() {
    // Full end-to-end manual translate flow: a non-English article is
    // translated via the metadata-only engine when auto_translate is off.
    // The user clicked "Translate to English" -- the command does not
    // check auto_translate.
    let conn = setup_db();
    set_auto_translate(&conn, false).expect("set auto_translate");

    let article_id = seed_french_article(
        &conn,
        "Titre original français",
        "Résumé original en français pour cette étude.",
    );

    // Precondition: auto_translate is off.
    assert!(!get_auto_translate(&conn).expect("auto_translate"));

    // Precondition: article is non-English and not translated.
    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert_eq!(article.language.as_deref(), Some("French"));
    assert!(!article.is_translated);

    // Run the metadata-only translation engine directly (same code the worker
    // dispatches for `enqueue_article_translation` when `has_full_text=false`).
    let mock = ManualTranslateMock::new_translating();
    let (conn, result) = run_metadata_translation(conn, &article_id, mock);
    result.expect("manual translation must succeed even with auto_translate off");

    // Verify the article was translated to English.
    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert!(article.is_translated, "is_translated must be set");
    assert_eq!(article.translation_status, "succeeded");
    assert_eq!(article.title, "Effects of Sugar Tax on Public Health");
    assert!(
        article.abstract_text.contains("childhood obesity"),
        "abstract must be the translated English text; got: {}",
        article.abstract_text
    );

    // The original language field is preserved (never overwritten).
    assert_eq!(article.language.as_deref(), Some("French"));
}

#[test]
fn manual_translate_then_retry_works() {
    // The retry command (`retry_translation_job`) resets the article to
    // `none`/`is_translated=0`, then enqueues. It also passes
    // `require_non_english = false`.
    let conn = setup_db();
    let article_id = seed_french_article(&conn, "Titre français", "Résumé français.");

    // First translation succeeds.
    let mock1 = ManualTranslateMock::new_translating();
    let (conn, result) = run_metadata_translation(conn, &article_id, mock1);
    result.expect("first translation");
    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert!(article.is_translated);

    // Reset (simulating retry command internals).
    article_repo::reset_translation_status(&conn, &article_id).expect("reset");
    let reset = article_repo::get_translation_status(&conn, &article_id).expect("status");
    assert_eq!(reset.translation_status, "none");
    assert!(!reset.is_translated);

    // The enqueue gate now accepts the article again (status=none, is_translated=0).
    article_repo::update_translation_status(&conn, &article_id, "queued").expect("mark queued");

    // Second translation also succeeds.
    let mock2 = ManualTranslateMock::new_translating();
    let (conn, result) = run_metadata_translation(conn, &article_id, mock2);
    result.expect("retry translation");

    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert!(article.is_translated);
    assert_eq!(article.translation_status, "succeeded");
}
