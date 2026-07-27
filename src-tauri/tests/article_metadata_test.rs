use bango_lib::db::article_repo::{self, ArticleMetaField, ArticleMetaValue};
use bango_lib::db::audit_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::models::audit::AuditAction;

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: insert one article and return its id.
fn seed_article(conn: &rusqlite::Connection) -> String {
    let article = NewArticle { title: "Test Article".to_string(), ..Default::default() };
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    inserted.id
}

// ─── Round-trip tests for each editable field ─────────────────────

#[test]
fn test_update_doi() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Doi,
        ArticleMetaValue::Scalar(Some("10.1001/test.2024".to_string())),
    )
    .expect("update DOI failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.doi.as_deref(), Some("10.1001/test.2024"));
}

#[test]
fn test_update_journal() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Journal,
        ArticleMetaValue::Scalar(Some("The Lancet".to_string())),
    )
    .expect("update journal failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.journal.as_deref(), Some("The Lancet"));
}

#[test]
fn test_update_language() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Language,
        ArticleMetaValue::Scalar(Some("English".to_string())),
    )
    .expect("update language failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.language.as_deref(), Some("English"));
}

#[test]
fn test_update_affiliation() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Affiliation,
        ArticleMetaValue::Scalar(Some("MIT, CSAIL".to_string())),
    )
    .expect("update affiliation failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.affiliation.as_deref(), Some("MIT, CSAIL"));
}

#[test]
fn test_update_publication_year_valid() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("2024".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, Some(2024));
}

#[test]
fn test_update_publication_year_clears_on_empty() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // Set a year first.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("2024".to_string())),
    )
    .expect("set year failed");

    // Clear it with an empty string -> should become NULL.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some(String::new())),
    )
    .expect("clear year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, None);
}

#[test]
fn test_update_publication_year_invalid_becomes_null() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // A non-numeric string should parse to None (clearing the field).
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("not-a-year".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, None);
}

#[test]
fn test_update_authors_array() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Authors,
        ArticleMetaValue::Array(vec![
            "Smith J".to_string(),
            "Doe A".to_string(),
            "Roe B".to_string(),
        ]),
    )
    .expect("update authors failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.authors, vec!["Smith J", "Doe A", "Roe B"]);
}

#[test]
fn test_update_authors_empty_array() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Authors,
        ArticleMetaValue::Array(vec!["X".to_string()]),
    )
    .expect("set authors failed");

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Authors,
        ArticleMetaValue::Array(vec![]),
    )
    .expect("clear authors failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert!(article.authors.is_empty());
}

#[test]
fn test_update_keywords_array() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Keywords,
        ArticleMetaValue::Array(vec!["obesity".to_string(), "sugar-tax".to_string()]),
    )
    .expect("update keywords failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.keywords, vec!["obesity", "sugar-tax"]);
}

#[test]
fn test_scalar_empty_string_clears_to_null() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // Set a DOI.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Doi,
        ArticleMetaValue::Scalar(Some("10.1001/set".to_string())),
    )
    .expect("set DOI failed");

    // Clear with empty string.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Doi,
        ArticleMetaValue::Scalar(Some(String::new())),
    )
    .expect("clear DOI failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.doi, None);
}

#[test]
fn test_scalar_whitespace_only_clears_to_null() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Doi,
        ArticleMetaValue::Scalar(Some("   ".to_string())),
    )
    .expect("update DOI failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.doi, None);
}

// ─── Year range guard (1800–2100) ──────────────────────────────────

#[test]
fn test_update_publication_year_below_min_clears_to_null() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // 1799 is below MIN_PUBLICATION_YEAR (1800) -> cleared to NULL.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("1799".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, None);
}

#[test]
fn test_update_publication_year_above_max_clears_to_null() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // 2101 is above MAX_PUBLICATION_YEAR (2100) -> cleared to NULL.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("2101".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, None);
}

#[test]
fn test_update_publication_year_boundary_min_1800_accepted() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("1800".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, Some(1800));
}

