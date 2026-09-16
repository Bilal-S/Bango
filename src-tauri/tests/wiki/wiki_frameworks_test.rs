//! Integration tests for the frameworks pipeline (extraction schema,
//! canonicalization, deterministic pre-seed + LLM polish, article links).
//!
//! Root cause background: wiki input switched to AI summaries, which dropped
//! named-theory mentions, so framework pages lost grounding (three fresh runs
//! produced zero). See `.worktrees/wikifix-final.md` + wiki/AGENTS.md.
//!
//! Inventory: `docs/test-plans/wiki-frameworks-tests.md`.

use async_trait::async_trait;
use bango_lib::commands::summary::merge_frameworks_into_blob;
use bango_lib::commands::wiki_cmd::wiki_articles_missing_frameworks;
use bango_lib::db::article_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::wiki::ingest::frameworks::{
    apply_alias_merges, canonical_name_map, fetch_framework_rows, preseed_frameworks, FrameworkRow,
    FrameworkSynthesizer,
};
use bango_lib::wiki::ingest::synthesis::parse_ai_summary;
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Seed one included article carrying `blob` as its AI summary (+ full text).
fn seed_included_article(conn: &Connection, title: &str, blob: &str) -> String {
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
    conn.execute(
        "UPDATE articles SET full_text = 'body text', full_text_ai_summary = ?2 WHERE id = ?1",
        params![&inserted.id, blob],
    )
    .unwrap();
    inserted.id
}

