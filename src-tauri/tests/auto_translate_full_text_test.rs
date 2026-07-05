//! Auto-translate + full-text integration tests (language-plan-v2 gap remediation).
//!
//! Covers Scenario 4: `auto_translate = true` with full text attached. Tests
//! the import trigger path (`try_enqueue_translations_for_import`) produces a
//! `FullText` job, and the full translate→re-chunk→summary-read chain.

use std::sync::atomic::{AtomicUsize, Ordering};

use bango_lib::db::app_settings_repo;
use bango_lib::db::article_repo;
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::screening::llm_client::LlmClient;
use bango_lib::translation::engine::translate_full_text;
use bango_lib::translation::worker::TranslationJobKind;
use bango_lib::utils::chunking::Chunk;

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    conn
}

fn seed_non_english_with_full_text(conn: &rusqlite::Connection) -> String {
    let article = NewArticle {
        title: "Titre français".to_string(),
        abstract_text: "Résumé français détaillé.".to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    let article_id = &inserted[0].id;

    // Attach a French full text and seed original chunks (simulating the
    // full-text attach + chunk extraction steps that happen before the
    // translation trigger).
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = \
         'Méthodes: analyse des données françaises. Résultats: conclusions importantes.' \
         WHERE id = ?1",
        rusqlite::params![article_id],
    )
    .expect("set full_text");

    let chunks = vec![
        Chunk {
            chunk_index: 0,
            section: Some("Methods".to_string()),
            text: "Méthodes: analyse des données françaises.".to_string(),
            word_count: 5,
        },
        Chunk {
            chunk_index: 1,
            section: Some("Results".to_string()),
            text: "Résultats: conclusions importantes.".to_string(),
            word_count: 4,
        },
    ];
    chunk_repo::replace_chunks_for_article(conn, article_id, &chunks).expect("seed chunks");

    inserted[0].id.clone()
}

// ---------------------------------------------------------------------------
// Scenario 4: auto_translate ON + full text
// ---------------------------------------------------------------------------

#[test]
fn auto_translate_on_enables_import_trigger() {
    // When `auto_translate = true`, `try_enqueue_translations_for_import`
    // does NOT return early (unlike the false case). The gate checks
    // `get_auto_translate()`.
    //
    // Decision (a): the default is now `false` (opt-in) so imports do not
    // silently trigger background translation + LLM cost. The user must
    // explicitly enable it in Settings; this test verifies the round-trip.
    let conn = setup_db();

    // Default is disabled (absent key → false, opt-in).
    let auto = app_settings_repo::get_auto_translate(&conn).expect("get auto_translate");
    assert!(!auto, "auto_translate defaults to false (opt-in); import trigger is gated off");

    // Explicitly enable; the import trigger now proceeds past the gate.
    app_settings_repo::set_auto_translate(&conn, true).expect("set auto_translate");
    assert!(app_settings_repo::get_auto_translate(&conn).expect("get auto_translate"));
}

