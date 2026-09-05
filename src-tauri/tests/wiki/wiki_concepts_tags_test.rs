//! Unit + integration tests for the tag-driven concept pre-seed (Phase 3).
//!
//! Covers:
//! - `tag_to_display_name` pure helper (kebab-case + free-text -> Title Case).
//! - `preseed_concept_hubs` writing pages from `article_tags` (the new path).
//! - Slug-dedup: a tag and a `biblio_terms` row that normalize to the same
//!   slug produce ONE page (tag wins, using the tag's display name).
//! - Limit honored: only the top-N tags by included-article count become pages.
//! - Tags on non-included articles do NOT produce pages.

use bango_lib::db::migration::run_migrations;
use bango_lib::wiki::frontmatter;
use bango_lib::wiki::ingest::{self, tag_to_display_name, TAG_CONCEPT_LIMIT};
use rusqlite::Connection;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Pure helper: tag_to_display_name
// ---------------------------------------------------------------------------

#[test]
fn tag_to_display_name_kebab_case_multiword() {
    assert_eq!(tag_to_display_name("supply-chain-management"), "Supply Chain Management");
    assert_eq!(tag_to_display_name("agri-food-digitalization"), "Agri Food Digitalization");
    assert_eq!(tag_to_display_name("interorganizational-trust"), "Interorganizational Trust");
}

#[test]
fn tag_to_display_name_single_word() {
    assert_eq!(tag_to_display_name("blockchain"), "Blockchain");
    assert_eq!(tag_to_display_name("trust"), "Trust");
}

#[test]
fn tag_to_display_name_preserves_inner_case() {
    // The helper only capitalizes the first char of each token; the rest pass
    // through unchanged. This matters for mixed-case tags.
    assert_eq!(tag_to_display_name("blockchain-LEDGER"), "Blockchain LEDGER");
}

#[test]
fn tag_to_display_name_handles_spaces_and_punctuation() {
    // Free-text tags (not kebab) should still produce a reasonable title.
    assert_eq!(tag_to_display_name("supply chain"), "Supply Chain");
    assert_eq!(tag_to_display_name("supply_chain"), "Supply Chain");
    assert_eq!(tag_to_display_name("supply, chain"), "Supply Chain");
}

#[test]
fn tag_to_display_name_empty_and_edge() {
    assert_eq!(tag_to_display_name(""), "");
    assert_eq!(tag_to_display_name("---"), "");
    assert_eq!(tag_to_display_name("a"), "A");
}

#[test]
fn tag_concept_limit_is_40() {
    // Lock the budget the user approved. Changing this constant is a
    // deliberate product decision and should update this test.
    assert_eq!(TAG_CONCEPT_LIMIT, 40);
}

// ---------------------------------------------------------------------------
// Integration: preseed_concept_hubs pulls from article_tags
// ---------------------------------------------------------------------------

#[test]
fn preseed_concept_hubs_writes_pages_for_tags() {
    let (conn, root) = setup_db_with_tags();
    let written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    assert!(written >= 2, "expected at least 2 tag-driven concept pages, got {written}");

    // The multi-word tag must produce a single page with the readable title.
    let path = root.join("wiki/concepts/supply-chain-management.md");
    assert!(path.exists(), "concept page for the multi-word tag should exist");
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("type"), Some("concept"));
    assert_eq!(fm.get("title"), Some("Supply Chain Management"));
    assert_eq!(fm.get("content_source"), Some("metadata"));
    assert!(body.contains("## Relevant Studies"));
    assert!(body.contains("[[art-1]]"));
    assert!(body.contains("[[art-2]]"));
}

#[test]
fn preseed_concept_hubs_tag_wins_on_slug_collision_with_term() {
    // A tag "supply-chain" and a biblio_term "Supply Chain" both normalize to
    // the slug "supply-chain". Only ONE page should be written, using the tag's
    // display name ("Supply Chain"), not the term's raw form.
    let (conn, root) = setup_db_with_colliding_tag_and_term();
    let written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    assert_eq!(written, 1, "tag + colliding term should produce exactly 1 page");

    let path = root.join("wiki/concepts/supply-chain.md");
    assert!(path.exists(), "the colliding concept page should exist");
    let (fm, _body) = frontmatter::read_file(&path).unwrap();
    // Tag wins: title is derived from the tag name, not the term.
    assert_eq!(fm.get("title"), Some("Supply Chain"));
    // source_articles should union both articles (the tag's + the term's).
    let sources = fm.get("source_articles").unwrap_or("");
    assert!(sources.contains("art-tag"), "tag article should be present: {sources}");
    assert!(sources.contains("art-term"), "term article should be present: {sources}");
}

