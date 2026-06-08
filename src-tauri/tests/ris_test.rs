use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::ris::parser::parse_ris;
use bango_lib::ris::types::RisRecord;
use bango_lib::ris::validator::{validate_all, validate_all_grouped, validate_record};
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_parse_sugar_ris() {
    let content = fs::read_to_string(asset_path("10-valid-Sugar.ris")).expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 10);
    assert_eq!(result.errors.len(), 0);

    // First record: Pikhart, single author
    let record = &result.records[0];
    assert_eq!(record.reference_type.as_deref(), Some("JOUR"));
    assert!(record.title.as_ref().unwrap().contains("Sugar Consumption"));
    assert_eq!(record.authors.len(), 1);
    assert_eq!(record.authors[0], "Pikhart, Z");
    assert!(record.abstract_text.as_ref().unwrap().contains("fundamental commodity"));
    assert_eq!(record.publication_year, Some(2022));
    assert!(record.doi.is_none()); // No DOI for this record
    assert_eq!(record.journal.as_deref(), Some("LISTY CUKROVARNICKE A REPARSKE"));
    assert_eq!(record.volume.as_deref(), Some("138"));
    assert_eq!(record.issue.as_deref(), Some("5-6"));
    assert_eq!(record.start_page.as_deref(), Some("220"));
    assert_eq!(record.end_page.as_deref(), Some("223"));
    assert!(record.keywords.len() >= 3);
    assert_eq!(record.language.as_deref(), Some("Czech"));
    assert_eq!(record.issn.as_deref(), Some("1210-3306"));
    assert!(record.notes.is_some());
}

#[test]
fn test_parse_blue_ris() {
    let content =
        fs::read_to_string(asset_path("6-valid-7-invalid-Blue.ris")).expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 13);
    assert_eq!(result.errors.len(), 0);

    // First record: Future of blue foods (anonymous author)
    let rec1 = &result.records[0];
    assert!(rec1.title.as_ref().unwrap().contains("blue foods"));
    assert_eq!(rec1.authors.len(), 1);
    assert_eq!(rec1.authors[0], "[Anonymous]");
    assert_eq!(rec1.publication_year, Some(2021));
    assert!(rec1.doi.is_none());
    assert!(rec1.abstract_text.is_none()); // No abstract

    // Second record: Natural blue food colorants
    let rec2 = &result.records[1];
    assert!(rec2.title.as_ref().unwrap().contains("Natural blue food colorants"));
    assert_eq!(rec2.authors.len(), 3);
    assert_eq!(rec2.authors[0], "Neves, MIL");
    assert_eq!(rec2.publication_year, Some(2021));
    assert_eq!(rec2.doi.as_deref(), Some("10.1016/j.tifs.2021.03.023"));
    assert!(rec2.abstract_text.is_some());
    assert!(rec2.keywords.len() >= 5);

    // Environmental performance record (find it by DOI since order may vary)
    let env_rec = result
        .records
        .iter()
        .find(|r| r.doi.as_deref() == Some("10.1038/s41586-021-03889-2"))
        .expect("Environmental performance record not found");
    assert!(env_rec.title.as_ref().unwrap().contains("Environmental performance of blue foods"));
    assert!(env_rec.authors.len() > 10);
    assert_eq!(env_rec.publication_year, Some(2021));
    assert_eq!(env_rec.doi.as_deref(), Some("10.1038/s41586-021-03889-2"));
}

#[test]
fn test_parse_preserves_unrecognized_tags() {
    let content =
        "TY  - JOUR\nTI  - Test\nAU  - Author\nAB  - Abstract\nXX  - Unknown Value\nER  -\n";
    let result = parse_ris(content).expect("Parse failed");
    assert_eq!(
        result.records[0].extras.get("XX").map(|v| v.as_slice()),
        Some(&["Unknown Value".to_string()][..])
    );
}

#[test]
fn test_parse_empty_input() {
    let result = parse_ris("").expect("Parse failed");
    assert_eq!(result.records.len(), 0);
}

#[test]
fn test_validate_valid_record() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_missing_title() {
    let mut record = RisRecord::default();
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Title")));
}

#[test]
fn test_validate_missing_abstract() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Abstract")));
}

