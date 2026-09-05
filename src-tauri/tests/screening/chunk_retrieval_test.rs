//! Integration tests for the Tier 3 chunk storage layer (`db::chunk_repo`)
//! and the `attach_full_text` -> chunk-population wiring.
//!
//! These cover the §T3.7 binding inventory for the repo round-trip + the
//! vertical slice that `attach_full_text` populates `article_chunks` with
//! contiguous `chunk_index` rows.

use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::utils::chunking::{chunk_sections, Chunk, DEFAULT_CHUNK_WORDS};
use bango_lib::utils::sections::{Section, SectionKind};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");
    conn
}

fn insert_article(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, 'Test Article', 'Author', 'Abstract text', 'working', 'test.ris')",
        rusqlite::params![id],
    )
    .expect("Insert article failed");
}

fn sample_chunks() -> Vec<Chunk> {
    let methods = Section {
        kind: SectionKind::Methods,
        heading: Some("## Methods".to_string()),
        body: (0..200).map(|i| format!("methodword{i}")).collect::<Vec<_>>().join(" "),
        word_count: 200,
    };
    let results = Section {
        kind: SectionKind::Results,
        heading: Some("## Results".to_string()),
        body: (0..200).map(|i| format!("resultword{i}")).collect::<Vec<_>>().join(" "),
        word_count: 200,
    };
    chunk_sections(&[methods, results], DEFAULT_CHUNK_WORDS)
}

// ── §T3.7 binding inventory: chunk_repo round-trip ──────────────────────

#[test]
fn chunk_repo_insert_list_roundtrip() {
    let conn = setup_db();
    insert_article(&conn, "art-1");
    let chunks = sample_chunks();
    let inserted = chunk_repo::replace_chunks_for_article(&conn, "art-1", &chunks).unwrap();
    assert_eq!(inserted, chunks.len());

    let listed = chunk_repo::list_chunks_for_article(&conn, "art-1").unwrap();
    assert_eq!(listed.len(), chunks.len(), "list returns same count as inserted");
    // chunk_index is contiguous 0..n.
    for (i, c) in listed.iter().enumerate() {
        assert_eq!(c.chunk_index, i, "contiguous chunk_index");
    }
    // Section labels survived the round-trip.
    assert!(listed.iter().any(|c| c.section.as_deref() == Some("Methods")));
    assert!(listed.iter().any(|c| c.section.as_deref() == Some("Results")));
}

#[test]
fn chunk_repo_delete_clears_article() {
    let conn = setup_db();
    insert_article(&conn, "art-2");
    let chunks = sample_chunks();
    chunk_repo::replace_chunks_for_article(&conn, "art-2", &chunks).unwrap();
    assert_eq!(chunk_repo::count_chunks_for_article(&conn, "art-2").unwrap(), chunks.len() as i64);

    chunk_repo::delete_chunks_for_article(&conn, "art-2").unwrap();
    assert_eq!(chunk_repo::count_chunks_for_article(&conn, "art-2").unwrap(), 0);
    assert!(chunk_repo::list_chunks_for_article(&conn, "art-2").unwrap().is_empty());
}

#[test]
fn chunk_repo_reinsert_replaces() {
    // Re-attach safety: calling replace twice for the same article does not
    // double-insert (DELETE-then-INSERT). Verified via count + UNIQUE constraint.
    let conn = setup_db();
    insert_article(&conn, "art-3");
    let chunks = sample_chunks();
    chunk_repo::replace_chunks_for_article(&conn, "art-3", &chunks).unwrap();
    // Insert a different set (one chunk).
    let single = vec![Chunk {
        chunk_index: 0,
        section: Some("Methods".to_string()),
        text: "only methods".to_string(),
        word_count: 2,
    }];
    chunk_repo::replace_chunks_for_article(&conn, "art-3", &single).unwrap();

    let listed = chunk_repo::list_chunks_for_article(&conn, "art-3").unwrap();
    assert_eq!(listed.len(), 1, "replace clears prior rows, no duplicates");
    assert_eq!(listed[0].text, "only methods");
}

