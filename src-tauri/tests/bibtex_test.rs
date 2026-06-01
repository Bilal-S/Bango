use bango_lib::bibtex::converter::convert_bibtex_entries;
use bango_lib::bibtex::parser::parse_bibtex;
use bango_lib::commands::import::ris_record_to_new_article;
use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::ris::validator::validate_all_grouped;
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_parse_sugar_bibtex() {
    let content =
        fs::read_to_string(asset_path("8-valid-2-invalid-sugar.bibtex")).expect("fixture not found");
    let result = parse_bibtex(&content);

    assert_eq!(result.entries.len(), 10, "Should parse 10 entries");
    assert_eq!(result.errors.len(), 0, "Should have 0 parse errors, got: {:?}", result.errors);

    // All entries are articles
    for (i, entry) in result.entries.iter().enumerate() {
        assert_eq!(entry.entry_type, "article", "Entry {} should be article", i);
    }

    // Entry 6 (EBSCO quotes) — key 2854824120170701
    let ebsco_entry = result
        .entries
        .iter()
        .find(|e| e.key == "2854824120170701")
        .expect("EBSCO entry not found");
    let title_field: Option<&str> =
        ebsco_entry.fields.iter().find(|(k, _)| k == "title").map(|(_, v)| v.as_str());
    assert!(
        title_field.unwrap().contains('"'),
        "EBSCO title should contain literal quotes: {:?}",
        title_field
    );
    assert!(title_field.unwrap().contains("Making better use"));
}

#[test]
fn test_convert_sugar_bibtex() {
    let content =
        fs::read_to_string(asset_path("8-valid-2-invalid-sugar.bibtex")).expect("fixture not found");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);

    assert_eq!(records.len(), 10);

    // Entry 2 (Bossie, Andrew and Kuehn, Daniel) — multi-author
    let bossie = records.iter().find(|r| r.title.as_deref() == Some("WWII contract spending and inequality.")).expect("Bossie entry");
    assert_eq!(bossie.authors, vec!["Bossie, Andrew", "Kuehn, Daniel"]);
    assert_eq!(bossie.publication_year, Some(2021));
    assert_eq!(bossie.journal.as_deref(), Some("Applied Economics Letters"));
    assert_eq!(bossie.volume.as_deref(), Some("28"));
    assert_eq!(bossie.issue.as_deref(), Some("8"));
    assert_eq!(bossie.start_page.as_deref(), Some("635"));
    assert_eq!(bossie.end_page.as_deref(), Some("639"));
    assert_eq!(bossie.issn.as_deref(), Some("1350-4851"));
    assert!(bossie.keywords.len() >= 4, "Should have multiple keywords");

    // Entry 1 (Sweet Surprise) — ISSN with "; Print" suffix, pages "13-null"
    let sweet = records
        .iter()
        .find(|r| r.title.as_deref() == Some("Sweet Surprise: WWII sugar rationing boosted kids' health decades later."))
        .expect("Sweet entry");
    assert_eq!(sweet.issn.as_deref(), Some("0036-8733"), "ISSN should be cleaned");
    assert_eq!(sweet.start_page.as_deref(), Some("13"));
    assert_eq!(sweet.end_page, None, "null end page should be None");

    // Entry 5 (Glaesmer) — many authors, semicolon keywords
    let glaesmer = records
        .iter()
        .find(|r| r.title.as_deref().unwrap().contains("Childhood maltreatment in children born of occupation"))
        .expect("Glaesmer entry");
    assert!(glaesmer.authors.len() >= 4, "Should have 4+ authors");
    assert!(glaesmer.keywords.len() >= 10, "Should have many keywords from semicolons");

    // Entry 6 — EBSCO quotes in title preserved
    let ebsco = records
        .iter()
        .find(|r| r.title.as_deref().unwrap().contains("Making better use"))
        .expect("EBSCO entry");
    assert!(ebsco.title.as_ref().unwrap().contains('"'), "Title should have literal quotes");

    // Entry 8 — empty abstract, empty keywords
    let geophys = records
        .iter()
        .find(|r| r.title.as_deref() == Some("Geophysical investigations of WWII air-raid shelters in the UK."))
        .expect("Geophys entry");
    assert_eq!(geophys.abstract_text.as_deref(), Some(""), "Abstract should be empty string");
    assert!(geophys.keywords.is_empty(), "Empty keywords should produce no entries");

    // Entry 10 — Japanese Midwives
    let midwives = records
        .iter()
        .find(|r| r.title.as_deref().unwrap().contains("Japanese Midwives Association"))
        .expect("Midwives entry");
    assert_eq!(midwives.publication_year, Some(2021));
    assert_eq!(midwives.authors, vec!["Etsuko MATSUOKA"]);
}

