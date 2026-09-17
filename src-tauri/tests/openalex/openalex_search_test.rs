//! Tests for the OpenAlex search URL builder + DOI library check.

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::openalex::search::{
    build_doi_direct_url, build_request_url, build_search_url, extract_doi_query,
};
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
fn extract_doi_query_accepts_bare_url_and_scheme_forms() {
    assert_eq!(
        extract_doi_query("10.1016/j.puhe.2018.04.012"),
        Some("10.1016/j.puhe.2018.04.012".to_string())
    );
    assert_eq!(extract_doi_query("https://doi.org/10.1/AbC"), Some("10.1/abc".to_string()));
    assert_eq!(extract_doi_query("doi:10.1/x"), Some("10.1/x".to_string()));
    assert_eq!(extract_doi_query("  10.1/Spaced  "), Some("10.1/spaced".to_string()));
}

#[test]
fn extract_doi_query_rejects_plain_queries_and_placeholders() {
    assert_eq!(extract_doi_query("sugar tax"), None);
    // Starts with "10." but has no slash: not a bare DOI.
    assert_eq!(extract_doi_query("10.1016 and health"), None);
    assert_eq!(extract_doi_query("NA"), None);
    assert_eq!(extract_doi_query(""), None);
}

#[test]
fn build_doi_direct_url_bypasses_search_sort_and_filters() {
    let url = build_doi_direct_url("10.1/abc", 25, 1, "test@example.com", None);
    assert!(
        url.contains("filter=doi%3Ahttps%3A%2F%2Fdoi.org%2F10.1%2Fabc")
            || url.contains("filter=doi:https://doi.org/10.1/abc")
    );
    // The always-on keyword-search filters must not silently hide the exact
    // work (`is_retracted` still appears in `select=` - the badge needs it).
    assert!(!url.contains("search="));
    assert!(!url.contains("sort="));
    assert!(!url.contains("has_abstract"));
    assert!(!url.contains("is_retracted%3Afalse"));
    assert!(!url.contains("is_retracted:false"));
}

#[test]
fn build_request_url_routes_doi_direct_despite_active_filters() {
    let filters = OpenAlexFilters {
        year_from: Some(2018),
        year_to: Some(2024),
        work_types: vec!["article".to_string()],
        language: Some("en".to_string()),
        is_oa: true,
        show_retracted: false,
    };
    let url = build_request_url(
        "https://doi.org/10.1/old-paper",
        &filters,
        "relevance_score:desc",
        25,
        1,
        "t@example.com",
        None,
    );
    assert!(url.contains("filter=doi"));
    // `publication_year` in `select=` is fine; the filter form is not.
    assert!(
        !url.contains("publication_year%3A"),
        "year filter must not constrain a DOI-direct fetch"
    );
    assert!(!url.contains("publication_year:"));
    assert!(!url.contains("search="));
    assert!(!url.contains("sort="));

    // Plain queries keep the full keyword-search URL.
    let keyword_url = build_request_url(
        "sugar tax",
        &filters,
        "relevance_score:desc",
        25,
        1,
        "t@example.com",
        None,
    );
    assert!(keyword_url.contains("search=sugar+tax"));
    assert!(keyword_url.contains("publication_year"));
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