struct FixedSynth(&'static str);

#[async_trait]
impl FrameworkSynthesizer for FixedSynth {
    async fn synthesize(&self, _row: &FrameworkRow) -> Result<String, AppError> {
        Ok(self.0.to_string())
    }
}

struct FailingSynth;

#[async_trait]
impl FrameworkSynthesizer for FailingSynth {
    async fn synthesize(&self, _row: &FrameworkRow) -> Result<String, AppError> {
        Err(AppError::Import("simulated failure".to_string()))
    }
}

fn read_page(root: &std::path::Path, slug: &str) -> String {
    std::fs::read_to_string(root.join("wiki/frameworks").join(format!("{slug}.md"))).unwrap()
}

#[tokio::test]
async fn frameworks_preseed_groups_articles_by_canonical_name() {
    let conn = test_db();
    let a1 = seed_included_article(
        &conn,
        "Paper One",
        r#"{"summary_150_250_words":"x","theoretical_frameworks":[{"name":"DSM-5","usage":"Classifies."}]}"#,
    );
    let a2 = seed_included_article(
        &conn,
        "Paper Two",
        r#"{"summary_150_250_words":"y","theoretical_frameworks":["Dsm 5"]}"#,
    );
    let a3 = seed_included_article(
        &conn,
        "Paper Three",
        r#"{"summary_150_250_words":"z","theoretical_frameworks":[{"name":"DSM-5","usage":"Diagnosis."}]}"#,
    );

    let rows = fetch_framework_rows(&conn).unwrap();
    assert_eq!(rows.len(), 1, "variant spellings must cluster: {rows:?}");
    assert_eq!(rows[0].slug, "dsm-5");
    assert_eq!(rows[0].name, "DSM-5", "most frequent variant wins");
    assert_eq!(rows[0].articles.len(), 3);

    let tmp = TempDir::new().unwrap();
    let written = preseed_frameworks(rows, tmp.path(), None).await.unwrap();
    assert_eq!(written, 1);
    let page = read_page(tmp.path(), "dsm-5");
    assert!(page.contains("## Publications Using This Framework"), "{page}");
    for id in [&a1, &a2, &a3] {
        assert!(page.contains(&format!("[[{id}|")), "missing link for {id}: {page}");
    }
    assert!(page.contains(&format!("source_articles: [\"{a1}\"")), "{page}");
}

#[tokio::test]
async fn framework_synthesis_writes_polished_page_with_publications_section() {
    let conn = test_db();
    seed_included_article(
        &conn,
        "Paper One",
        r#"{"theoretical_frameworks":[{"name":"TPB","usage":"Predicts intention."}]}"#,
    );
    seed_included_article(
        &conn,
        "Paper Two",
        r#"{"theoretical_frameworks":[{"name":"TPB","usage":"Extends to diet."}]}"#,
    );
    let rows = fetch_framework_rows(&conn).unwrap();

    let tmp = TempDir::new().unwrap();
    let synth = FixedSynth("## Core Tenets\n- Attitudes shape intention.");
    preseed_frameworks(rows, tmp.path(), Some(&synth)).await.unwrap();
    let page = read_page(tmp.path(), "tpb");
    assert!(page.contains("## Core Tenets"), "LLM body missing: {page}");
    // The deterministic Publications section is always complete.
    assert!(page.contains("## Publications Using This Framework"), "{page}");
    assert!(page.contains("[["), "{page}");
}

#[tokio::test]
async fn framework_synthesis_falls_back_to_skeleton_on_failure() {
    let conn = test_db();
    seed_included_article(
        &conn,
        "Paper One",
        r#"{"theoretical_frameworks":[{"name":"TPB","usage":"Predicts intention."}]}"#,
    );
    seed_included_article(
        &conn,
        "Paper Two",
        r#"{"theoretical_frameworks":[{"name":"TPB","usage":"Extends to diet."}]}"#,
    );
    let rows = fetch_framework_rows(&conn).unwrap();

    let tmp = TempDir::new().unwrap();
    let written = preseed_frameworks(rows, tmp.path(), Some(&FailingSynth)).await.unwrap();
    assert_eq!(written, 1, "skeleton page must still be written: {written}");
    let page = read_page(tmp.path(), "tpb");
    assert!(page.contains("named by 2 article(s)"), "skeleton body missing: {page}");
    assert!(page.contains("## Usage in This Review"), "{page}");
    assert!(page.contains("## Publications Using This Framework"), "{page}");
}

#[test]
fn backfill_query_targets_articles_missing_framework_field() {
    let conn = test_db();
    let with_field = seed_included_article(
        &conn,
        "Has Field",
        r#"{"summary_150_250_words":"x","theoretical_frameworks":[]}"#,
    );
    let missing = seed_included_article(&conn, "Missing Field", r#"{"summary_150_250_words":"y"}"#);
    let no_full_text = {
        // Blob without the field but no full text: extraction impossible.
        seed_included_article(&conn, "No Full Text", r#"{"summary_150_250_words":"z"}"#)
    };
    conn.execute("UPDATE articles SET full_text = NULL WHERE id = ?1", params![&no_full_text])
        .unwrap();

    let ids = wiki_articles_missing_frameworks(&conn).unwrap();
    assert_eq!(ids, vec![missing.clone()], "only the blob-less-field full-text article: {ids:?}");
    assert!(!ids.contains(&with_field));
}

#[test]
fn framework_extraction_schema_captures_usage_notes() {
    // Parser accepts both object entries and plain strings.
    let parsed = parse_ai_summary(
        r#"{"theoretical_frameworks":[{"name":"TPB","usage":"Predicts intention."},{"name":"COM-B"},"plain name"]}"#,
    )
    .unwrap();
    assert_eq!(parsed.theoretical_frameworks.len(), 3);
    assert_eq!(parsed.theoretical_frameworks[0].name, "TPB");
    assert_eq!(parsed.theoretical_frameworks[0].usage.as_deref(), Some("Predicts intention."));
    assert_eq!(parsed.theoretical_frameworks[1].usage, None);
    assert_eq!(parsed.theoretical_frameworks[2].name, "plain name");

    // Merge preserves every other blob key and stores empty arrays too.
    let merged = merge_frameworks_into_blob(
        r#"{"summary_150_250_words":"s","key_insights":["k"]}"#,
        r#"{"theoretical_frameworks":[]}"#,
    )
    .unwrap();
    assert!(merged.contains("\"theoretical_frameworks\""), "{merged}");
    assert!(merged.contains("\"key_insights\""), "{merged}");
    assert!(merged.contains("summary_150_250_words"), "{merged}");
}

#[test]
fn framework_alias_merges_unify_acronym_variants() {
    let names: Vec<String> =
        ["DSM-5", "DSM-5", "Diagnostic and Statistical Manual of Mental Disorders", "TPB"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    // Deterministic layer: slug clusters keep acronym + full title separate.
    let map = canonical_name_map(&names);
    assert_eq!(
        map.get("Diagnostic and Statistical Manual of Mental Disorders").map(String::as_str),
        Some("Diagnostic and Statistical Manual of Mental Disorders")
    );
    // LLM-merge layer: the alias merge unifies them under DSM-5.
    let merged = apply_alias_merges(
        &map,
        &[(
            "DSM-5".to_string(),
            vec!["Diagnostic and Statistical Manual of Mental Disorders".to_string()],
        )],
    );
    assert_eq!(
        merged.get("Diagnostic and Statistical Manual of Mental Disorders").map(String::as_str),
        Some("DSM-5")
    );
    assert_eq!(merged.get("DSM-5").map(String::as_str), Some("DSM-5"));
    assert_eq!(merged.get("TPB").map(String::as_str), Some("TPB"));
}

#[test]
fn synthesis_preseed_links_canonical_frameworks() {
    let conn = test_db();
    let id = seed_included_article(
        &conn,
        "Paper One",
        r#"{"summary_150_250_words":"digest","theoretical_frameworks":[{"name":"DSM-5","usage":"x"}]}"#,
    );
    let tmp = TempDir::new().unwrap();
    let written =
        bango_lib::wiki::ingest::preseed_synthesis_from_ai_summaries(&conn, tmp.path()).unwrap();
    assert_eq!(written, 1);
    let page = std::fs::read_to_string(tmp.path().join("wiki/synthesis").join(format!("{id}.md")))
        .unwrap();
    assert!(page.contains("## Theoretical Frameworks"), "{page}");
    assert!(page.contains("[[dsm-5|DSM-5]]"), "{page}");
}
