//! Tests for OpenAlex field mapping: abstract reconstruction, snippet truncation,
//! and work-to-article mapping.

use std::collections::HashMap;

use bango_lib::openalex::mapping;
use bango_lib::openalex::OpenAlexAuthor;
use bango_lib::openalex::OpenAlexAuthorship;
use bango_lib::openalex::OpenAlexBiblio;
use bango_lib::openalex::OpenAlexKeyword;
use bango_lib::openalex::OpenAlexOpenAccess;
use bango_lib::openalex::OpenAlexPrimaryLocation;
use bango_lib::openalex::OpenAlexSource;
use bango_lib::openalex::OpenAlexWork;

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
fn reconstruct_abstract_basic() {
    let mut index = HashMap::new();
    index.insert("The".to_string(), vec![0]);
    index.insert("impact".to_string(), vec![1]);
    index.insert("of".to_string(), vec![2]);
    index.insert("the".to_string(), vec![3]);
    index.insert("UK".to_string(), vec![4]);
    index.insert("Soft".to_string(), vec![5]);
    index.insert("Drinks".to_string(), vec![6]);
    index.insert("Industry".to_string(), vec![7]);
    index.insert("Levy".to_string(), vec![8]);
    let result = mapping::reconstruct_abstract(&Some(index));
    assert_eq!(result, "The impact of the UK Soft Drinks Industry Levy");
}

#[test]
fn reconstruct_abstract_empty() {
    let index: HashMap<String, Vec<i32>> = HashMap::new();
    let result = mapping::reconstruct_abstract(&Some(index));
    assert_eq!(result, "");
}

#[test]
fn reconstruct_abstract_null_index() {
    let result = mapping::reconstruct_abstract(&None);
    assert_eq!(result, "");
}

#[test]
fn truncate_snippet_word_boundary() {
    let abstract_text = "This is a long abstract ".repeat(11);
    let result = mapping::truncate_snippet(&abstract_text);
    assert!(result.chars().count() <= 203);
    assert!(result.ends_with("..."));
    let without_ellipsis = &result[..result.len() - 3];
    assert!(!without_ellipsis.ends_with(' '));
}

#[test]
fn truncate_snippet_under_200_no_ellipsis() {
    let abstract_text = "This is a short abstract.";
    let result = mapping::truncate_snippet(abstract_text);
    assert_eq!(result, abstract_text);
    assert!(!result.ends_with("..."));
}

#[test]
fn map_work_to_new_article_full() {
    let work = make_test_work();
    let article = mapping::map_work_to_new_article(&work);
    assert_eq!(
        article.title,
        "The impact of the UK Soft Drinks Industry Levy on childhood obesity"
    );
    assert!(!article.abstract_text.is_empty());
    assert_eq!(article.authors, vec!["Jane Smith", "John Doe"]);
    assert_eq!(article.publication_year, Some(2019));
    assert_eq!(article.date, Some("2019-06-01".to_string()));
    assert_eq!(article.doi, Some("10.1016/j.puhe.2018.04.012".to_string()));
    assert_eq!(article.journal, Some("Journal of Public Health".to_string()));
    assert_eq!(article.volume, Some("143".to_string()));
    assert_eq!(article.issue, Some("2".to_string()));
    assert_eq!(article.start_page, Some("89".to_string()));
    assert_eq!(article.end_page, Some("97".to_string()));
    assert_eq!(article.keywords, vec!["sugar tax", "obesity prevention"]);
    assert_eq!(article.language, Some("en".to_string()));
    assert_eq!(article.issn, Some("0022-3184".to_string()));
    assert_eq!(article.eissn, Some("1741-2854".to_string()));
    assert_eq!(article.num_cited, Some(142));
    assert_eq!(article.import_source, Some("openalex".to_string()));
    assert_eq!(article.reference_type, Some("JOUR".to_string()));
}

#[test]
fn map_work_to_new_article_minimal() {
    let work = OpenAlexWork {
        id: "https://openalex.org/W123".to_string(),
        doi: None,
        title: Some("Minimal Work".to_string()),
        publication_year: Some(2020),
        publication_date: None,
        authorships: vec![],
        primary_location: None,
        abstract_inverted_index: None,
        biblio: None,
        cited_by_count: 0,
        language: None,
        keywords: vec![],
        work_type: None,
        open_access: None,
        is_retracted: None,
        referenced_works: vec![],
    };
    let article = mapping::map_work_to_new_article(&work);
    assert_eq!(article.title, "Minimal Work");
    assert_eq!(article.abstract_text, "");
    assert!(article.authors.is_empty());
    assert!(article.keywords.is_empty());
    assert!(article.journal.is_none());
    assert!(article.doi.is_none());
    assert!(article.volume.is_none());
    assert!(article.issn.is_none());
    assert!(article.eissn.is_none());
    assert!(article.reference_type.is_none());
}

#[test]
fn map_work_strips_doi_prefix() {
    let work = OpenAlexWork {
        id: "W1".to_string(),
        doi: Some("https://doi.org/10.1234/ABC".to_string()),
        title: Some("Test".to_string()),
        publication_year: None,
        publication_date: None,
        authorships: vec![],
        primary_location: None,
        abstract_inverted_index: None,
        biblio: None,
        cited_by_count: 0,
        language: None,
        keywords: vec![],
        work_type: None,
        open_access: None,
        is_retracted: None,
        referenced_works: vec![],
    };
    let article = mapping::map_work_to_new_article(&work);
    assert_eq!(article.doi, Some("10.1234/abc".to_string()));
}

#[test]
fn map_work_publication_date_to_date_column() {
    let work = OpenAlexWork {
        id: "W1".to_string(),
        doi: None,
        title: Some("Test".to_string()),
        publication_year: Some(2023),
        publication_date: Some("2023-05-15".to_string()),
        authorships: vec![],
        primary_location: None,
        abstract_inverted_index: None,
        biblio: None,
        cited_by_count: 0,
        language: None,
        keywords: vec![],
        work_type: None,
        open_access: None,
        is_retracted: None,
        referenced_works: vec![],
    };
    let article = mapping::map_work_to_new_article(&work);
    assert_eq!(article.date, Some("2023-05-15".to_string()));
}

#[test]
fn map_work_eissn_differs_from_issn_l() {
    let work = OpenAlexWork {
        id: "W1".to_string(),
        doi: None,
        title: Some("Test".to_string()),
        publication_year: None,
        publication_date: None,
        authorships: vec![],
        primary_location: Some(OpenAlexPrimaryLocation {
            source: Some(OpenAlexSource {
                display_name: Some("Test Journal".to_string()),
                issn_l: Some("1234-5678".to_string()),
                issn: Some(vec!["1234-5678".to_string(), "8765-4321".to_string()]),
            }),
            landing_page_url: None,
            pdf_url: None,
        }),
        abstract_inverted_index: None,
        biblio: None,
        cited_by_count: 0,
        language: None,
        keywords: vec![],
        work_type: None,
        open_access: None,
        is_retracted: None,
        referenced_works: vec![],
    };
    let article = mapping::map_work_to_new_article(&work);
    assert_eq!(article.issn, Some("1234-5678".to_string()));
    assert_eq!(article.eissn, Some("8765-4321".to_string()));
}
