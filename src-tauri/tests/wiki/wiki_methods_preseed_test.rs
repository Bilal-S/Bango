//! Integration tests for the method hub pre-seed (Tier A2).
//!
//! Covers both on-ramps:
//! - Primary: AI-summary `study_design` (when articles have section-aware
//!   summaries with a Methods section).
//! - Fallback: `biblio_terms` (abstracts-only corpora).
//!
//! Also covers the canonical study-design synonym map (e.g. "RCT" ->
//! "Randomized Controlled Trial") and the `status: reviewed` preservation.

use bango_lib::wiki::frontmatter;
use bango_lib::wiki::ingest;
use bango_lib::wiki::storage;
use rusqlite::Connection;
use tempfile::TempDir;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    bango_lib::db::migration::run_migrations(&conn).unwrap();
    conn
}

fn setup_root() -> std::path::PathBuf {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    storage::scaffold_tree(&root).unwrap();
    // Leak the TempDir so the test body can use the path (cleanup is fine to
    // skip; the OS reclaims temp on exit). Mirrors `wiki_deterministic_test.rs`.
    std::mem::forget(tmp);
    root
}

/// Insert an included article. When `ai_summary` is `Some`, it's stored as the
/// `full_text_ai_summary` JSON blob (the source for `study_design` extraction).
fn insert_article(conn: &Connection, id: &str, title: &str, ai_summary: Option<&str>) {
    conn.execute(
        "INSERT INTO articles (id, title, status, authors, publication_year, abstract_text, full_text_ai_summary) \
         VALUES (?1, ?2, 'included', '[]', 2021, 'Abstract.', ?3)",
        rusqlite::params![id, title, ai_summary],
    )
    .unwrap();
}

/// Insert a `biblio_terms` row + its `biblio_article_terms` link.
fn insert_term(
    conn: &Connection,
    term_id: &str,
    raw: &str,
    normalized: &str,
    article_id: &str,
    freq: i64,
) {
    conn.execute(
        "INSERT INTO biblio_terms (id, raw_term, normalized_term) VALUES (?1, ?2, ?3)",
        rusqlite::params![term_id, raw, normalized],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO biblio_article_terms (article_id, term_id, frequency) VALUES (?1, ?2, ?3)",
        rusqlite::params![article_id, term_id, freq],
    )
    .unwrap();
}

/// Build a v2 AI-summary JSON blob with a Methods section carrying `study_design`.
fn summary_with_study_design(design: &str) -> String {
    serde_json::json!({
        "summary_150_250_words": "A digest.",
        "section_summaries": [
            {
                "section": "Methods",
                "summary": "We describe the methods.",
                "key_points": [],
                "study_design": design,
            }
        ]
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Primary path: AI-summary study_design
// ---------------------------------------------------------------------------

#[test]
fn preseed_methods_writes_page_per_study_design() {
    let conn = test_db();
    let root = setup_root();

    insert_article(
        &conn,
        "art-1",
        "Paper One",
        Some(&summary_with_study_design("Randomized Controlled Trial")),
    );
    insert_article(
        &conn,
        "art-2",
        "Paper Two",
        Some(&summary_with_study_design("Difference-in-Differences")),
    );

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 2, "one page per distinct study design");

    // RCT page exists and carries both the canonical title + source_articles.
    let rct_path = root.join("wiki/methods/randomized-controlled-trial.md");
    assert!(rct_path.exists(), "RCT method page should exist");
    let (fm, body) = frontmatter::read_file(&rct_path).unwrap();
    assert_eq!(fm.get("type"), Some("method"));
    assert_eq!(fm.get("title"), Some("Randomized Controlled Trial"));
    assert_eq!(fm.get("slug"), Some("randomized-controlled-trial"));
    let sources = frontmatter::parse_list(fm.get("source_articles").unwrap_or(""));
    assert!(sources.contains(&"art-1".to_string()));
    assert!(body.contains("[[art-1]]"));

    // DiD page exists.
    let did_path = root.join("wiki/methods/difference-in-differences.md");
    assert!(did_path.exists(), "DiD method page should exist");
}

#[test]
fn preseed_methods_skips_when_no_summaries() {
    // No articles with AI summaries -> the summary path returns 0 rows. The
    // fallback (biblio_terms) also finds nothing here, so the result is 0.
    let conn = test_db();
    let root = setup_root();
    insert_article(&conn, "art-1", "Paper One", None);

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 0, "no summaries + no method terms -> 0 pages");
}

#[test]
fn preseed_methods_respects_reviewed() {
    let conn = test_db();
    let root = setup_root();
    insert_article(
        &conn,
        "art-1",
        "Paper One",
        Some(&summary_with_study_design("Randomized Controlled Trial")),
    );

    // Pre-create a reviewed method page.
    let path = root.join("wiki/methods/randomized-controlled-trial.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("status", "reviewed");
    fm.set("slug", "randomized-controlled-trial");
    frontmatter::write_file(&path, &fm, "# User edited").unwrap();

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 0, "reviewed method page should not be overwritten");
}

#[test]
fn preseed_methods_unions_articles_for_same_design() {
    let conn = test_db();
    let root = setup_root();
    // Two articles both using "RCT" (different surface forms that
    // canonicalize to the same design).
    insert_article(
        &conn,
        "art-1",
        "Paper One",
        Some(&summary_with_study_design("Randomized Controlled Trial")),
    );
    insert_article(&conn, "art-2", "Paper Two", Some(&summary_with_study_design("RCT")));

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 1, "two RCT variants should fold into one page");

    let path = root.join("wiki/methods/randomized-controlled-trial.md");
    let (fm, body) = frontmatter::read_file(&path).unwrap();
    let sources = frontmatter::parse_list(fm.get("source_articles").unwrap_or(""));
    assert!(sources.contains(&"art-1".to_string()));
    assert!(sources.contains(&"art-2".to_string()));
    assert!(body.contains("[[art-1]]"));
    assert!(body.contains("[[art-2]]"));
}