#[test]
fn test_validate_sugar_bibtex() {
    let content =
        fs::read_to_string(asset_path("8-valid-2-invalid-sugar.bibtex")).expect("fixture not found");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);
    let (valid, errors, groups) = validate_all_grouped(&records);

    assert_eq!(valid.len(), 8, "Should have 8 valid records");
    assert_eq!(errors.len(), 2, "Should have 2 validation errors (missing abstracts)");

    // Error group should mention Abstract
    let abstract_group = groups.iter().find(|g| g.message.contains("Abstract"));
    assert!(abstract_group.is_some(), "Should have abstract-related error group");
    assert_eq!(abstract_group.unwrap().count, 2);

    // The two invalid records should be at indices 1 and 8 (Sweet Surprise and Geophysical)
    assert_eq!(errors.len(), 2, "Should have exactly 2 errors");
    let error_indices: Vec<usize> = errors.iter().map(|e| e.record_index).collect();
    assert!(
        error_indices.contains(&1),
        "Entry 1 (Sweet Surprise) should be invalid, got indices: {:?}",
        error_indices
    );
    assert!(
        error_indices.contains(&8),
        "Entry 8 (Geophysical) should be invalid, got indices: {:?}",
        error_indices
    );
    // All errors should mention Abstract
    for e in &errors {
        assert!(e.message.contains("Abstract"), "Error should mention Abstract: {}", e.message);
    }
}

#[test]
fn test_full_import_pipeline_sugar_bibtex() {
    let content =
        fs::read_to_string(asset_path("8-valid-2-invalid-sugar.bibtex")).expect("fixture not found");
    let parse_result = parse_bibtex(&content);
    assert_eq!(parse_result.entries.len(), 10);
    assert_eq!(parse_result.errors.len(), 0);

    let records = convert_bibtex_entries(&parse_result.entries);
    let (valid, errors, _groups) = validate_all_grouped(&records);

    assert_eq!(errors.len(), 2, "Should have 2 validation errors (missing abstracts)");
    assert_eq!(valid.len(), 8, "Should have 8 valid records");

    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    let articles = article_repo::get_all_articles(&conn).expect("Query failed");
    assert_eq!(articles.len(), 0, "Should start empty");

    let new_articles: Vec<_> = valid.iter().map(ris_record_to_new_article).collect();
    let imported =
        article_repo::insert_articles_batch(&conn, &new_articles, "8-valid-2-invalid-sugar.bibtex")
            .expect("Insert failed");

    assert_eq!(imported.len(), 8, "Should import 8 articles");

    // Verify the imported articles in DB
    let all = article_repo::get_all_articles(&conn).expect("Query failed");
    assert_eq!(all.len(), 8);

    // Spot-check: Bossie article should be present
    assert!(
        all.iter().any(|a| a.title.contains("WWII contract spending")),
        "Bossie article should be in DB"
    );
    // Sweet Surprise and Geophysical should NOT be present (no abstract)
    assert!(
        all.iter().all(|a| !a.title.contains("Sweet Surprise")),
        "Sweet Surprise should NOT be in DB"
    );
    assert!(
        all.iter().all(|a| !a.title.contains("Geophysical investigations")),
        "Geophysical should NOT be in DB"
    );
}

#[test]
fn test_bibtex_ebsco_quotes_preserved() {
    let content =
        fs::read_to_string(asset_path("8-valid-2-invalid-sugar.bibtex")).expect("fixture not found");
    let parse_result = parse_bibtex(&content);
    let records = convert_bibtex_entries(&parse_result.entries);

    let ebsco = records
        .iter()
        .find(|r| r.title.as_deref().unwrap().contains("Making better use"))
        .expect("EBSCO entry not found");

    // The title should contain literal quotes around "Making better use of U.S. women"
    assert!(ebsco.title.as_ref().unwrap().starts_with('"'), "Title should start with a quote");
    assert!(
        ebsco.title.as_ref().unwrap().contains("women\""),
        "Title should contain closing quote after women"
    );
    assert!(ebsco.abstract_text.is_some(), "Should have abstract");
    assert!(!ebsco.abstract_text.as_ref().unwrap().is_empty(), "Abstract should not be empty");
    assert_eq!(ebsco.publication_year, Some(2017));
    assert_eq!(ebsco.volume.as_deref(), Some("53"));
    assert_eq!(ebsco.issue.as_deref(), Some("3"));
    assert_eq!(ebsco.start_page.as_deref(), Some("228"));
    assert_eq!(ebsco.end_page.as_deref(), Some("245"));
}