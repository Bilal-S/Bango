//! Integration tests for `wiki::fts` extracted from the inline `#[cfg(test)]`
//! block per `docs/CLAUDE.md` §Testing:
//! "Avoid large inline unit tests in library source files ... move them into
//! standalone integration test files under `src-tauri/tests/`."
//!
//! These tests exercise the FTS5 index build/search/rebuild, the two-tier
//! drift-detection helpers, and the T1.2 chunk-aware retrieval vertical slice.

use std::collections::HashMap;

use bango_lib::wiki::frontmatter;
use bango_lib::wiki::fts::{
    self, build_match_query, chunk_page_rows, collect_page_rows, collect_wiki_pages,
    compute_directory_fingerprint, ensure_index_populated, ensure_table, get_dir_hash,
    manifest_drifted, read_manifest, rebuild_index, rebuild_index_with_manifest, search,
    strip_table_placeholders, PageRow,
};
use rusqlite::Connection;
use tempfile::TempDir;

/// Write a wiki page with standard frontmatter under `<root>/wiki/<subdir>/<slug>.md`.
fn write_wiki_page(root: &std::path::Path, subdir: &str, slug: &str, title: &str, body: &str) {
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("slug", slug);
    fm.set("title", title);
    fm.set("type", "concept");
    fm.set("summary", &format!("{title} summary"));
    fm.set("status", "draft");
    fm.set("source_articles", "[]");
    fm.set("links", "[]");
    frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
}

/// Scaffold the `app_settings` + `wiki_index_manifest` tables the drift-detection
/// helpers read/write. Mirrors the v002 migration shape without depending on
/// the full migration suite.
fn scaffold_drift_tables(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE wiki_index_manifest (
             file_path TEXT PRIMARY KEY,
             content_hash TEXT NOT NULL
         );",
    )
    .unwrap();
}

// ── Rebuild + search basics ──────────────────────────────────────────

#[test]
fn rebuild_indexes_all_wiki_pages() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "concepts",
        "sugar-tax",
        "Sugar Tax",
        "# Sugar Tax\nA levy on sugary drinks.",
    );
    write_wiki_page(
        root,
        "concepts",
        "obesity",
        "Obesity",
        "# Obesity\nA major public health concern.",
    );
    write_wiki_page(
        root,
        "authors",
        "jane-doe",
        "Jane Doe",
        "# Jane Doe\nResearcher in nutrition.",
    );

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    let count = rebuild_index(&conn, root).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn search_returns_relevant_hits_by_bm25() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax beverage levy obesity");
    write_wiki_page(
        root,
        "concepts",
        "obesity",
        "Obesity",
        "obesity public health body mass index",
    );
    write_wiki_page(
        root,
        "authors",
        "jane-doe",
        "Jane Doe",
        "nutrition researcher sugar consumption",
    );

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "sugar", 5).unwrap();
    assert!(!hits.is_empty(), "should find results for 'sugar'");
    let slugs: Vec<&str> = hits.iter().map(|h| h.slug.as_str()).collect();
    assert!(slugs.contains(&"sugar-tax") || slugs.contains(&"jane-doe"));
}

#[test]
fn search_handles_empty_results_gracefully() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "zzznonexistent", 5).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_respects_limit() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    for i in 0..10 {
        write_wiki_page(
            root,
            "concepts",
            &format!("topic-{i}"),
            &format!("Topic {i}"),
            "common keyword shared",
        );
    }

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "common", 3).unwrap();
    assert!(hits.len() <= 3);
}

#[test]
fn search_escapes_quotes_in_query() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "test", "Test", "content with a quote here");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let result = search(&conn, "\"quoted phrase\"", 5);
    assert!(result.is_ok());
}

#[test]
fn rebuild_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    let c1 = rebuild_index(&conn, root).unwrap();
    let c2 = rebuild_index(&conn, root).unwrap();
    assert_eq!(c1, c2);
    let hits = search(&conn, "alpha", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

// ── ensure_index_populated self-heal ─────────────────────────────────

#[test]
fn ensure_index_populated_rebuilds_when_empty_but_pages_exist() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax levy");
    write_wiki_page(root, "authors", "jane-doe", "Jane Doe", "nutrition researcher");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    let hits_before = search(&conn, "sugar", 5).unwrap();
    assert!(hits_before.is_empty(), "index should be empty before self-heal");

    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(rebuilt, "should report a rebuild was performed");
    let hits_after = search(&conn, "sugar", 5).unwrap();
    assert!(!hits_after.is_empty(), "search should find results after self-heal");
    let slugs: Vec<&str> = hits_after.iter().map(|h| h.slug.as_str()).collect();
    assert!(slugs.contains(&"sugar-tax"));
}

