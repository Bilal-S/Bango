//! Cluster thematic analysis tests (binding inventory mirrored in
//! `docs/test-plans/cluster-themes-tests.md`).
//!
//! Covers the pure helpers in `bango_lib::biblio::thematic`: prompt builder,
//! system prompt contract, Top-N cap + ranking, link protocol registry, and
//! the member-resolution dispatcher (author + three-source term resolution)
//! against an in-memory migrated SQLite database.

use bango_lib::biblio::normalizer::normalize_term;
use bango_lib::biblio::thematic::{
    apply_top_n_cap, build_cluster_themes_prompt, cluster_themes_system_prompt, link_protocols_for,
    resolve_members_to_articles, ClusterArticleSummary, ClusterMember, AUTHORS_MAX_CHARS,
    MAX_ARTICLES_PER_CLUSTER,
};
use bango_lib::db::biblio_repo::upsert_author;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::biblio::NetworkType;
use rusqlite::Connection;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn insert_article(conn: &Connection, id: &str, status: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, status) \
         VALUES (?1, ?2, 'Abstract text.', 'Smith J', ?3)",
        rusqlite::params![id, format!("Title {id}"), status],
    )
    .unwrap();
}

fn link_author(conn: &Connection, article_id: &str, author_id: &str) {
    conn.execute(
        "INSERT INTO biblio_article_authors (id, article_id, author_id, author_order) \
         VALUES (?1, ?2, ?3, 0)",
        rusqlite::params![format!("baa-{article_id}-{author_id}"), article_id, author_id],
    )
    .unwrap();
}

fn insert_biblio_term(conn: &Connection, id: &str, raw_term: &str) {
    conn.execute(
        "INSERT INTO biblio_terms (id, raw_term, normalized_term) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, raw_term, normalize_term(raw_term)],
    )
    .unwrap();
}

fn link_term(conn: &Connection, article_id: &str, term_id: &str) {
    conn.execute(
        "INSERT INTO biblio_article_terms (id, article_id, term_id, frequency) \
         VALUES (?1, ?2, ?3, 1)",
        rusqlite::params![format!("bat-{article_id}-{term_id}"), article_id, term_id],
    )
    .unwrap();
}

fn attach_tag(conn: &Connection, article_id: &str, tag_name: &str) {
    let tag_id = format!("tag-{article_id}");
    conn.execute(
        "INSERT OR IGNORE INTO tags (id, name, source) VALUES (?1, ?2, 'user_created')",
        rusqlite::params![tag_id, tag_name],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, tag_id],
    )
    .unwrap();
}

fn attach_label(conn: &Connection, article_id: &str, label_name: &str) {
    let label_id = format!("label-{article_id}");
    conn.execute(
        "INSERT OR IGNORE INTO labels (id, name, source) VALUES (?1, ?2, 'user_created')",
        rusqlite::params![label_id, label_name],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO article_labels (article_id, label_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, label_id],
    )
    .unwrap();
}

fn summary(id: &str, title: &str, year: i32, num_cited: Option<i32>) -> ClusterArticleSummary {
    ClusterArticleSummary {
        id: id.to_string(),
        title: title.to_string(),
        authors: "Smith J".to_string(),
        year: Some(year),
        abstract_text: Some("An abstract about sugar taxes.".to_string()),
        doi: Some(format!("10.1000/{id}")),
        keywords: Some("policy".to_string()),
        num_cited: num_cited.map(i64::from),
    }
}

fn members(ids: &[(&str, &str)]) -> Vec<ClusterMember> {
    ids.iter()
        .map(|(id, label)| ClusterMember { id: (*id).to_string(), label: (*label).to_string() })
        .collect()
}

// ── Prompt builder ────────────────────────────────────────────────────────

#[test]
fn build_cluster_themes_prompt_coauthorship_contains_members_and_articles() {
    let members = members(&[("auth-1", "Alice Smith"), ("auth-2", "Bob Jones")]);
    let articles = vec![summary("art-1", "Sugar tax policy effects", 2021, Some(12))];
    let prompt = build_cluster_themes_prompt(&NetworkType::CoAuthorship, 0, &members, &articles, 1);

    assert!(prompt.contains("Alice Smith"));
    assert!(prompt.contains("auth-1"));
    assert!(prompt.contains("Sugar tax policy effects"));
    assert!(prompt.contains("art-1"));
    assert!(prompt.contains("[Author Name](author:"));
    assert!(prompt.contains("[Article Title](article:"));
    assert!(prompt.contains("Authors: Smith J"));
    assert!(prompt.contains("Keywords: policy"));
}