#[test]
fn test_update_publication_year_boundary_max_2100_accepted() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::PublicationYear,
        ArticleMetaValue::Scalar(Some("2100".to_string())),
    )
    .expect("update year failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.publication_year, Some(2100));
}

// ─── Journal re-link on edit ──────────────────────────────────────

#[test]
fn test_journal_edit_sets_journal_index_id_to_null_for_unrecognized() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // Set a journal name that does NOT exist in journal_index.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Journal,
        ArticleMetaValue::Scalar(Some("Totally Unknown Journal".to_string())),
    )
    .expect("update journal failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.journal.as_deref(), Some("Totally Unknown Journal"));
    // journal_index_id must be NULL because the name is not in the index.
    assert!(article.journal_index_id.is_none());
}

#[test]
fn test_journal_edit_resolves_journal_index_id_for_known_journal() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // Seed a journal_index row.
    conn.execute(
        "INSERT INTO journal_index (id, journal_title) VALUES ('jidx-1', 'The Lancet')",
        [],
    )
    .expect("seed journal_index failed");

    // Edit the article's journal to match the seeded row.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Journal,
        ArticleMetaValue::Scalar(Some("The Lancet".to_string())),
    )
    .expect("update journal failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.journal.as_deref(), Some("The Lancet"));
    // journal_index_id must be resolved to the seeded row's id.
    assert_eq!(article.journal_index_id.as_deref(), Some("jidx-1"));
}

// ─── Audit trail test ─────────────────────────────────────────────

#[test]
fn test_metadata_edit_writes_audit_row() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Doi,
        ArticleMetaValue::Scalar(Some("10.1001/audited".to_string())),
    )
    .expect("update failed");

    // Simulate the command-layer audit write.
    audit_repo::create_or_update_entry(
        &conn,
        &id,
        "metadata_edit",
        None,
        None,
        Some(&format!("Metadata edited: {}", ArticleMetaField::Doi.label())),
        "user",
    )
    .expect("audit write failed");

    let trail = audit_repo::get_audit_trail(&conn, &id).expect("audit trail fetch failed");
    let metadata_entries: Vec<_> =
        trail.iter().filter(|e| e.action == AuditAction::MetadataEdit).collect();
    assert_eq!(metadata_entries.len(), 1, "expected exactly one metadata_edit audit row");
    assert!(
        metadata_entries[0].details.as_ref().is_some_and(|d| d.contains("DOI")),
        "audit detail should mention the field label"
    );
}

// ─── Title tests (TEXT NOT NULL field) ─────────────────────────────

#[test]
fn test_update_title() {
    let conn = setup_db();
    let id = seed_article(&conn);

    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Title,
        ArticleMetaValue::Scalar(Some("A New Title".to_string())),
    )
    .expect("update title failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.title, "A New Title");
}

#[test]
fn test_update_title_empty_rejected() {
    let conn = setup_db();
    let id = seed_article(&conn);

    // Empty string must be rejected (title is NOT NULL).
    let result = article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Title,
        ArticleMetaValue::Scalar(Some(String::new())),
    );
    assert!(result.is_err(), "empty title should be rejected");

    // Whitespace-only must also be rejected.
    let result = article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Title,
        ArticleMetaValue::Scalar(Some("   ".to_string())),
    );
    assert!(result.is_err(), "whitespace-only title should be rejected");

    // None payload must also be rejected.
    let result = article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Title,
        ArticleMetaValue::Scalar(None),
    );
    assert!(result.is_err(), "None title payload should be rejected");

    // Original title is preserved after the rejected updates.
    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.title, "Test Article");

    // A valid non-empty title still works after the rejected attempts.
    article_repo::update_article_metadata_field(
        &conn,
        &id,
        ArticleMetaField::Title,
        ArticleMetaValue::Scalar(Some("Valid Replacement".to_string())),
    )
    .expect("valid title update failed");
    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.title, "Valid Replacement");
}