#[test]
fn ensure_index_populated_returns_false_when_already_populated() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(!rebuilt, "should not rebuild an already-populated index");
}

#[test]
fn ensure_index_populated_returns_false_when_no_pages_exist() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("wiki")).unwrap();

    let conn = Connection::open_in_memory().unwrap();
    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(!rebuilt, "should not rebuild when there are no pages to index");
}

#[test]
fn ensure_index_populated_rebuilds_when_index_count_mismatches_disk() {
    // Reproduces the "J Adams on disk but not in Chat" staleness.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");
    write_wiki_page(root, "concepts", "beta", "Beta", "beta content");
    write_wiki_page(root, "authors", "jane-doe", "Jane Doe", "nutrition researcher");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();
    assert!(search(&conn, "jane", 5).unwrap().iter().any(|h| h.slug == "jane-doe"));

    write_wiki_page(root, "authors", "j-adams", "J Adams", "adams sugar policy");
    assert!(
        search(&conn, "adams", 5).unwrap().is_empty(),
        "new page should not be in the stale index"
    );

    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(rebuilt, "should rebuild on count mismatch");
    let hits = search(&conn, "adams", 5).unwrap();
    assert!(
        hits.iter().any(|h| h.slug == "j-adams"),
        "J Adams should be retrievable after self-heal"
    );
}

#[test]
fn ensure_index_populated_rebuilds_when_index_has_more_rows_than_disk() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");
    write_wiki_page(root, "concepts", "beta", "Beta", "beta content");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    std::fs::remove_file(root.join("wiki/concepts/beta.md")).unwrap();

    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(rebuilt, "should rebuild when index has stale extra rows");
    assert!(search(&conn, "beta", 5).unwrap().is_empty());
}

// ── Natural-language query handling ──────────────────────────────────

#[test]
fn search_finds_pages_by_any_token_in_natural_language_question() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(
        root,
        "authors",
        "author-adams-j",
        "J Adams",
        "J Adams appears across several SDIL studies on household purchases.",
    );
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "levy on sugary drinks");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "Who is J Adams?", 5).unwrap();
    assert!(!hits.is_empty(), "natural-language question must find pages");
    assert!(hits.iter().any(|h| h.slug == "author-adams-j"), "should find the J Adams page");
}

#[test]
fn build_match_query_drops_stop_words_and_or_joins_meaningful_tokens() {
    assert_eq!(build_match_query("Who is J Adams?"), "\"j\" OR \"adams\"");
}

#[test]
fn build_match_query_handles_punctuation_as_token_separator() {
    assert_eq!(build_match_query("What is the sugar-tax?"), "\"sugar\" OR \"tax\"");
}

#[test]
fn build_match_query_treats_fts_operators_as_literal_tokens() {
    let q = build_match_query("sugar AND tax OR levy*");
    assert_eq!(q, "\"sugar\" OR \"tax\" OR \"levy\"");
    assert!(!q.contains(" OR AND") && !q.contains(" OR OR"));
}

#[test]
fn build_match_query_falls_back_to_all_tokens_when_only_stop_words() {
    assert_eq!(build_match_query("the and is"), "\"the\" OR \"and\" OR \"is\"");
}

#[test]
fn build_match_query_returns_empty_when_no_alphanumeric_tokens() {
    assert_eq!(build_match_query("??? !!!"), "");
    assert_eq!(build_match_query(""), "");
}

#[test]
fn build_match_query_single_token_no_or() {
    assert_eq!(build_match_query("sugar"), "\"sugar\"");
    assert_eq!(build_match_query("the sugar"), "\"sugar\"");
}

#[test]
fn search_returns_empty_for_pure_stopword_question_without_erroring() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha content");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "the and is", 5).unwrap();
    assert!(hits.is_empty());

    let hits2 = search(&conn, "???", 5).unwrap();
    assert!(hits2.is_empty());
}