#[test]
fn preseed_methods_canonicalizes_rct_synonym() {
    // "RCT", "Randomised Controlled Trial" (British spelling), and
    // "Randomized Controlled Trial" all fold to the same canonical page.
    let conn = test_db();
    let root = setup_root();
    insert_article(&conn, "art-1", "A", Some(&summary_with_study_design("RCT")));
    insert_article(
        &conn,
        "art-2",
        "B",
        Some(&summary_with_study_design("Randomised Controlled Trial")),
    );
    insert_article(
        &conn,
        "art-3",
        "C",
        Some(&summary_with_study_design("Randomized Controlled Trial")),
    );

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 1, "all three variants canonicalize to one page");
    let path = root.join("wiki/methods/randomized-controlled-trial.md");
    assert!(path.exists());
    let (fm, _) = frontmatter::read_file(&path).unwrap();
    assert_eq!(fm.get("title"), Some("Randomized Controlled Trial"));
}

#[test]
fn preseed_methods_skips_unrecognized_study_design() {
    // A study_design that doesn't match the lexicon should not produce a page.
    let conn = test_db();
    let root = setup_root();
    insert_article(
        &conn,
        "art-1",
        "Paper One",
        Some(&summary_with_study_design("Totally Made Up Design")),
    );

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 0, "unrecognized study_design should be skipped");
}

// ---------------------------------------------------------------------------
// Fallback path: biblio_terms (abstracts-only corpora)
// ---------------------------------------------------------------------------

#[test]
fn preseed_methods_works_abstract_only_via_biblio_terms_fallback() {
    // No AI summaries -> the fallback mines `biblio_terms`. Only method-related
    // terms (per the lexicon) produce pages; non-method terms are filtered.
    let conn = test_db();
    let root = setup_root();
    insert_article(&conn, "art-1", "Paper One", None);
    insert_article(&conn, "art-2", "Paper Two", None);

    // Method-related term (matches the lexicon).
    insert_term(&conn, "t1", "RCT", "rct", "art-1", 3);
    // Non-method term (should be filtered out).
    insert_term(&conn, "t2", "Obesity", "obesity", "art-1", 5);
    insert_term(&conn, "t3", "Cohort", "cohort", "art-2", 2);

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 2, "only method-related terms should produce pages");

    let rct_path = root.join("wiki/methods/randomized-controlled-trial.md");
    assert!(rct_path.exists(), "RCT page should exist from biblio_terms fallback");
    let cohort_path = root.join("wiki/methods/cohort-study.md");
    assert!(cohort_path.exists(), "Cohort page should exist from biblio_terms fallback");

    // The non-method "obesity" term did NOT produce a method page.
    assert!(
        !root.join("wiki/methods/obesity.md").exists(),
        "non-method term should not produce a method page"
    );
}

// ---------------------------------------------------------------------------
// Provenance guard (Tier D3)
// ---------------------------------------------------------------------------

#[test]
fn preseed_methods_pages_carry_source_articles() {
    // Every pre-seeded method page must have non-empty source_articles
    // (the grounding contract; the grounding gate in A1 relies on this).
    let conn = test_db();
    let root = setup_root();
    insert_article(
        &conn,
        "art-1",
        "Paper One",
        Some(&summary_with_study_design("Randomized Controlled Trial")),
    );
    insert_article(
        &conn,
        "art-2",
        "Paper Two",
        Some(&summary_with_study_design("Systematic Review")),
    );

    let written = ingest::preseed_methods(&conn, &root, 25).unwrap();
    assert_eq!(written, 2);

    // Walk every method page and assert source_articles is non-empty.
    let methods_dir = root.join("wiki/methods");
    let mut checked = 0;
    for entry in std::fs::read_dir(&methods_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let (fm, _) = frontmatter::read_file(&path).unwrap();
        let sources = frontmatter::parse_list(fm.get("source_articles").unwrap_or(""));
        assert!(
            !sources.is_empty(),
            "method page {:?} must have non-empty source_articles",
            path.file_name()
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "should have checked both method pages");
}
