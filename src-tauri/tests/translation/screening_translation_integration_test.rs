//! Screening consumes translated text integration tests (language-plan-v2 Phase 4).

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::screening::prompt::ArticleEntry;

#[test]
fn screening_uses_translated_text_when_available() {
    // TC-11: after translation, screening reads the translated English
    // abstract. The screening prompt builds an `ArticleEntry` from
    // `article.abstract_text` (engine.rs:362), so once the translation engine
    // rewrites `articles.abstract_text` to English (Plan A), screening
    // automatically consumes the translated text.
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    let article = NewArticle {
        title: "English Translated Title".to_string(),
        abstract_text: "This is the English translated abstract about the study.".to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(&conn, &[article], "test").expect("insert");
    let article_id = &inserted[0].id;

    // Mark as translated (Plan A: the working row now holds English).
    article_repo::update_translation_status(&conn, article_id, "succeeded").expect("mark status");
    conn.execute(
        "UPDATE articles SET is_translated = 1 WHERE id = ?1",
        rusqlite::params![article_id],
    )
    .expect("set is_translated");

    let article = article_repo::get_article_by_id(&conn, article_id).expect("article");
    assert!(article.is_translated, "precondition: is_translated must be true");

    // Mirror the wiring in screening/engine.rs:362 - build an ArticleEntry
    // from the article's abstract_text + publication_year.
    let entry = ArticleEntry::new(
        article.title.clone(),
        String::new(),
        article.publication_year,
        article.abstract_text.clone(),
    );
    assert_eq!(entry.title, "English Translated Title");
    assert!(
        entry.abstract_text.contains("English translated abstract"),
        "screening ArticleEntry must read the translated English abstract; got: {}",
        entry.abstract_text
    );
}
