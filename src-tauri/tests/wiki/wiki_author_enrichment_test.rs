//! Integration tests for bibliometric author-page enrichment (co-authors,
//! most-cited, key references, curated keywords, main themes).
//!
//! Regression origin: `collect_coauthors` silently returned empty for every
//! author (rusqlite parameter-count error swallowed by `unwrap_or_default`),
//! so the Collaborators section never appeared on live pages.
//!
//! Inventory: `docs/test-plans/wiki-author-enrichment-tests.md`.

use bango_lib::db::article_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use bango_lib::wiki::ingest::authors::{
    build_author_manifest, render_author_page, AuthorManifest, AuthorManifestEntry,
};
use rusqlite::{params, Connection};

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

fn seed_included_article(conn: &Connection, title: &str) -> String {
    let article = NewArticle {
        title: title.to_string(),
        abstract_text: "Abstract.".to_string(),
        authors: vec!["Doe, Jane".to_string()],
        publication_year: Some(2022),
        keywords: vec!["wiki".to_string()],
        import_source: Some("test".to_string()),
        ..Default::default()
    };
    let inserted = article_repo::insert_article(conn, &article).unwrap();
    article_repo::update_article_status(conn, &inserted.id, "included").unwrap();
    inserted.id
}

/// Seed one canonical author linked to `article_ids`.
fn seed_author(conn: &Connection, display: &str, normalized: &str, article_ids: &[&str]) {
    let id = format!("auth-{normalized}");
    conn.execute(
        "INSERT INTO biblio_authors \
         (id, article_count, display_name, estimated_h_index, first_author_count, \
          normalized_name, total_citations) \
         VALUES (?1, ?2, ?3, 1, 0, ?4, 0)",
        params![id, article_ids.len() as i64, display, normalized],
    )
    .unwrap();
    for (i, article_id) in article_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO biblio_article_authors \
             (article_id, author_id, author_order, raw_name) VALUES (?1, ?2, ?3, ?4)",
            params![article_id, id, (i + 1) as i64, display],
        )
        .unwrap();
    }
}

fn seed_term(conn: &Connection, article_id: &str, raw: &str, normalized: &str, frequency: i64) {
    let term_id = format!("term-{normalized}");
    conn.execute(
        "INSERT OR IGNORE INTO biblio_terms \
         (id, article_count, normalized_term, raw_term, source, term_type) \
         VALUES (?1, 1, ?2, ?3, 'metadata', 'keyword')",
        params![term_id, normalized, raw],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO biblio_article_terms (article_id, frequency, term_id) VALUES (?1, ?2, ?3)",
        params![article_id, frequency, term_id],
    )
    .unwrap();
}

fn seed_reference(conn: &Connection, id: &str, title: &str, year: i64) {
    conn.execute(
        "INSERT INTO reference_papers (id, title, publication_year) VALUES (?1, ?2, ?3)",
        params![id, title, year],
    )
    .unwrap();
}

/// Link `paper_id` as a reference (type=1) used by `article_id`.
fn link_reference(conn: &Connection, article_id: &str, paper_id: &str) {
    conn.execute(
        "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type) \
         VALUES (?1, ?2, ?3, 1)",
        params![format!("link-{article_id}-{paper_id}"), article_id, paper_id],
    )
    .unwrap();
}

fn entry_for<'a>(manifest: &'a AuthorManifest, slug: &str) -> &'a AuthorManifestEntry {
    manifest.entries.iter().find(|e| e.slug == slug).unwrap()
}

/// Regression for the dead co-authorship query: co-authors must come back
/// from the biblio DB ranked by shared papers, and render as wikilinks.
#[test]
fn manifest_build_includes_coauthors_from_biblio_db() {
    let conn = test_db();
    let a1 = seed_included_article(&conn, "Paper One");
    let a2 = seed_included_article(&conn, "Paper Two");
    seed_author(&conn, "Doe, Jane", "doe j", &[&a1, &a2]);
    seed_author(&conn, "Smith, K", "smith k", &[&a1]);
    seed_author(&conn, "Jones, M", "jones m", &[&a1, &a2]);

    let manifest = build_author_manifest(&conn).unwrap();
    let doe = entry_for(&manifest, "author-doe-j");
    let slugs: Vec<&str> = doe.coauthors.iter().map(|c| c.slug.as_str()).collect();
    assert!(slugs.contains(&"author-jones-m"), "co-authors empty or wrong: {slugs:?}");
    assert!(slugs.contains(&"author-smith-k"), "co-authors empty or wrong: {slugs:?}");
    assert_eq!(doe.coauthors[0].slug, "author-jones-m", "ranked by shared papers: {slugs:?}");

    let (_fm, body) = render_author_page(doe);
    assert!(body.contains("## Frequent Collaborators"), "{body}");
    assert!(body.contains("[[author-jones-m]]"), "{body}");
}

