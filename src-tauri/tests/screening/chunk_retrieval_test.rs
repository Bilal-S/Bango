//! Integration tests for the Tier 3 chunk storage layer (`db::chunk_repo`)
//! and the `attach_full_text` -> chunk-population wiring, plus the pure
//! retrieval-layer ranker (`rank_chunks_by_criteria`) extracted from the
//! inline `#[cfg(test)] mod tests` in `src/screening/chunk_retrieval.rs`.
//!
//! These cover the §T3.7 binding inventory for the repo round-trip + the
//! vertical slice that `attach_full_text` populates `article_chunks` with
//! contiguous `chunk_index` rows.

use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::screening::chunk_retrieval::{
    rank_chunks_by_criteria, DEFAULT_CHUNK_BUDGET_PER_ARTICLE, DEFAULT_MAX_CHUNK_WORDS,
    DEFAULT_TOP_K, METHODS_BOOST,
};
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

// ── rank_chunks_by_criteria (extracted from the inline tests) ──────────

fn chunk(index: usize, section: Option<&str>, text: &str) -> Chunk {
    Chunk {
        chunk_index: index,
        section: section.map(str::to_string),
        text: text.to_string(),
        word_count: text.split_whitespace().count(),
    }
}

fn criteria(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|s| s.to_string()).collect()
}

// ── §T3.7 inventory tests (binding) ───────────────────────────────

