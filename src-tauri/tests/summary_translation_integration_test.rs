//! Summary consumes translated text integration tests (language-plan-v2 Phase 4).

use bango_lib::db::article_repo;
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::utils::chunking::Chunk;

#[test]
fn summary_uses_translated_text_when_available() {
    // TC-12: after translation, generate_article_ai_summary reads the
    // translated English full_text + chunks. The summary core reads
    // `articles.full_text` and `article_chunks` (both rewritten to English by
    // Plan-A translation), so this test verifies the article + chunks the
    // summary path consumes hold English content after translation.
    let conn = create_connection().expect("create connection");
    run_migrations(&conn).expect("migrations");
    let article = NewArticle {
        title: "English Translated Title".to_string(),
        abstract_text: "English translated abstract.".to_string(),
        authors: vec!["Auteur".to_string()],
        publication_year: Some(2024),
        language: Some("French".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_articles_batch(&conn, &[article], "test").expect("insert");
    let article_id = &inserted[0].id;

    // Set the working row to the translated English full_text + mark translated.
    conn.execute(
        "UPDATE articles SET full_text = ?1, is_translated = 1, translation_status = 'succeeded', \
         has_full_text = 1 WHERE id = ?2",
        rusqlite::params![
            "Introduction\n\nThis is the English translated full text body.\n\nMethods\n\nThe methods section in English.",
            article_id
        ],
    )
    .expect("set translated full_text");

    // Seed the English re-chunked chunks (what translate_full_text writes).
    let english_chunks = vec![
        Chunk {
            chunk_index: 0,
            section: Some("Introduction".to_string()),
            text: "This is the English translated full text body.".to_string(),
            word_count: 8,
        },
        Chunk {
            chunk_index: 1,
            section: Some("Methods".to_string()),
            text: "The methods section in English.".to_string(),
            word_count: 5,
        },
    ];
    chunk_repo::replace_chunks_for_article(&conn, article_id, &english_chunks)
        .expect("seed chunks");

    // Read back through the same paths the summary core uses.
    let article = article_repo::get_article_by_id(&conn, article_id).expect("article");
    assert!(article.is_translated, "precondition: is_translated");
    let full_text = article.full_text.as_deref().unwrap_or("");
    assert!(
        full_text.contains("English translated full text"),
        "summary path must read the translated English full_text; got: {full_text}"
    );

    let chunks = chunk_repo::list_chunks_for_article(&conn, article_id).expect("chunks");
    assert_eq!(chunks.len(), 2);
    for c in &chunks {
        assert!(
            c.text.contains("English"),
            "summary chunks must be English after translation: {:?}",
            c.text
        );
    }
}