#[test]
fn build_cluster_themes_prompt_keyword_contains_terms_and_articles() {
    let members = members(&[("sugar tax", "sugar tax"), ("obesity", "obesity")]);
    let articles = vec![summary("art-9", "Levy impact study", 2020, None)];
    let prompt = build_cluster_themes_prompt(&NetworkType::CoOccurrence, 2, &members, &articles, 1);

    assert!(prompt.contains("sugar tax"));
    assert!(prompt.contains("Levy impact study"));
    assert!(prompt.contains("[Article Title](article:"));
    // The keyword network has no author entities: no author protocol taught.
    assert!(!prompt.contains("author:"));
}

#[test]
fn build_cluster_themes_prompt_forbids_em_dash() {
    let members = members(&[("a", "A")]);
    let articles = vec![summary("art-1", "T", 2020, Some(1))];
    let user = build_cluster_themes_prompt(&NetworkType::CoAuthorship, 0, &members, &articles, 1);
    let system = cluster_themes_system_prompt();

    assert!(!user.contains('\u{2014}'));
    assert!(!system.contains('\u{2014}'));
}

#[test]
fn system_prompt_requires_thematic_sections() {
    let system = cluster_themes_system_prompt();
    assert!(system.contains("## Overview"));
    assert!(system.contains("## Main Themes"));
    assert!(system.contains("## Representative Articles"));
}

#[test]
fn build_cluster_themes_prompt_states_cap_when_truncated() {
    let members = members(&[("a", "A")]);
    let articles =
        vec![summary("art-1", "One", 2020, Some(9)), summary("art-2", "Two", 2021, Some(5))];

    let truncated =
        build_cluster_themes_prompt(&NetworkType::CoAuthorship, 0, &members, &articles, 5);
    assert!(truncated
        .contains("*Based on the 2 most representative of 5 included articles (ranked by citations, then recency).*"));

    // No disclosure when nothing was truncated.
    let full = build_cluster_themes_prompt(&NetworkType::CoAuthorship, 0, &members, &articles, 2);
    assert!(!full.contains("most representative of"));
}

#[test]
fn build_cluster_themes_prompt_truncates_authors_and_skips_empty_lines() {
    let mut long = summary("art-1", "Long author list", 2020, Some(1));
    long.authors = "Author ".repeat(200).trim().to_string();
    let prompt = build_cluster_themes_prompt(
        &NetworkType::CoAuthorship,
        0,
        &members(&[("a", "A")]),
        std::slice::from_ref(&long),
        1,
    );

    // The Authors: line truncates on a word boundary with an ellipsis and
    // stays within the cap.
    let rendered = prompt
        .lines()
        .find(|line| line.starts_with("  Authors:"))
        .map(|line| line.trim_start_matches("  Authors: "))
        .unwrap();
    assert!(rendered.ends_with("..."));
    assert!(rendered.chars().count() <= AUTHORS_MAX_CHARS);
    assert!(!rendered.trim_end_matches('.').ends_with(' '));

    // Empty authors and absent keywords render neither line.
    let mut bare = summary("art-2", "Bare article", 2021, Some(2));
    bare.authors = String::new();
    bare.keywords = None;
    let bare_prompt = build_cluster_themes_prompt(
        &NetworkType::CoAuthorship,
        0,
        &members(&[("a", "A")]),
        std::slice::from_ref(&bare),
        1,
    );
    assert!(!bare_prompt.contains("Authors:"));
    assert!(!bare_prompt.contains("Keywords:"));
}

// ── Resolution (in-memory DB) ─────────────────────────────────────────────

#[test]
fn resolve_authors_to_articles_returns_included_only() {
    let conn = test_db();
    insert_article(&conn, "art-in", "included");
    insert_article(&conn, "art-out", "rejected");
    let author = upsert_author(&conn, "alice", "Alice Smith").unwrap();
    link_author(&conn, "art-in", &author);
    link_author(&conn, "art-out", &author);

    let out =
        resolve_members_to_articles(&conn, &NetworkType::CoAuthorship, &[author.clone()]).unwrap();

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "art-in");
    assert_eq!(out[0].title, "Title art-in");
}

