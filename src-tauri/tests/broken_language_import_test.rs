//! Broken language import integration tests (language-plan-v2 gap remediation).
//!
//! Covers Scenario 2: importing articles with missing, blank, or garbage
//! language metadata. Verifies the import pipeline handles absent language
//! without crashing, the skip-policy gate blocks translation, and screening
//! can still read the article's original abstract.

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::screening::prompt::ArticleEntry;
use bango_lib::translation::language::{is_english_language, should_skip_translation};

fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    conn
}

fn seed_article(
    conn: &rusqlite::Connection,
    title: &str,
    abstract_text: &str,
    language: Option<&str>,
) -> String {
    let article = NewArticle {
        title: title.to_string(),
        abstract_text: abstract_text.to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2024),
        language: language.map(str::to_string),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(conn, &[article], "test").expect("insert");
    inserted[0].id.clone()
}

// ---------------------------------------------------------------------------
// Scenario 2: Broken abstracts without language
// ---------------------------------------------------------------------------

#[test]
fn import_article_with_none_language_does_not_crash() {
    // Importing an article with `language: None` (RIS record missing the LA
    // tag) must succeed without crash. The article is stored with language=NULL.
    let conn = setup_db();
    let article_id = seed_article(
        &conn,
        "A Study Without Language Metadata",
        "This is a perfectly valid English abstract about a clinical trial.",
        None,
    );

    let article = article_repo::get_article_by_id(&conn, &article_id).expect("read article");
    assert_eq!(article.language, None, "language must be None in the database");
    assert_eq!(article.title, "A Study Without Language Metadata");
    assert!(!article.is_translated);
    assert_eq!(article.translation_status, "none");
}

#[test]
fn absent_language_skips_translation() {
    // Articles with `language = None` must be skipped by the translation
    // pipeline. `should_skip_translation(None)` returns true per plan §G.
    let conn = setup_db();
    let article_id =
        seed_article(&conn, "Clinical Study", "This study examines treatment outcomes.", None);

    let article = article_repo::get_article_by_id(&conn, &article_id).expect("read article");
    assert!(should_skip_translation(article.language.as_deref()));
    assert!(!is_english_language(article.language.as_deref()));
    // is_english_language returns false for None (it's not English),
    // but should_skip_translation returns true (unknown → skip).
    // This distinction matters: the article is NOT English, but we
    // still don't translate it because we don't know what it is.
}

#[test]
fn blank_and_whitespace_language_also_skips() {
    // Empty string and whitespace-only language values must be treated the
    // same as None: skip translation.
    let conn = setup_db();

    let empty_id = seed_article(&conn, "Study A", "Abstract text.", Some(""));
    let whitespace_id = seed_article(&conn, "Study B", "Abstract text.", Some("   "));

    let empty_article = article_repo::get_article_by_id(&conn, &empty_id).expect("read empty");
    let ws_article = article_repo::get_article_by_id(&conn, &whitespace_id).expect("read ws");

    // Both return true for should_skip_translation.
    assert!(should_skip_translation(empty_article.language.as_deref()));
    assert!(should_skip_translation(ws_article.language.as_deref()));
}

#[test]
fn absent_language_article_still_screenable() {
    // An article with no language metadata can still be screened. The
    // screening engine builds an `ArticleEntry` from `article.abstract_text`,
    // which is available regardless of the language field.
    let conn = setup_db();
    let article_id = seed_article(
        &conn,
        "Clinical Trial Results",
        "This study demonstrates that the intervention significantly reduced mortality.",
        None,
    );

    let article = article_repo::get_article_by_id(&conn, &article_id).expect("read article");
    assert_eq!(article.language, None);

    // Build a screening entry (mirrors screening/engine.rs:362).
    let entry = ArticleEntry::new(
        article.title.clone(),
        String::new(),
        article.publication_year,
        article.abstract_text.clone(),
    );
    assert_eq!(entry.title, "Clinical Trial Results");
    assert!(
        entry.abstract_text.contains("significantly reduced mortality"),
        "screening must read the abstract regardless of missing language"
    );

    // The translation status fields default correctly.
    assert!(!article.is_translated);
    assert_eq!(article.translation_status, "none");
}

#[test]
fn mixed_language_batch_handles_all_cases() {
    // A batch of articles with various language values (English, French,
    // None, blank) must all import successfully. Only the non-English
    // article with a known language should be eligible for translation.
    let conn = setup_db();

    let articles = vec![
        NewArticle {
            title: "English Paper".to_string(),
            abstract_text: "Abstract.".to_string(),
            authors: vec!["A".to_string()],
            publication_year: Some(2024),
            language: Some("English".to_string()),
            ..Default::default()
        },
        NewArticle {
            title: "French Paper".to_string(),
            abstract_text: "Résumé.".to_string(),
            authors: vec!["B".to_string()],
            publication_year: Some(2024),
            language: Some("French".to_string()),
            ..Default::default()
        },
        NewArticle {
            title: "No Language Paper".to_string(),
            abstract_text: "Abstract.".to_string(),
            authors: vec!["C".to_string()],
            publication_year: Some(2024),
            language: None,
            ..Default::default()
        },
        NewArticle {
            title: "Blank Language Paper".to_string(),
            abstract_text: "Abstract.".to_string(),
            authors: vec!["D".to_string()],
            publication_year: Some(2024),
            language: Some("".to_string()),
            ..Default::default()
        },
    ];

    let inserted =
        article_repo::insert_articles_batch(&conn, &articles, "test").expect("batch insert");
    assert_eq!(inserted.len(), 4, "all four articles must be inserted");

    // Verify each article's skip-policy outcome.
    let en = &inserted[0];
    let fr = &inserted[1];
    let none_lang = &inserted[2];
    let blank = &inserted[3];

    let en_article = article_repo::get_article_by_id(&conn, &en.id).expect("en");
    let fr_article = article_repo::get_article_by_id(&conn, &fr.id).expect("fr");
    let none_article = article_repo::get_article_by_id(&conn, &none_lang.id).expect("none");
    let blank_article = article_repo::get_article_by_id(&conn, &blank.id).expect("blank");

    // English → skip (already English).
    assert!(should_skip_translation(en_article.language.as_deref()));
    // French → do NOT skip (needs translation).
    assert!(!should_skip_translation(fr_article.language.as_deref()));
    // None → skip (unknown language).
    assert!(should_skip_translation(none_article.language.as_deref()));
    // Blank → skip (unknown language).
    assert!(should_skip_translation(blank_article.language.as_deref()));

    // All four have valid abstracts for screening.
    for a in &inserted {
        let article = article_repo::get_article_by_id(&conn, &a.id).expect("read");
        let entry = ArticleEntry::new(
            article.title.clone(),
            String::new(),
            article.publication_year,
            article.abstract_text.clone(),
        );
        assert!(!entry.title.is_empty());
        assert!(!entry.abstract_text.is_empty());
    }
}