// ── collect_wiki_pages directory walker ──────────────────────────────

#[test]
fn collect_wiki_pages_lists_all_md_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
    write_wiki_page(root, "authors", "beta", "Beta", "b");

    let pages = collect_wiki_pages(root).unwrap();
    assert_eq!(pages.len(), 2);
}

#[test]
fn collect_wiki_pages_handles_missing_dir() {
    let tmp = TempDir::new().unwrap();
    let pages = collect_wiki_pages(tmp.path()).unwrap();
    assert!(pages.is_empty());
}

#[test]
fn collect_wiki_pages_excludes_top_level_log_and_index() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
    let wiki_dir = root.join("wiki");
    std::fs::write(wiki_dir.join("log.md"), "# Wiki Audit Log\n\nentry.").unwrap();
    std::fs::write(wiki_dir.join("index.md"), "# Wiki Index\n\n- [alpha](concepts/alpha.md)")
        .unwrap();

    let pages = collect_wiki_pages(root).unwrap();
    let stems: Vec<&str> =
        pages.iter().filter_map(|p| p.file_stem().and_then(|s| s.to_str())).collect();
    assert!(stems.contains(&"alpha"), "concept page should be listed");
    assert!(!stems.contains(&"log"), "top-level log.md must be excluded");
    assert!(!stems.contains(&"index"), "top-level index.md must be excluded");
    assert_eq!(pages.len(), 1, "only the concept page should be listed");
}

#[test]
fn collect_wiki_pages_keeps_subdirectory_log_or_index() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "a");
    write_wiki_page(root, "concepts", "log", "Log Concept", "a logging concept page");
    write_wiki_page(root, "authors", "index", "Index Author", "an author named Index");

    let pages = collect_wiki_pages(root).unwrap();
    let stems: Vec<&str> =
        pages.iter().filter_map(|p| p.file_stem().and_then(|s| s.to_str())).collect();
    assert!(stems.contains(&"alpha"));
    assert!(stems.contains(&"log"), "wiki/concepts/log.md must be listed");
    assert!(stems.contains(&"index"), "wiki/authors/index.md must be listed");
    assert_eq!(pages.len(), 3);
}

#[test]
fn rebuild_index_excludes_top_level_log_and_index() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax levy");
    std::fs::write(root.join("wiki/log.md"), "ingest run audit entry mentioning sugar").unwrap();

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    let count = rebuild_index(&conn, root).unwrap();
    assert_eq!(count, 1, "log.md must not be indexed");
    let hits = search(&conn, "sugar", 5).unwrap();
    assert!(hits.iter().all(|h| h.slug == "sugar-tax"), "log.md must not surface in search");
    assert_eq!(hits.len(), 1);
}

// ── Two-tier drift detection helpers ─────────────────────────────────

#[test]
fn compute_directory_fingerprint_none_for_empty() {
    let rows: Vec<PageRow> = Vec::new();
    assert!(compute_directory_fingerprint(&rows).is_none());
}

#[test]
fn compute_directory_fingerprint_changes_on_body_edit() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body v1");

    let rows1 = collect_page_rows(root).unwrap();
    let hash1 = compute_directory_fingerprint(&rows1).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body v2 with more words");

    let rows2 = collect_page_rows(root).unwrap();
    let hash2 = compute_directory_fingerprint(&rows2).unwrap();
    assert_ne!(hash1, hash2, "fingerprint must change when a file is edited");
}

#[test]
fn compute_directory_fingerprint_is_order_independent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "zebra", "Zebra", "z");
    write_wiki_page(root, "concepts", "alpha", "Alpha", "a");

    let mut rows = collect_page_rows(root).unwrap();
    let hash1 = compute_directory_fingerprint(&rows).unwrap();
    rows.reverse();
    let hash2 = compute_directory_fingerprint(&rows).unwrap();
    assert_eq!(hash1, hash2, "fingerprint must be order-independent (sorted by rel_path)");
}

