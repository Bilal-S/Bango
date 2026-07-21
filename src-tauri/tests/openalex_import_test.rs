//! Tests for the OpenAlex import pipeline against an in-memory SQLite database.

use std::collections::HashMap;

use bango_lib::db::article_repo;
use bango_lib::db::migration;
use bango_lib::openalex::mapping;
use bango_lib::openalex::OpenAlexAuthor;
use bango_lib::openalex::OpenAlexAuthorship;
use bango_lib::openalex::OpenAlexBiblio;
use bango_lib::openalex::OpenAlexKeyword;
use bango_lib::openalex::OpenAlexOpenAccess;
use bango_lib::openalex::OpenAlexPrimaryLocation;
use bango_lib::openalex::OpenAlexSource;
use bango_lib::openalex::OpenAlexWork;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    migration::run_migrations(&conn).unwrap();
    conn
}

fn make_test_work() -> OpenAlexWork {
    let mut inverted_index = HashMap::new();
    inverted_index.insert("The".to_string(), vec![0]);
    inverted_index.insert("impact".to_string(), vec![1]);
    inverted_index.insert("of".to_string(), vec![2]);
    inverted_index.insert("the".to_string(), vec![3]);
    inverted_index.insert("UK".to_string(), vec![4]);
    inverted_index.insert("Soft".to_string(), vec![5]);
    inverted_index.insert("Drinks".to_string(), vec![6]);
    inverted_index.insert("Industry".to_string(), vec![7]);
    inverted_index.insert("Levy".to_string(), vec![8]);

    OpenAlexWork {
        id: "https://openalex.org/W2741809807".to_string(),
        doi: Some("https://doi.org/10.1016/j.puhe.2018.04.012".to_string()),
        title: Some(
            "The impact of the UK Soft Drinks Industry Levy on childhood obesity".to_string(),
        ),
        publication_year: Some(2019),
        publication_date: Some("2019-06-01".to_string()),
        authorships: vec![
            OpenAlexAuthorship {
                author_position: Some("first".to_string()),
                author: OpenAlexAuthor {
                    display_name: Some("Jane Smith".to_string()),
                    id: Some("https://openalex.org/A123".to_string()),
                },
                institutions: vec![],
            },
            OpenAlexAuthorship {
                author_position: Some("last".to_string()),
                author: OpenAlexAuthor {
                    display_name: Some("John Doe".to_string()),
                    id: Some("https://openalex.org/A456".to_string()),
                },
                institutions: vec![],
            },
        ],
        primary_location: Some(OpenAlexPrimaryLocation {
            source: Some(OpenAlexSource {
                display_name: Some("Journal of Public Health".to_string()),
                issn_l: Some("0022-3184".to_string()),
                issn: Some(vec!["0022-3184".to_string(), "1741-2854".to_string()]),
            }),
            landing_page_url: Some(
                "https://academic.oup.com/jpubhealth/article/143/2/89/...".to_string(),
            ),
            pdf_url: None,
        }),
        abstract_inverted_index: Some(inverted_index),
        biblio: Some(OpenAlexBiblio {
            volume: Some("143".to_string()),
            issue: Some("2".to_string()),
            first_page: Some("89".to_string()),
            last_page: Some("97".to_string()),
        }),
        cited_by_count: 142,
        language: Some("en".to_string()),
        keywords: vec![
            OpenAlexKeyword { display_name: "sugar tax".to_string(), score: Some(0.92) },
            OpenAlexKeyword { display_name: "obesity prevention".to_string(), score: Some(0.81) },
        ],
        work_type: Some("article".to_string()),
        open_access: Some(OpenAlexOpenAccess {
            is_oa: Some(true),
            oa_status: Some("green".to_string()),
            oa_url: Some("https://ora.ox.ac.uk/objects/uuid:...".to_string()),
        }),
        is_retracted: Some(false),
        referenced_works: vec![],
    }
}

#[test]
fn import_single_openalex_article() {
    let conn = setup_db();
    let work = make_test_work();
    let new_article = mapping::map_work_to_new_article(&work);
    let imported = article_repo::insert_articles_batch(&conn, &[new_article], "openalex").unwrap();

    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].import_source, Some("openalex".to_string()));
    assert_eq!(
        imported[0].title,
        "The impact of the UK Soft Drinks Industry Levy on childhood obesity"
    );
}

#[test]
fn import_openalex_runs_dedup_classify() {
    let conn = setup_db();
    let work = make_test_work();
    let new_article = mapping::map_work_to_new_article(&work);
    let imported = article_repo::insert_articles_batch(&conn, &[new_article], "openalex").unwrap();

    assert_eq!(imported[0].status.as_str(), "duplicate");

    let _ = bango_lib::commands::dedup::classify_imported_articles(&conn, &imported).unwrap();

    let article = article_repo::get_article_by_id(&conn, &imported[0].id).unwrap();
    assert_eq!(article.status.as_str(), "working");
}

#[test]
fn import_openalex_audit_entry() {
    let conn = setup_db();
    let work = make_test_work();
    let new_article = mapping::map_work_to_new_article(&work);
    let imported = article_repo::insert_articles_batch(&conn, &[new_article], "openalex").unwrap();

    let audit_entries: Vec<(String, String)> = conn
        .prepare("SELECT action, details FROM audit_entries WHERE article_id = ?1")
        .unwrap()
        .query_map([&imported[0].id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert!(!audit_entries.is_empty());
    let import_entry = audit_entries.iter().find(|(action, _)| action == "import");
    assert!(import_entry.is_some(), "Expected an 'import' audit entry");
    assert!(import_entry.unwrap().1.contains("openalex"));
}

#[test]
fn import_openalex_duplicate_doi_skip() {
    let conn = setup_db();
    let work = make_test_work();
    let new_article = mapping::map_work_to_new_article(&work);

    let first =
        article_repo::insert_articles_batch(&conn, std::slice::from_ref(&new_article), "openalex")
            .unwrap();
    assert_eq!(first.len(), 1);

    let second = article_repo::insert_articles_batch(&conn, &[new_article], "openalex").unwrap();
    assert_eq!(second.len(), 1);

    let _ = bango_lib::commands::dedup::classify_imported_articles(&conn, &second).unwrap();
    let dup_article = article_repo::get_article_by_id(&conn, &second[0].id).unwrap();
    assert_eq!(dup_article.status.as_str(), "duplicate");
}

#[test]
fn check_dois_in_library_batch() {
    let conn = setup_db();

    let work = make_test_work();
    let new_article = mapping::map_work_to_new_article(&work);
    article_repo::insert_articles_batch(&conn, &[new_article], "openalex").unwrap();

    let dois_to_check =
        vec!["10.1016/j.puhe.2018.04.012".to_string(), "10.9999/not.in.library".to_string()];
    let found = article_repo::check_dois_in_library(&conn, &dois_to_check).unwrap();

    assert_eq!(found.len(), 1);
    assert!(found.contains(&"10.1016/j.puhe.2018.04.012".to_string()));
    assert!(!found.contains(&"10.9999/not.in.library".to_string()));
}

#[test]
#[ignore = "Tier 2: harvest_referenced_works_batch"]
fn harvest_referenced_works_batch() {
    // TODO: Tier 2 - test the reference-harvest batch-fetch pipeline.
}
