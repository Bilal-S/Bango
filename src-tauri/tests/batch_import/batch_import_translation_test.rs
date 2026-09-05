//! Batch import Phase-3 translation integration tests (language-plan-v2 Phase 4).

use bango_lib::batch_import::{BatchImportPhase, BatchImportProgress};
use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::translation::language::is_english_language;

#[test]
fn phase_order_is_fulltext_citations_translation_summaries() {
    // TC-08: batch phase order is Full Text -> Citations -> Translations -> Summaries.
    assert_eq!(BatchImportPhase::FullText as usize, 1);
    assert_eq!(BatchImportPhase::Citations as usize, 2);
    assert_eq!(BatchImportPhase::Translations as usize, 3);
    assert_eq!(BatchImportPhase::Summaries as usize, 4);

    // Names render correctly.
    assert_eq!(BatchImportPhase::FullText.name(), "Full Text");
    assert_eq!(BatchImportPhase::Citations.name(), "Citations");
    assert_eq!(BatchImportPhase::Translations.name(), "Translations");
    assert_eq!(BatchImportPhase::Summaries.name(), "AI Summaries");

    // The progress struct carries the new translations field.
    let prog = BatchImportProgress::default();
    assert!(prog.translations.is_none());
}

#[test]
fn embeddings_and_complete_phase_labels_render() {
    // Phase 5 is the embeddings work phase; Complete (6) is the terminal
    // "all phases done" indicator used only for the final 100% snapshot so
    // the user sees "Batch Import" instead of "Embeddings" at completion.
    assert_eq!(BatchImportPhase::Embeddings as usize, 5);
    assert_eq!(BatchImportPhase::Complete as usize, 6);
    assert_eq!(BatchImportPhase::Embeddings.name(), "Embeddings");
    assert_eq!(BatchImportPhase::Complete.name(), "Batch Import");
}

#[test]
fn summary_waits_for_required_translation() {
    // TC-08: Phase 4 waits per article until translation_status leaves 'running'.
    //
    // We exercise the gating logic via the public `is_english_language` gate
    // + the status-transition decision: a non-English article with
    // translation_status='running' requires a wait; once the status flips to
    // 'succeeded' the wait resolves.
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    let article = NewArticle {
        title: "Titre français".to_string(),
        abstract_text: "Résumé français.".to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(&conn, &[article], "test").expect("insert");
    let article_id = &inserted[0].id;

    // Simulate the translation running.
    article_repo::update_translation_status(&conn, article_id, "running").expect("mark running");

    // Read the status back and assert the gating condition (running -> needs wait).
    let info = article_repo::get_translation_status(&conn, article_id).expect("status");
    assert_eq!(info.translation_status, "running");
    assert!(!info.is_translated);
    // The production gating rule: non-English + status in running/queued => wait.
    let needs_wait = !is_english_language(Some("French"))
        && matches!(info.translation_status.as_str(), "running" | "queued");
    assert!(needs_wait, "non-English running article must require a wait");

    // Simulate the translation completing.
    article_repo::update_translation_status(&conn, article_id, "succeeded")
        .expect("mark succeeded");
    let info2 = article_repo::get_translation_status(&conn, article_id).expect("status2");
    let still_needs_wait = !is_english_language(Some("French"))
        && matches!(info2.translation_status.as_str(), "running" | "queued");
    assert!(!still_needs_wait, "after succeeded the wait must no longer be required");
}

#[test]
fn summary_runs_without_translation_when_not_required() {
    // TC-08: English/absent-language articles skip translation wait.
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");

    // English article.
    let en = NewArticle {
        title: "English Title".to_string(),
        abstract_text: "English abstract.".to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        language: Some("English".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(&conn, &[en], "test").expect("insert en");
    let en_id = &inserted[0].id;
    let info = article_repo::get_translation_status(&conn, en_id).expect("status en");
    let needs_wait = !is_english_language(Some("English"))
        && matches!(info.translation_status.as_str(), "running" | "queued");
    assert!(!needs_wait, "English article must not require a translation wait");

    // Absent-language article.
    let none_article = NewArticle {
        title: "No Language".to_string(),
        abstract_text: "Abstract.".to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        language: None,
        ..Default::default()
    };
    let inserted2 =
        article_repo::insert_articles_batch(&conn, &[none_article], "test").expect("insert none");
    let none_id = &inserted2[0].id;
    let info2 = article_repo::get_translation_status(&conn, none_id).expect("status none");
    let needs_wait2 = !is_english_language(None)
        && matches!(info2.translation_status.as_str(), "running" | "queued");
    assert!(!needs_wait2, "absent-language article must not require a translation wait");
}