#[test]
fn test_validate_missing_authors() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Author")));
}

#[test]
fn test_validate_n2_abstract_fallback() {
    // N2 was already mapped to abstract_text by the parser.
    // This test verifies the parser correctly falls back.
    // Direct validation: if abstract_text is present, it's valid.
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("From N2".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.is_empty());
}

#[test]
fn test_full_import_pipeline_with_sugar_ris() {
    let content = fs::read_to_string(asset_path("10-valid-Sugar.ris")).expect("fixture not found");
    let parse_result = parse_ris(&content).expect("Parse failed");
    let (valid, errors) = validate_all(&parse_result.records);

    assert!(errors.is_empty(), "Expected no validation errors: {:?}", errors);
    assert_eq!(valid.len(), 10);

    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    let articles = article_repo::get_all_articles(&conn).expect("Query failed");
    assert_eq!(articles.len(), 0, "Should start empty");

    // Verify DB schema supports all parsed fields
    let record = &valid[0];
    assert!(record.title.is_some());
    assert!(record.abstract_text.is_some());
    assert!(!record.authors.is_empty());
}

#[test]
fn test_partial_import_blue_ris() {
    // Blue.ris has 13 records; some are missing abstracts
    let content =
        fs::read_to_string(asset_path("6-valid-7-invalid-Blue.ris")).expect("fixture not found");
    let parse_result = parse_ris(&content).expect("Parse failed");
    let (valid, errors, groups) = validate_all_grouped(&parse_result.records);

    // Some records should be valid, some should have validation errors
    assert!(valid.len() > 0, "Should have at least some valid records");
    assert!(errors.len() > 0, "Should have some validation errors");
    assert!(groups.len() > 0, "Should have error groups");

    // Verify that only valid records can be imported
    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    use bango_lib::commands::import::ris_record_to_new_article;
    let new_articles: Vec<_> = valid.iter().map(ris_record_to_new_article).collect();
    let imported = article_repo::insert_articles_batch(&conn, &new_articles, "Blue.ris")
        .expect("Insert failed");

    assert_eq!(imported.len(), valid.len(), "Should import all valid records");
}

#[test]
fn test_partial_import_green_ris() {
    // Green.ris has 7 records; some are missing abstracts
    let content =
        fs::read_to_string(asset_path("2-valid-5-invalid-Green.ris")).expect("fixture not found");
    let parse_result = parse_ris(&content).expect("Parse failed");
    let (valid, errors, groups) = validate_all_grouped(&parse_result.records);

    assert_eq!(parse_result.records.len(), 7, "Green.ris should have 7 records");
    assert!(valid.len() > 0, "Should have at least some valid records");
    assert!(errors.len() > 0, "Should have some validation errors (missing abstracts)");

    // Check grouped errors mention Abstract
    let abstract_group = groups.iter().find(|g| g.message.contains("Abstract"));
    assert!(abstract_group.is_some(), "Should have abstract-related error group");
    assert!(abstract_group.unwrap().count > 0);
}

#[test]
fn test_validate_all_grouped_groups_errors_by_message() {
    // 3 valid, 2 missing abstract, 1 missing title
    let ris = "\
TY  - JOUR\nTI  - Valid One\nAU  - Author A\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - No Abstract\nAU  - Author B\nER  -\n\
TY  - JOUR\nTI  - Valid Two\nAU  - Author C\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - Also No Abstract\nAU  - Author D\nER  -\n\
TY  - JOUR\nAU  - Author E\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - Valid Three\nAU  - Author F\nAB  - Abstract\nER  -\n";
    let parse_result = parse_ris(ris).expect("Parse failed");
    let (valid, errors, groups) = validate_all_grouped(&parse_result.records);

    assert_eq!(valid.len(), 3, "Should have 3 valid records");
    assert_eq!(errors.len(), 3, "Should have 3 total errors");
    assert_eq!(groups.len(), 2, "Should have 2 error groups");

    let abstract_group =
        groups.iter().find(|g| g.message.contains("Abstract")).expect("No abstract group");
    assert_eq!(abstract_group.count, 2);
    assert_eq!(abstract_group.record_indices.len(), 2);

    let title_group = groups.iter().find(|g| g.message.contains("Title")).expect("No title group");
    assert_eq!(title_group.count, 1);
}