#[test]
fn rank_chunks_empty_chunks_returns_empty() {
    let inc = criteria(&["children obesity"]);
    let out = rank_chunks_by_criteria(
        &[],
        &inc,
        &[],
        DEFAULT_TOP_K,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert!(out.is_empty());
}

#[test]
fn rank_chunks_empty_criteria_returns_all_unscored() {
    // No criteria => every chunk ties at score 0.0. Returns up to top_k in
    // original order.
    let chunks = vec![
        chunk(0, Some("Methods"), "alpha beta gamma"),
        chunk(1, Some("Results"), "delta epsilon"),
    ];
    let out = rank_chunks_by_criteria(
        &chunks,
        &[],
        &[],
        5,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out.len(), 2, "all chunks returned when criteria empty");
    assert!(out.iter().all(|c| c.score == 0.0), "all tie at 0.0 with no criteria");
    assert_eq!(out[0].chunk_index, 0, "original order preserved");
    assert_eq!(out[1].chunk_index, 1);
}

#[test]
fn rank_chunks_methods_section_gets_boost() {
    // Two chunks with identical criteria overlap; the Methods one ranks first.
    let chunks = vec![
        chunk(0, Some("Text"), "sugar tax study design rct"),
        chunk(1, Some("Methods"), "sugar tax study design rct"),
    ];
    let inc = criteria(&["sugar tax rct"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out[0].section.as_deref(), Some("Methods"), "Methods chunk boosted to top");
    assert_eq!(out[1].section.as_deref(), Some("Text"));
    assert!(
        (out[0].score - out[1].score - METHODS_BOOST).abs() < 1e-9,
        "boost delta == METHODS_BOOST"
    );
}

#[test]
fn rank_chunks_respects_top_k() {
    let chunks: Vec<Chunk> =
        (0..5).map(|i| chunk(i, Some("Methods"), &format!("sugar tax {i}"))).collect();
    let inc = criteria(&["sugar tax"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out.len(), 2, "top_k=2 with 5 chunks -> 2 returned");
}

#[test]
fn rank_chunks_filters_oversized_chunks() {
    // chunk 0 is oversized (> MAX), chunk 1 is normal and matches.
    let big = format!("sugar {}", "word ".repeat(DEFAULT_MAX_CHUNK_WORDS + 10));
    let chunks = vec![chunk(0, Some("Methods"), &big), chunk(1, Some("Methods"), "sugar tax levy")];
    let inc = criteria(&["sugar tax"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out.len(), 1, "oversized chunk excluded");
    assert_eq!(out[0].chunk_index, 1, "only the normal chunk remains");
}

#[test]
fn rank_chunks_criteria_token_overlap_drives_ranking() {
    // chunk 0 has 3 criteria tokens; chunk 1 has 1. chunk 0 ranks first.
    let chunks = vec![
        chunk(0, Some("Text"), "sugar tax children"), // 3 matches
        chunk(1, Some("Text"), "sugar other prose words"), // 1 match
    ];
    let inc = criteria(&["sugar tax children obesity"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out[0].chunk_index, 0, "chunk with more overlap ranks first");
    assert!(out[0].score > out[1].score);
}

#[test]
fn rank_chunks_handles_stop_words_in_criteria() {
    // "the RCT and children" -> tokens {rct, children} only.
    let chunks = vec![
        chunk(0, Some("Methods"), "rct children participants"),
        chunk(1, Some("Text"), "the and is prose words"),
    ];
    let inc = criteria(&["the RCT and children"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out[0].chunk_index, 0, "chunk matching {{rct, children}} ranks first");
    assert!(out[0].score > 0.0);
}

#[test]
fn budget_guard_drops_lowest_chunk_when_over_budget() {
    // 3 chunks each ~300 words, top_k=3 -> sum ~900. Budget=700 forces one
    // drop (2 chunks = ~600 <= 700, so exactly one drop is enough).
    let words = "word ".repeat(300);
    let chunks = vec![
        chunk(0, Some("Methods"), &format!("sugar tax {words}")),
        chunk(1, Some("Results"), &format!("sugar tax {words}")),
        chunk(2, Some("Text"), &format!("sugar tax {words}")),
    ];
    let inc = criteria(&["sugar tax"]);
    // top_k=3, budget=700 => 3x~302=906 > 700 -> drop lowest -> 2x~302=604 <= 700.
    let out = rank_chunks_by_criteria(&chunks, &inc, &[], 3, DEFAULT_MAX_CHUNK_WORDS, 700);
    assert_eq!(out.len(), 2, "budget guard drops 1 chunk: got {}", out.len());
    // The Methods chunk (boosted, highest score) survives.
    assert!(
        out.iter().any(|c| c.section.as_deref() == Some("Methods")),
        "highest-ranked Methods chunk survives the drop"
    );
}

// ── Extra robustness tests ─────────────────────────────────────────

#[test]
fn rank_chunks_top_k_zero_returns_empty() {
    let chunks = vec![chunk(0, Some("Methods"), "sugar tax")];
    let inc = criteria(&["sugar"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &[],
        0,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert!(out.is_empty());
}

#[test]
fn rank_chunks_budget_never_drops_below_one() {
    // A single chunk that exceeds the budget but is within the size cap:
    // keep it (1 chunk > 0 evidence). Must pass a `max_chunk_words` high
    // enough that the chunk is NOT filtered out before the budget guard.
    let big = format!("sugar {}", "word ".repeat(800));
    let chunks = vec![chunk(0, Some("Methods"), &big)];
    let inc = criteria(&["sugar"]);
    let out = rank_chunks_by_criteria(&chunks, &inc, &[], 2, 1000, 100);
    assert_eq!(out.len(), 1, "never drop below 1 chunk even if over budget");
}

#[test]
fn rank_chunks_exclusion_criteria_also_contribute_tokens() {
    // Exclusion criteria tokens should also match (we rank by "criteria
    // relevance", not just inclusion relevance).
    let chunks = vec![
        chunk(0, Some("Methods"), "observational cohort adults"),
        chunk(1, Some("Text"), "unrelated prose here"),
    ];
    let inc = criteria(&[]);
    let exc = criteria(&["observational studies adults"]);
    let out = rank_chunks_by_criteria(
        &chunks,
        &inc,
        &exc,
        2,
        DEFAULT_MAX_CHUNK_WORDS,
        DEFAULT_CHUNK_BUDGET_PER_ARTICLE,
    );
    assert_eq!(out[0].chunk_index, 0, "chunk matching exclusion tokens ranks first");
    assert!(out[0].score > 0.0);
}