/// Most Cited ranks the author's own papers by `articles.num_cited`.
#[test]
fn manifest_most_cited_uses_num_cited_ranking() {
    let conn = test_db();
    let top = seed_included_article(&conn, "Top Cited Paper");
    let mid = seed_included_article(&conn, "Mid Cited Paper");
    let zero = seed_included_article(&conn, "Zero Cited Paper");
    conn.execute("UPDATE articles SET num_cited = 50 WHERE id = ?1", params![&top]).unwrap();
    conn.execute("UPDATE articles SET num_cited = 5 WHERE id = ?1", params![&mid]).unwrap();
    seed_author(&conn, "Doe, Jane", "doe j", &[&top, &mid, &zero]);

    let manifest = build_author_manifest(&conn).unwrap();
    let doe = entry_for(&manifest, "author-doe-j");
    let (_fm, body) = render_author_page(doe);

    assert!(body.contains("## Most Cited"), "{body}");
    assert!(body.contains("\"Top Cited Paper\" (2022) - 50 citations"), "{body}");
    let cited_section = body.split("## Most Cited").nth(1).unwrap().split("## ").next().unwrap();
    assert!(!cited_section.contains("Zero Cited Paper"), "{cited_section}");
}

/// Key References ranks external reference papers by usage across the
/// author's articles.
#[test]
fn manifest_key_references_ranks_reference_papers() {
    let conn = test_db();
    let a1 = seed_included_article(&conn, "Paper One");
    let a2 = seed_included_article(&conn, "Paper Two");
    let a3 = seed_included_article(&conn, "Paper Three");
    seed_author(&conn, "Doe, Jane", "doe j", &[&a1, &a2, &a3]);
    seed_reference(&conn, "ref-hi", "Frequently Used Reference", 2020);
    seed_reference(&conn, "ref-lo", "Rarely Used Reference", 2015);
    for article_id in [&a1, &a2, &a3] {
        link_reference(&conn, article_id, "ref-hi");
    }
    link_reference(&conn, &a1, "ref-lo");

    let manifest = build_author_manifest(&conn).unwrap();
    let doe = entry_for(&manifest, "author-doe-j");
    assert_eq!(doe.references.len(), 2, "{:?}", doe.references);
    assert_eq!(doe.references[0].title, "Frequently Used Reference");
    assert_eq!(doe.references[0].times_used, 3);

    let (_fm, body) = render_author_page(doe);
    assert!(body.contains("## Key References"), "{body}");
    assert!(body.contains("\"Frequently Used Reference\" (2020) - used by 3"), "{body}");
}

/// Main Themes link to seeded concept hub pages derived from the author's
/// user-curated tags.
#[test]
fn manifest_main_themes_link_to_concept_hubs() {
    let conn = test_db();
    let a1 = seed_included_article(&conn, "Paper One");
    seed_author(&conn, "Doe, Jane", "doe j", &[&a1]);
    conn.execute(
        "INSERT INTO tags (id, color, name, source) \
         VALUES ('tag-1', '#000000', 'Sugar Tax', 'user_created')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO article_tags (article_id, tag_id) VALUES (?1, 'tag-1')",
        params![&a1],
    )
    .unwrap();

    let manifest = build_author_manifest(&conn).unwrap();
    let doe = entry_for(&manifest, "author-doe-j");
    assert!(
        doe.main_themes.iter().any(|t| t.slug == "sugar-tax"),
        "main themes missing sugar-tax: {:?}",
        doe.main_themes
    );
    let (_fm, body) = render_author_page(doe);
    assert!(body.contains("## Main Themes"), "{body}");
    assert!(body.contains("[[sugar-tax|Sugar Tax]]"), "{body}");
}

/// Research Areas drop blocklisted filler terms and keep the real ones.
#[test]
fn keyword_curation_filters_generic_terms() {
    let conn = test_db();
    let a1 = seed_included_article(&conn, "Paper One");
    seed_author(&conn, "Doe, Jane", "doe j", &[&a1]);
    seed_term(&conn, &a1, "upon", "upon", 10);
    seed_term(&conn, &a1, "significant", "significant", 9);
    seed_term(&conn, &a1, "years", "years", 8);
    seed_term(&conn, &a1, "HPLC", "hplc", 7);
    seed_term(&conn, &a1, "food dyes", "food dyes", 6);

    let manifest = build_author_manifest(&conn).unwrap();
    let doe = entry_for(&manifest, "author-doe-j");
    assert!(doe.keywords.contains(&"HPLC".to_string()), "{:?}", doe.keywords);
    assert!(doe.keywords.contains(&"food dyes".to_string()), "{:?}", doe.keywords);
    assert!(!doe.keywords.iter().any(|k| k.eq_ignore_ascii_case("upon")), "{:?}", doe.keywords);
    assert!(
        !doe.keywords.iter().any(|k| k.eq_ignore_ascii_case("significant")),
        "{:?}",
        doe.keywords
    );
    assert!(doe.keywords.len() <= 10, "{:?}", doe.keywords);

    let (_fm, body) = render_author_page(doe);
    assert!(body.contains("## Research Areas"), "{body}");
    let areas = body.split("## Research Areas").nth(1).unwrap().split("## ").next().unwrap();
    assert!(!areas.contains("upon"), "{areas}");
}