#[test]
fn chunk_repo_missing_chunks_query_detects_un_chunked_articles() {
    let conn = setup_db();
    insert_article(&conn, "chunked");
    insert_article(&conn, "unchunked");
    // Mark both as having non-empty full text; seed chunks for `chunked` only.
    // (Non-empty `full_text` is required because the query excludes empty-text
    // articles - the soft-fallback attach path that produces them would
    // otherwise be retried on every screening run.)
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = 'body' WHERE id = 'chunked'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = 'body' WHERE id = 'unchunked'",
        [],
    )
    .unwrap();
    chunk_repo::replace_chunks_for_article(&conn, "chunked", &sample_chunks()).unwrap();

    let missing = chunk_repo::get_articles_with_full_text_missing_chunks(&conn).unwrap();
    assert_eq!(missing.len(), 1, "only the unchunked article is missing chunks");
    assert_eq!(missing[0], "unchunked");
}

#[test]
fn missing_chunks_query_excludes_empty_full_text_articles() {
    // Regression guard for the chunk-retry-spam fix: an article with
    // `has_full_text = 1` but NULL/empty `full_text` (the soft-fallback attach
    // path for corrupt PDFs) must NOT be returned, since re-parsing the same
    // invalid source would never produce chunks.
    let conn = setup_db();
    insert_article(&conn, "empty-ft");
    insert_article(&conn, "real-ft");
    // `empty-ft` mirrors the soft-fallback attach state: `has_full_text = 1`
    // but empty `full_text`.
    conn.execute("UPDATE articles SET has_full_text = 1, full_text = '' WHERE id = 'empty-ft'", [])
        .unwrap();
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = 'real body text' WHERE id = 'real-ft'",
        [],
    )
    .unwrap();

    let missing = chunk_repo::get_articles_with_full_text_missing_chunks(&conn).unwrap();
    assert!(
        !missing.contains(&"empty-ft".to_string()),
        "empty-full_text article must be excluded to prevent retry spam"
    );
    assert!(
        missing.contains(&"real-ft".to_string()),
        "non-empty article with no chunks is returned"
    );
}

#[test]
fn chunk_repo_count_articles_with_full_text() {
    let conn = setup_db();
    insert_article(&conn, "a1");
    insert_article(&conn, "a2");
    insert_article(&conn, "a3");
    // Only a1 and a2 have full text.
    conn.execute("UPDATE articles SET has_full_text = 1 WHERE id IN ('a1', 'a2')", []).unwrap();

    assert_eq!(chunk_repo::count_articles_with_full_text(&conn).unwrap(), 2);
}

/// Tier 3 Gap 4 regression: `get_articles_with_full_text` returns every article
/// with `has_full_text = 1` regardless of whether it already has chunks. The
/// "Rebuild text chunks" button relies on this so it can repair a corrupted /
/// partial / outdated chunk set, not just backfill empty ones. Contrast with
/// `get_articles_with_full_text_missing_chunks`, which the screening-start guard
/// uses to backfill only truly empty articles.
#[test]
fn get_articles_with_full_text_returns_all_regardless_of_chunks() {
    let conn = setup_db();
    insert_article(&conn, "with-chunks");
    insert_article(&conn, "without-chunks");
    insert_article(&conn, "no-fulltext");
    // Set non-empty `full_text` so the missing-chunks query considers them.
    conn.execute(
        "UPDATE articles SET has_full_text = 1, full_text = 'body' \
         WHERE id IN ('with-chunks', 'without-chunks')",
        [],
    )
    .unwrap();
    // Seed chunks for `with-chunks` only.
    chunk_repo::replace_chunks_for_article(&conn, "with-chunks", &sample_chunks()).unwrap();

    // `force=true` query: both full-text articles, including the chunked one.
    let all = chunk_repo::get_articles_with_full_text(&conn).unwrap();
    assert_eq!(all.len(), 2, "force=true returns both full-text articles");
    assert!(all.contains(&"with-chunks".to_string()));
    assert!(all.contains(&"without-chunks".to_string()));

    // `force=false` query: only the chunkless article (screening-start guard).
    let missing = chunk_repo::get_articles_with_full_text_missing_chunks(&conn).unwrap();
    assert_eq!(missing.len(), 1, "force=false returns only chunkless articles");
    assert_eq!(missing[0], "without-chunks");
}
