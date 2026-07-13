//! Tests for the OpenAlex search URL builder.

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