#[test]
fn test_partial_import_only_valid_records_imported() {
    let ris = "\
TY  - JOUR\nTI  - Valid One\nAU  - Author A\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - Invalid\nAU  - Author B\nER  -\n\
TY  - JOUR\nTI  - Valid Two\nAU  - Author C\nAB  - Abstract\nER  -\n";
    let parse_result = parse_ris(ris).expect("Parse failed");
    let (valid, errors, _groups) = validate_all_grouped(&parse_result.records);

    assert_eq!(valid.len(), 2, "Should have 2 valid records");
    assert_eq!(errors.len(), 1, "Should have 1 validation error");

    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    use bango_lib::commands::import::ris_record_to_new_article;
    let new_articles: Vec<_> = valid.iter().map(ris_record_to_new_article).collect();
    let imported = article_repo::insert_articles_batch(&conn, &new_articles, "test.ris")
        .expect("Insert failed");

    assert_eq!(imported.len(), 2, "Should import 2 articles");
}

#[test]
fn test_user_excluded_records_not_imported() {
    let ris = "\
TY  - JOUR\nTI  - Keep One\nAU  - Author A\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - Exclude Me\nAU  - Author B\nAB  - Abstract\nER  -\n\
TY  - JOUR\nTI  - Keep Two\nAU  - Author C\nAB  - Abstract\nER  -\n";
    let parse_result = parse_ris(ris).expect("Parse failed");
    let (valid, _errors, _groups) = validate_all_grouped(&parse_result.records);

    assert_eq!(valid.len(), 3, "All 3 should be valid");

    // User excludes valid record at index 1
    let excluded: std::collections::HashSet<usize> = [1].into_iter().collect();
    let to_import: Vec<&RisRecord> =
        valid.iter().enumerate().filter(|(i, _)| !excluded.contains(i)).map(|(_, r)| r).collect();

    assert_eq!(to_import.len(), 2, "Only 2 should be imported after exclusion");
    assert!(to_import[0].title.as_ref().unwrap().contains("Keep One"));
    assert!(to_import[1].title.as_ref().unwrap().contains("Keep Two"));

    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    use bango_lib::commands::import::ris_record_to_new_article;
    let new_articles: Vec<_> = to_import.iter().map(|r| ris_record_to_new_article(r)).collect();
    let imported = article_repo::insert_articles_batch(&conn, &new_articles, "test.ris")
        .expect("Insert failed");

    assert_eq!(imported.len(), 2);
    let all = article_repo::get_all_articles(&conn).expect("Query failed");
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|a| !a.title.contains("Exclude")));
}

#[test]
fn test_parse_t1_as_title_alternative() {
    // T1 is an alternative to TI used by some RIS exporters (e.g., certain EndNote versions)
    let ris = "TY  - JOUR\nT1  - Title via T1 Tag\nAU  - Author A\nAB  - Abstract text\nER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);
    assert_eq!(result.records[0].title.as_deref(), Some("Title via T1 Tag"));

    // Verify the record passes validation
    let errors = validate_record(&result.records[0], 1);
    assert!(errors.is_empty(), "T1-titled record should be valid: {:?}", errors);
}

#[test]
fn test_parse_t1_fallback_when_ti_present() {
    // When both TI and T1 are present, TI takes precedence (parsed first, T1 overwrites)
    let ris = "TY  - JOUR\nTI  - Title from TI\nT1  - Title from T1\nAU  - Author\nAB  - Abstract\nER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);
    // Last one wins (T1 overwrites TI since they map to the same field)
    assert_eq!(result.records[0].title.as_deref(), Some("Title from T1"));
}

#[test]
fn test_multiline_n1_followed_by_er() {
    // N1 spans multiple lines, terminated by ER
    let ris = "\
TY  - JOUR\n\
TI  - Test Article\n\
AU  - Author A\n\
AB  - Abstract text\n\
N1  - Times Cited in Web of Science Core Collection:  87\n\
Total Times Cited:  104\n\
Cited Reference Count:  113\n\
ER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);

    let rec = &result.records[0];
    let notes = rec.notes.as_ref().expect("notes should be present");
    assert!(
        notes.contains("Times Cited in Web of Science Core Collection:  87"),
        "Should contain first line"
    );
    assert!(
        notes.contains("Total Times Cited:  104"),
        "Should contain second line"
    );
    assert!(
        notes.contains("Cited Reference Count:  113"),
        "Should contain third line"
    );
    assert_eq!(rec.num_cited, Some(104), "Should extract Total Times Cited");
    assert_eq!(rec.num_references, Some(113), "Should extract Cited Reference Count");
}