#[test]
fn resolve_terms_to_articles_returns_included_only() {
    let conn = test_db();
    insert_article(&conn, "art-in", "included");
    insert_article(&conn, "art-out", "working");
    insert_biblio_term(&conn, "term-1", "Sugar Tax");
    link_term(&conn, "art-in", "term-1");
    link_term(&conn, "art-out", "term-1");

    let id = normalize_term("Sugar Tax");
    let out = resolve_members_to_articles(&conn, &NetworkType::CoOccurrence, &[id]).unwrap();

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "art-in");
}

#[test]
fn resolve_terms_to_articles_dedupes_across_terms() {
    let conn = test_db();
    insert_article(&conn, "art-1", "included");
    insert_biblio_term(&conn, "term-1", "Sugar Tax");
    insert_biblio_term(&conn, "term-2", "Obesity");
    link_term(&conn, "art-1", "term-1");
    link_term(&conn, "art-1", "term-2");

    let ids = vec![normalize_term("Sugar Tax"), normalize_term("Obesity")];
    let out = resolve_members_to_articles(&conn, &NetworkType::CoOccurrence, &ids).unwrap();

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "art-1");
}

#[test]
fn resolve_terms_to_articles_matches_tags_and_labels_sources() {
    let conn = test_db();
    insert_article(&conn, "art-tag", "included");
    insert_article(&conn, "art-label", "included");
    attach_tag(&conn, "art-tag", "Childhood Obesity");
    attach_label(&conn, "art-label", "Priority Read");

    let ids = vec![normalize_term("Childhood Obesity"), normalize_term("Priority Read")];
    let out = resolve_members_to_articles(&conn, &NetworkType::CoOccurrence, &ids).unwrap();

    let mut got: Vec<&str> = out.iter().map(|a| a.id.as_str()).collect();
    got.sort_unstable();
    assert_eq!(got, vec!["art-label", "art-tag"]);
}

#[test]
fn resolve_articles_applies_top_n_cap_and_ranking() {
    let mut articles = Vec::new();
    for i in 0..45 {
        articles.push(summary(
            &format!("art-{i:02}"),
            &format!("Title {i}"),
            2010 + (i % 10),
            Some((i % 15) as i32),
        ));
    }
    // NULL citation count must rank last and fall out of the capped window.
    articles.push(summary("art-null", "Null cited", 2015, None));

    let (capped, total) = apply_top_n_cap(articles);

    assert_eq!(total, 46);
    assert_eq!(capped.len(), MAX_ARTICLES_PER_CLUSTER);
    assert_eq!(capped[0].num_cited, Some(14));
    assert!(!capped.iter().any(|a| a.id == "art-null"));
}

#[test]
fn resolve_members_dispatches_by_network_type() {
    let conn = test_db();
    insert_article(&conn, "art-author", "included");
    insert_article(&conn, "art-term", "included");
    let author = upsert_author(&conn, "bob", "Bob Jones").unwrap();
    link_author(&conn, "art-author", &author);
    insert_biblio_term(&conn, "term-1", "Machine Learning");
    link_term(&conn, "art-term", "term-1");

    let via_author =
        resolve_members_to_articles(&conn, &NetworkType::CoAuthorship, &[author]).unwrap();
    assert_eq!(via_author.len(), 1);
    assert_eq!(via_author[0].id, "art-author");

    let via_term = resolve_members_to_articles(
        &conn,
        &NetworkType::CoOccurrence,
        &[normalize_term("Machine Learning")],
    )
    .unwrap();
    assert_eq!(via_term.len(), 1);
    assert_eq!(via_term[0].id, "art-term");
}

#[test]
fn resolve_members_rejects_unsupported_network_type() {
    let conn = test_db();
    for network in [NetworkType::Citation, NetworkType::BiblioCoupling, NetworkType::CoCitation] {
        let err = resolve_members_to_articles(&conn, &network, &["x".to_string()]).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "expected Validation for {network}");
    }
}

#[test]
fn link_protocols_per_network_restrict_author_links() {
    let occurrence = link_protocols_for(&NetworkType::CoOccurrence);
    assert!(occurrence.iter().any(|p| p.prefix == "article"));
    assert!(!occurrence.iter().any(|p| p.prefix == "author"));

    let authorship = link_protocols_for(&NetworkType::CoAuthorship);
    assert!(authorship.iter().any(|p| p.prefix == "article"));
    assert!(authorship.iter().any(|p| p.prefix == "author"));
}