#[test]
fn manifest_drifted_detects_content_change() {
    let mut stored = HashMap::new();
    stored.insert("wiki/concepts/alpha.md".to_string(), "hash-a-v1".to_string());
    stored.insert("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string());

    let disk_same = vec![
        ("wiki/concepts/alpha.md".to_string(), "hash-a-v1".to_string()),
        ("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string()),
    ];
    assert!(!manifest_drifted(&stored, &disk_same), "identical hashes -> no drift");

    let disk_changed = vec![
        ("wiki/concepts/alpha.md".to_string(), "hash-a-v2".to_string()),
        ("wiki/concepts/beta.md".to_string(), "hash-b-v1".to_string()),
    ];
    assert!(manifest_drifted(&stored, &disk_changed), "changed hash -> drift");
}

#[test]
fn manifest_drifted_detects_path_set_change() {
    let mut stored = HashMap::new();
    stored.insert("wiki/concepts/alpha.md".to_string(), "hash-a".to_string());

    let disk_added = vec![
        ("wiki/concepts/alpha.md".to_string(), "hash-a".to_string()),
        ("wiki/concepts/beta.md".to_string(), "hash-b".to_string()),
    ];
    assert!(manifest_drifted(&stored, &disk_added), "added file -> drift");

    let disk_removed: Vec<(String, String)> = Vec::new();
    assert!(manifest_drifted(&stored, &disk_removed), "removed file -> drift");
}

#[test]
fn rebuild_index_with_manifest_writes_manifest_and_dir_hash() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_wiki_page(root, "concepts", "alpha", "Alpha", "alpha body");
    write_wiki_page(root, "authors", "jane", "Jane", "jane body");

    let conn = Connection::open_in_memory().unwrap();
    scaffold_drift_tables(&conn);
    ensure_table(&conn).unwrap();

    let count = rebuild_index_with_manifest(&conn, root).unwrap();
    assert_eq!(count, 2);

    let manifest = read_manifest(&conn).unwrap();
    assert_eq!(manifest.len(), 2);
    assert!(get_dir_hash(&conn).is_some());

    let count2 = rebuild_index_with_manifest(&conn, root).unwrap();
    assert_eq!(count2, 2);
    let manifest2 = read_manifest(&conn).unwrap();
    assert_eq!(manifest2.len(), 2);
}

// ── T1.2 vertical-slice tests (chunk-aware retrieval) ────────────────

/// Write a wiki page with a long, section-structured body so
/// `collect_page_rows` -> `chunk_sections` produces multiple chunk rows.
fn write_long_sectioned_page(root: &std::path::Path, slug: &str, title: &str) {
    let methods_sentence = "This study employed a randomised controlled trial design \
        across multiple sites to evaluate the primary outcome measure with covariate \
        adjustment for baseline characteristics and sensitivity analyses."; // ~28 words
    let methods_body = methods_sentence.repeat(50); // ~1400 words
    let body = format!(
        "# {title}\n\n## Introduction\nIntro text here about the study context.\n\n\
         ## Methods\n{methods_body}\n\n## Results\nThe results showed a significant effect.\n\n\
         ## Discussion\nThese findings have important policy implications."
    );
    write_wiki_page(root, "sources", slug, title, &body);
}

#[test]
fn rebuild_index_emits_chunk_rows_for_long_sectioned_pages() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_long_sectioned_page(root, "smith-2023", "Smith 2023");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let mut stmt = conn
        .prepare("SELECT chunk_index, section, parent_slug FROM wiki_pages_fts WHERE chunk_index IS NOT NULL")
        .unwrap();
    let chunk_rows: Vec<(Option<i32>, Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    assert!(
        !chunk_rows.is_empty(),
        "long sectioned page must emit at least one chunk row with chunk_index IS NOT NULL"
    );
    assert!(
        chunk_rows.iter().any(|(_, section, _)| section.as_deref() == Some("Methods")),
        "at least one chunk must have section='Methods': {chunk_rows:?}"
    );
    assert!(
        chunk_rows.iter().all(|(_, _, ps)| ps.as_deref() == Some("smith-2023")),
        "all chunk rows must share parent_slug='smith-2023': {chunk_rows:?}"
    );
}

#[test]
fn ensure_index_populated_no_rebuild_when_chunk_rows_match_distinct_pages() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_long_sectioned_page(root, "smith-2023", "Smith 2023");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let fts_row_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM wiki_pages_fts", [], |r| r.get(0)).unwrap();
    assert!(fts_row_count > 1, "expected multiple chunk rows, got {fts_row_count}");

    let rebuilt = ensure_index_populated(&conn, root).unwrap();
    assert!(!rebuilt, "must not rebuild when chunk rows are consistent with disk pages");
}