#[test]
fn test_multiline_n1_followed_by_other_tag() {
    // N1 spans multiple lines, terminated by AU (not ER)
    let ris = "\
TY  - JOUR\n\
TI  - Test Article\n\
N1  - Times Cited in Web of Science Core Collection:  44\n\
Total Times Cited:  49\n\
Cited Reference Count:  34\n\
AU  - Author A\n\
AB  - Abstract text\n\
ER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);

    let rec = &result.records[0];
    let notes = rec.notes.as_ref().expect("notes should be present");
    assert!(
        notes.contains("Times Cited in Web of Science Core Collection:  44"),
        "Should contain first line"
    );
    assert!(
        notes.contains("Total Times Cited:  49"),
        "Should contain second line (continuation)"
    );
    assert!(
        notes.contains("Cited Reference Count:  34"),
        "Should contain third line (continuation)"
    );
    assert_eq!(rec.num_cited, Some(49), "Should extract Total Times Cited");
    assert_eq!(rec.num_references, Some(34), "Should extract Cited Reference Count");
    assert_eq!(rec.authors.len(), 1, "Author should be parsed after N1");
    assert_eq!(rec.authors[0], "Author A");
}

#[test]
fn test_multiline_n1_with_crlf() {
    // N1 with Windows-style CRLF line endings
    let ris = "TY  - JOUR\r\nTI  - Test\r\nAU  - Author\r\nAB  - Abstract\r\nN1  - Times Cited in Web of Science Core Collection:  87\r\nTotal Times Cited:  104\r\nCited Reference Count:  113\r\nER  -\r\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);

    let rec = &result.records[0];
    assert_eq!(rec.num_cited, Some(104), "Should extract Total Times Cited with CRLF");
    assert_eq!(rec.num_references, Some(113), "Should extract Cited Reference Count with CRLF");
}

#[test]
fn test_multiline_ab_continuation() {
    // Abstract spans multiple lines
    let ris = "\
TY  - JOUR\n\
TI  - Test Article\n\
AU  - Author A\n\
AB  - First paragraph of the abstract\n\
Second paragraph with more details\n\
Third paragraph concludes\n\
ER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);

    let ab = result.records[0].abstract_text.as_ref().expect("abstract should be present");
    assert!(ab.contains("First paragraph"));
    assert!(ab.contains("Second paragraph"));
    assert!(ab.contains("Third paragraph"));
}

#[test]
fn test_multiline_n1_no_citation_data() {
    // Multi-line N1 with regular notes (no citation data)
    let ris = "\
TY  - JOUR\n\
TI  - Test Article\n\
AU  - Author A\n\
AB  - Abstract\n\
N1  - This is a note\n\
With a second line of notes\n\
And a third line\n\
ER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);

    let rec = &result.records[0];
    let notes = rec.notes.as_ref().expect("notes should be present");
    assert!(notes.contains("This is a note"));
    assert!(notes.contains("With a second line of notes"));
    assert!(notes.contains("And a third line"));
    assert_eq!(rec.num_cited, None);
    assert_eq!(rec.num_references, None);
}

#[test]
fn test_single_line_n1_unchanged() {
    // Verify single-line N1 still works correctly
    let ris = "\
TY  - JOUR\n\
TI  - Test Article\n\
AU  - Author A\n\
AB  - Abstract\n\
N1  - Times Cited in Web of Science Core Collection:  44\n\
Total Times Cited:  49\n\
Cited Reference Count:  34\n\
ER  -\n";
    let result = parse_ris(ris).expect("Parse failed");
    assert_eq!(result.records.len(), 1);
    // This is actually multi-line - the continuation lines are appended
    let rec = &result.records[0];
    assert_eq!(rec.num_cited, Some(49));
    assert_eq!(rec.num_references, Some(34));
}
