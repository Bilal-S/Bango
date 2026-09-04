//! Tests for the OpenAlex search URL builder + DOI library check.

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::openalex::search::build_search_url;
use bango_lib::openalex::OpenAlexFilters;

#[test]
fn build_search_url_basic_query() {
    let filters = OpenAlexFilters::default();
    let url = build_search_url(
        "sugar tax",
        &filters,
        "relevance_score:desc",
        25,
        1,
        "test@example.com",
        None,
    );
    assert!(url.contains("search=sugar+tax"));
    assert!(url.contains("per_page=25"));
    assert!(url.contains("page=1"));
}

#[test]
fn build_search_url_has_abstract_always_on() {
    let filters = OpenAlexFilters::default();
    let url =
        build_search_url("test", &filters, "relevance_score:desc", 25, 1, "test@example.com", None);
    assert!(url.contains("has_abstract%3Atrue") || url.contains("has_abstract:true"));
}

#[test]
fn build_search_url_is_retracted_default_off() {
    let filters = OpenAlexFilters::default();
    let url =
        build_search_url("test", &filters, "relevance_score:desc", 25, 1, "test@example.com", None);
    assert!(url.contains("is_retracted%3Afalse") || url.contains("is_retracted:false"));
}

#[test]
fn build_search_url_url_encodes_query() {
    let filters = OpenAlexFilters::default();
    let url = build_search_url(
        "(sugar OR \"sugar-sweetened\") AND tax",
        &filters,
        "relevance_score:desc",
        25,
        1,
        "test@example.com",
        None,
    );
    assert!(!url.contains("(sugar OR"));
    assert!(url.contains("search="));
}

#[test]
fn build_search_url_with_filters() {
    let filters = OpenAlexFilters {
        year_from: Some(2018),
        year_to: Some(2024),
        work_types: vec!["article".to_string()],
        language: Some("en".to_string()),
        is_oa: true,
        show_retracted: false,
    };
    let url =
        build_search_url("test", &filters, "relevance_score:desc", 25, 1, "test@example.com", None);
    assert!(url.contains("publication_year"));
    assert!(url.contains("type"));
    assert!(url.contains("language"));
    assert!(url.contains("is_oa"));
}

#[test]
fn check_dois_in_library_case_insensitive() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");

    // Legacy mixed-case stored DOI (as written by pre-canonicalization builds).
    let article = NewArticle {
        title: "Mixed Case DOI".to_string(),
        doi: Some("10.1016/J.Puhe.2018.04.012".to_string()),
        ..Default::default()
    };
    article_repo::insert_article(&conn, &article).expect("insert article");

    // Canonical probe finds the mixed-case row; the returned value is lowercase
    // so exact `Set`/`HashSet` consumers (backend search, frontend store) match.
    let found = article_repo::check_dois_in_library(
        &conn,
        &["10.1016/j.puhe.2018.04.012".to_string(), "10.9999/absent".to_string()],
    )
    .expect("check dois");
    assert_eq!(found, vec!["10.1016/j.puhe.2018.04.012".to_string()]);

    // Prefixed probe also resolves: inputs are canonicalized server-side.
    let found_prefixed = article_repo::check_dois_in_library(
        &conn,
        &["https://doi.org/10.1016/j.puhe.2018.04.012".to_string()],
    )
    .expect("check dois prefixed");
    assert_eq!(found_prefixed, vec!["10.1016/j.puhe.2018.04.012".to_string()]);
}