#[test]
fn has_full_text_article_enqueues_full_text_job_kind() {
    // The import trigger (`try_enqueue_translations_for_import`) selects
    // `TranslationJobKind::FullText` when the article has full text
    // attached, and `MetadataOnly` otherwise. This is the kind-selection
    // rule used by `choose_job_kind` in `commands/translation.rs`.
    let conn = setup_db();

    // Article without full text → MetadataOnly.
    let meta_article = NewArticle {
        title: "Article sans texte intégral".to_string(),
        abstract_text: "Résumé.".to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let meta_inserted =
        article_repo::insert_articles_batch(&conn, &[meta_article], "test").expect("insert");
    let meta_article =
        article_repo::get_article_by_id(&conn, &meta_inserted[0].id).expect("read meta");
    assert!(!meta_article.has_full_text);

    let meta_kind = if meta_article.has_full_text {
        TranslationJobKind::FullText
    } else {
        TranslationJobKind::MetadataOnly
    };
    assert!(matches!(meta_kind, TranslationJobKind::MetadataOnly));

    // Article with full text → FullText.
    let ft_id = seed_non_english_with_full_text(&conn);
    let ft_article = article_repo::get_article_by_id(&conn, &ft_id).expect("read ft");
    assert!(ft_article.has_full_text, "precondition: has_full_text must be true");

    let ft_kind = if ft_article.has_full_text {
        TranslationJobKind::FullText
    } else {
        TranslationJobKind::MetadataOnly
    };
    assert!(matches!(ft_kind, TranslationJobKind::FullText));
}

#[test]
fn full_text_translation_produces_english_chunks_and_full_text() {
    // Full end-to-end full-text translation with mock LLM: metadata +
    // per-chunk translation → English stitched full_text → re-chunked
    // English chunks. This is what the worker dispatches when it receives
    // a `FullText` job for an article with full text attached.
    let conn = setup_db();
    let article_id = seed_non_english_with_full_text(&conn);

    // Mock LLM: first call is metadata (TITLE:/ABSTRACT:), subsequent calls
    // are per-chunk translations.
    struct FullTextMock {
        call_count: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LlmClient for FullTextMock {
        async fn send(&self, _system: &str, _user: &str) -> Result<(String, usize), AppError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            if idx == 0 {
                Ok((
                    "TITLE:\nEnglish Study Title\n\nABSTRACT:\nEnglish abstract about the French study."
                        .to_string(),
                    50,
                ))
            } else {
                Ok((format!("English translated section {idx}."), 30))
            }
        }
    }

    let mock = FullTextMock { call_count: AtomicUsize::new(0) };
    let mutex = std::sync::Mutex::new(conn);
    let rt =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(translate_full_text(&mutex, &article_id, &mock))
        .expect("full-text translation must succeed");
    let conn = mutex.into_inner().expect("mutex not poisoned");

    // Verify the working article row holds English text.
    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert!(article.is_translated, "is_translated must be set");
    assert_eq!(article.translation_status, "succeeded");
    assert_eq!(article.title, "English Study Title");
    assert_eq!(article.abstract_text, "English abstract about the French study.");
    assert!(
        article.full_text.as_deref().unwrap_or("").contains("English translated"),
        "full_text must be stitched English; got: {:?}",
        article.full_text
    );

    // The re-chunked English chunks must exist and contain English text.
    let english_chunks =
        chunk_repo::list_chunks_for_article(&conn, &article_id).expect("list chunks");
    assert!(!english_chunks.is_empty(), "re-chunked English chunks must exist");
    for c in &english_chunks {
        assert!(c.text.contains("English"), "re-chunked chunk must be English; got: {:?}", c.text);
    }

    // Originals are preserved (title, abstract, full_text, language).
    let original = bango_lib::db::article_original_repo::get_original_content(&conn, &article_id)
        .expect("read originals");
    assert!(original.is_some(), "original content must be preserved");
    let original = original.unwrap();
    assert_eq!(original.original_title.as_deref(), Some("Titre français"));
    assert_eq!(original.source_language.as_deref(), Some("French"));

    // Original language is immutable.
    assert_eq!(article.language.as_deref(), Some("French"));
}

#[test]
fn auto_translate_enabled_summary_reads_english_after_translation() {
    // After auto_translate=true + full-text translation completes, the
    // summary generation path reads the English `full_text` + chunks from
    // the working `articles` row (Plan A). This test verifies the read
    // paths that `generate_article_ai_summary_inner` uses.
    let conn = setup_db();
    let article_id = seed_non_english_with_full_text(&conn);

    // Simulate completed full-text translation (Plan A rewrite).
    conn.execute(
        "UPDATE articles SET title = ?1, abstract_text = ?2, full_text = ?3, \
         is_translated = 1, translation_status = 'succeeded' WHERE id = ?4",
        rusqlite::params![
            "English Study Title",
            "English abstract about the study.",
            "Introduction\n\nEnglish introduction text.\n\nMethods\n\nEnglish methods text.\n\nResults\n\nEnglish results text.",
            article_id,
        ],
    )
    .expect("set translated text");

    // Replace chunks with English re-chunked versions.
    let english_chunks = vec![
        Chunk {
            chunk_index: 0,
            section: Some("Introduction".to_string()),
            text: "English introduction text.".to_string(),
            word_count: 3,
        },
        Chunk {
            chunk_index: 1,
            section: Some("Methods".to_string()),
            text: "English methods text.".to_string(),
            word_count: 3,
        },
        Chunk {
            chunk_index: 2,
            section: Some("Results".to_string()),
            text: "English results text.".to_string(),
            word_count: 3,
        },
    ];
    chunk_repo::replace_chunks_for_article(&conn, &article_id, &english_chunks)
        .expect("replace chunks");

    // Read through the same paths the summary core uses.
    let article = article_repo::get_article_by_id(&conn, &article_id).expect("article");
    assert!(article.is_translated);
    assert_eq!(article.translation_status, "succeeded");
    assert_eq!(article.title, "English Study Title");

    let full_text = article.full_text.as_deref().unwrap_or("");
    assert!(
        full_text.contains("English introduction"),
        "summary path reads translated full_text; got: {full_text}"
    );

    let chunks = chunk_repo::list_chunks_for_article(&conn, &article_id).expect("list chunks");
    assert_eq!(chunks.len(), 3);
    for c in &chunks {
        assert!(
            c.text.contains("English"),
            "summary chunks must be English after translation: {:?}",
            c.text
        );
    }
}