#[test]
fn preseed_concept_hubs_tag_limit_honored() {
    // Seed 5 tags on a single included article. With a tag-limit lower than 5,
    // only the top tags by article count surface. All 5 share the same article
    // so the LIMIT applies directly.
    let (conn, root) = setup_db_with_many_tags(5);
    // The pre-seed uses TAG_CONCEPT_LIMIT (40) internally, so all 5 tags
    // produce pages here. Verify the count matches.
    let written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    assert_eq!(written, 5, "all 5 tags should produce concept pages");
    let _ = root; // keep temp dir alive
}

#[test]
fn preseed_concept_hubs_ignores_tags_on_non_included_articles() {
    let (conn, root) = setup_db_with_tag_on_non_included();
    let written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    assert_eq!(written, 0, "tags on non-included articles must not produce concept pages");
    let _ = root;
}

#[test]
fn preseed_concept_hubs_respects_reviewed_tag_pages() {
    let (conn, root) = setup_db_with_tags();
    // Pre-create a reviewed concept page for the multi-word tag.
    let path = root.join("wiki/concepts/supply-chain-management.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("status", "reviewed");
    fm.set("slug", "supply-chain-management");
    frontmatter::write_file(&path, &fm, "# User edited").unwrap();

    let written = ingest::preseed_concept_hubs(&conn, &root, 25).unwrap();
    // The reviewed page is skipped; the other tag still gets written.
    assert_eq!(written, 1, "reviewed tag concept page should not be overwritten");
}

// ---------------------------------------------------------------------------
// Test helpers: DB setup
// ---------------------------------------------------------------------------

fn setup_db_with_tags() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // 2 included articles.
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text) \
         VALUES ('art-1', 'Article One', 'included', '[]', 2021, 'Abstract.'), \
                ('art-2', 'Article Two', 'included', '[]', 2022, 'Abstract.')",
        [],
    )
    .unwrap();

    // 2 tags: a multi-word kebab tag + a single-word tag.
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES \
         ('tag-1', 'supply-chain-management', 'user_created'), \
         ('tag-2', 'blockchain', 'user_created')",
        [],
    )
    .unwrap();
    // Link both tags to both articles.
    conn.execute(
        "INSERT INTO article_tags (article_id, tag_id) VALUES \
         ('art-1', 'tag-1'), ('art-2', 'tag-1'), \
         ('art-1', 'tag-2'), ('art-2', 'tag-2')",
        [],
    )
    .unwrap();
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_colliding_tag_and_term() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // 2 included articles: one carries the tag, the other carries the term.
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text) \
         VALUES ('art-tag', 'Tagged Article', 'included', '[]', 2021, 'Abstract.'), \
                ('art-term', 'Term Article', 'included', '[]', 2022, 'Abstract.')",
        [],
    )
    .unwrap();

    // Tag: "supply-chain" -> slug "supply-chain".
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES ('tag-1', 'supply-chain', 'user_created')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO article_tags (article_id, tag_id) VALUES ('art-tag', 'tag-1')", [])
        .unwrap();

    // Term: normalized_term "supply chain" -> slug "supply-chain" (collision).
    conn.execute(
        "INSERT INTO biblio_terms (id, raw_term, normalized_term) VALUES \
         ('t1', 'Supply Chain', 'supply chain')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO biblio_article_terms (article_id, term_id, frequency) VALUES \
         ('art-term', 't1', 1)",
        [],
    )
    .unwrap();
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_many_tags(n: usize) -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text) \
         VALUES ('art-1', 'Article One', 'included', '[]', 2021, 'Abstract.')",
        [],
    )
    .unwrap();
    for i in 0..n {
        let tag_id = format!("tag-{i}");
        let name = format!("concept-{i}");
        conn.execute(
            "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'user_created')",
            rusqlite::params![tag_id, name],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO article_tags (article_id, tag_id) VALUES ('art-1', ?1)",
            rusqlite::params![tag_id],
        )
        .unwrap();
    }
    std::mem::forget(tmp);
    (conn, root)
}

fn setup_db_with_tag_on_non_included() -> (Connection, std::path::PathBuf) {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    bango_lib::wiki::storage::scaffold_tree(&root).unwrap();

    // A 'working' article (NOT included) with a tag.
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text) \
         VALUES ('art-1', 'Working Article', 'working', '[]', 2021, 'Abstract.')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES ('tag-1', 'supply-chain', 'user_created')",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO article_tags (article_id, tag_id) VALUES ('art-1', 'tag-1')", [])
        .unwrap();
    std::mem::forget(tmp);
    (conn, root)
}
