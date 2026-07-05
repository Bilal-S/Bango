use bango_lib::commands::import::{extract_cr_for_imported, ris_record_to_new_article};
use bango_lib::db::article_repo;
use bango_lib::db::audit_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::reference_repo;
use bango_lib::models::audit::AuditAction;
use bango_lib::ris::types::RisRecord;
use std::collections::HashMap;

fn make_ris_record(title: &str, cr_entries: Vec<&str>) -> RisRecord {
    let mut extras = HashMap::new();
    if !cr_entries.is_empty() {
        extras.insert("CR".to_string(), cr_entries.iter().map(|s| s.to_string()).collect());
    }
    RisRecord {
        title: Some(title.to_string()),
        abstract_text: Some("Abstract text".to_string()),
        authors: vec!["Author A".to_string()],
        publication_year: Some(2023),
        doi: Some(format!("10.1234/{}", title.to_lowercase().replace(' ', "-"))),
        journal: Some("Test Journal".to_string()),
        reference_type: Some("JOUR".to_string()),
        extras,
        ..Default::default()
    }
}

#[test]
fn test_import_single_cr_reference() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let record = make_ris_record("Article One", vec!["Smith A, 2019, NATURE, V5, P100, 10.1/a"]);
    let new_article = ris_record_to_new_article(&record);
    let article = article_repo::insert_article(&conn, &new_article).expect("insert article failed");

    let errors = extract_cr_for_imported(&conn, std::slice::from_ref(&article), &[&record]);
    assert!(errors.is_empty(), "No errors expected");

    // Should have 1 reference paper
    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].paper.doi.as_deref(), Some("10.1/a"));

    // Should have audit entry
    let audit = audit_repo::get_audit_trail(&conn, &article.id).expect("get audit failed");
    let ref_import_entry = audit.iter().find(|e| e.action == AuditAction::ReferenceImport);
    assert!(ref_import_entry.is_some(), "Should have reference_import audit entry");
}

#[test]
fn test_import_multiple_cr_references() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let record = make_ris_record(
        "Survey Article",
        vec![
            "Smith A, 2019, NATURE, V5, P100, 10.1/a",
            "Jones B, 2021, SCIENCE, V2, P50, 10.2/b",
            "Lee C, 2022, CELL, V3, P200, 10.3/c",
        ],
    );
    let new_article = ris_record_to_new_article(&record);
    let article = article_repo::insert_article(&conn, &new_article).expect("insert article failed");

    let errors = extract_cr_for_imported(&conn, std::slice::from_ref(&article), &[&record]);
    assert!(errors.is_empty());

    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert_eq!(refs.len(), 3, "Should have 3 reference papers");
}

#[test]
fn test_import_overlapping_cr_same_doi() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Article 1 references paper with DOI 10.1/shared
    let record1 =
        make_ris_record("Article 1", vec!["Smith A, 2019, NATURE, V5, P100, 10.1/shared"]);
    let new_article1 = ris_record_to_new_article(&record1);
    let article1 =
        article_repo::insert_article(&conn, &new_article1).expect("insert article1 failed");

    extract_cr_for_imported(&conn, std::slice::from_ref(&article1), &[&record1]);

    // Article 2 also references the same paper (same DOI)
    let record2 =
        make_ris_record("Article 2", vec!["Smith A, 2019, NATURE, V5, P100, 10.1/shared"]);
    let new_article2 = ris_record_to_new_article(&record2);
    let article2 =
        article_repo::insert_article(&conn, &new_article2).expect("insert article2 failed");

    extract_cr_for_imported(&conn, std::slice::from_ref(&article2), &[&record2]);

    // Both articles should link to the same paper
    let refs1 = reference_repo::get_references_for_article(&conn, &article1.id, None)
        .expect("get refs1 failed");
    let refs2 = reference_repo::get_references_for_article(&conn, &article2.id, None)
        .expect("get refs2 failed");

    assert_eq!(refs1.len(), 1);
    assert_eq!(refs2.len(), 1);
    assert_eq!(
        refs1[0].paper.id, refs2[0].paper.id,
        "Both should point to the same deduplicated paper"
    );
}

#[test]
fn test_reimport_same_file_no_duplicate_papers() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let record =
        make_ris_record("Original Article", vec!["Smith A, 2019, NATURE, V5, P100, 10.1/unique"]);

    // First import
    let new_article1 = ris_record_to_new_article(&record);
    let article1 =
        article_repo::insert_article(&conn, &new_article1).expect("insert article1 failed");
    extract_cr_for_imported(&conn, std::slice::from_ref(&article1), &[&record]);

    // Re-import same file (different article record, same CR content)
    let new_article2 = ris_record_to_new_article(&record);
    let article2 =
        article_repo::insert_article(&conn, &new_article2).expect("insert article2 failed");
    extract_cr_for_imported(&conn, std::slice::from_ref(&article2), &[&record]);

    // Both should reference the same single paper
    let refs1 = reference_repo::get_references_for_article(&conn, &article1.id, None)
        .expect("get refs1 failed");
    let refs2 = reference_repo::get_references_for_article(&conn, &article2.id, None)
        .expect("get refs2 failed");

    assert_eq!(refs1[0].paper.id, refs2[0].paper.id);
}

#[test]
fn test_import_no_cr_field() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Record with no CR entries
    let record = make_ris_record("No CR Article", vec![]);
    let new_article = ris_record_to_new_article(&record);
    let article = article_repo::insert_article(&conn, &new_article).expect("insert article failed");

    let errors = extract_cr_for_imported(&conn, std::slice::from_ref(&article), &[&record]);
    assert!(errors.is_empty());

    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert!(refs.is_empty(), "No CR → no references");
}

#[test]
fn test_import_malformed_cr_graceful() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let record = make_ris_record("Bad CR Article", vec!["totally malformed, garbage entry"]);
    let new_article = ris_record_to_new_article(&record);
    let article = article_repo::insert_article(&conn, &new_article).expect("insert article failed");

    let _errors = extract_cr_for_imported(&conn, std::slice::from_ref(&article), &[&record]);
    // Should not panic, may or may not produce errors
    // The paper should still be created with the raw text as title
    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert_eq!(refs.len(), 1, "Should still create a paper from malformed CR");
}