#[test]
fn search_returns_section_label_for_chunk_rows() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_long_sectioned_page(root, "smith-2023", "Smith 2023");

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let hits = search(&conn, "randomised", 10).unwrap();
    assert!(!hits.is_empty(), "should find the Methods chunk");

    let has_methods_chunk =
        hits.iter().any(|h| h.section.as_deref() == Some("Methods") && h.chunk_index.is_some());
    assert!(
        has_methods_chunk,
        "at least one hit must have section=Some(\"Methods\") + chunk_index set: {:?}",
        hits.iter().map(|h| (&h.slug, &h.section, h.chunk_index)).collect::<Vec<_>>()
    );
}

// ── Tier 2 Phase 1: atomic GFM table handling ────────────────────────

#[test]
fn chunk_page_rows_emits_atomic_table_row_for_gfm_table_page() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let header = "| Metric | Value | Description |\n| --- | --- | --- |\n";
    let rows = (0..600)
        .map(|i| format!("| metric{i} | {i} | description of metric {i} number {i} |\n"))
        .collect::<String>();
    let table_body = format!("{header}{rows}");
    let body = format!("# Data Table\n\n{table_body}");
    write_wiki_page(root, "sources", "big-table", "Big Table", &body);

    let conn = Connection::open_in_memory().unwrap();
    ensure_table(&conn).unwrap();
    rebuild_index(&conn, root).unwrap();

    let mut stmt = conn
        .prepare("SELECT chunk_index, section, parent_slug, body FROM wiki_pages_fts WHERE section = 'Table'")
        .unwrap();
    let table_rows: Vec<(Option<i32>, Option<String>, Option<String>, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, String>(3)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(
        table_rows.len(),
        1,
        "GFM table must be ONE atomic FTS row, got {}: this is the Gap 1 regression",
        table_rows.len()
    );
    assert!(
        table_rows[0].3.contains("metric599"),
        "atomic table row must contain the full table body, got: {}...",
        &table_rows[0].3[..200.min(table_rows[0].3.len())]
    );
    let fragment_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM wiki_pages_fts WHERE body LIKE '%| --- |%' AND body NOT LIKE '%metric599%'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(fragment_count, 0, "no line-split table fragments should exist");
}

#[test]
fn strip_table_placeholders_removes_markers() {
    let text = "Some intro prose.\n\n<!-- TABLE:1 -->\n\nMore prose.\n\n<!-- TABLE:2 -->\n";
    let stripped = strip_table_placeholders(text);
    assert!(!stripped.contains("<!-- TABLE:"), "placeholders must be stripped: {stripped}");
    assert!(stripped.contains("Some intro prose."), "prose must survive: {stripped}");
    assert!(stripped.contains("More prose."), "prose must survive: {stripped}");
}

#[test]
fn rebuild_index_with_manifest_does_not_crash_on_long_chunked_pages() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_long_sectioned_page(root, "smith-2023", "Smith 2023");

    let conn = Connection::open_in_memory().unwrap();
    scaffold_drift_tables(&conn);
    ensure_table(&conn).unwrap();

    let count = rebuild_index_with_manifest(&conn, root).unwrap();
    assert!(count > 1, "chunked page should produce multiple FTS rows: {count}");

    let manifest = read_manifest(&conn).unwrap();
    assert_eq!(manifest.len(), 1, "manifest must have one row per file, not per chunk");
    assert!(
        manifest.keys().all(|k| k.contains("smith-2023")),
        "manifest key must be the file path: {manifest:?}"
    );

    let fts_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM wiki_pages_fts", [], |r| r.get(0)).unwrap();
    assert!(fts_rows > 1, "FTS should have multiple chunk rows: {fts_rows}");
    let distinct_parents: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT COALESCE(parent_slug, slug)) FROM wiki_pages_fts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(distinct_parents, 1, "all chunk rows share one parent slug");
}

// Silence the unused-import warning for `fts` when only re-exports are used.
#[allow(unused_imports)]
use fts as _;
// Silence the unused-import warning for `chunk_page_rows` (used indirectly by
// rebuild_index; kept for direct-call regression coverage if added later).
#[allow(unused_imports)]
use chunk_page_rows as _;
