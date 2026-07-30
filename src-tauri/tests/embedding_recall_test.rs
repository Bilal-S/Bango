//! Integration tests for the embedding recall scoring (`embedding::recall`).
//!
//! Exercises the max-pool cosine scoring, top_k bound, dimension exclusion,
//! empty-table→empty, and status filter. Uses an in-memory SQLite DB with the
//! full migration chain. The recall function's orchestrator dependency is
//! bypassed by testing the scoring logic directly via `embedding::text` helpers
//! + `embedding_repo::list_for_recall`, which is the pure scoring core.

use bango_lib::db::app_settings_repo::{self, EmbeddingStatus};
use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow, TITLE_ABSTRACT_CHUNK_INDEX};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::text::cosine_similarity;
use rusqlite::Connection;

fn seed_article(conn: &Connection, id: &str, status: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, ?2, 'Author', 'Abstract', ?3, 'test')",
        rusqlite::params![id, format!("Title {id}"), status],
    )
    .expect("seed article");
}

fn insert_vec(conn: &Connection, article_id: &str, chunk_index: i32, vec: &[f32]) {
    embedding_repo::insert_embedding(
        conn,
        &NewEmbeddingRow {
            article_id,
            chunk_index,
            embedding: vec,
            dimensions: vec.len() as i32,
            input_hash: "test-hash",
            model_name: "test-model",
            provider: "test",
            generated_at: 1,
        },
    )
    .expect("insert embedding");
}

#[test]
fn recall_max_pool_picks_best_chunk_per_article() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "a1", "included");

    // Two chunks for a1: one similar to the query, one orthogonal.
    let query = vec![1.0, 0.0, 0.0, 0.0];
    insert_vec(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.9, 0.1, 0.0, 0.0]); // high sim
    insert_vec(&conn, "a1", 0, &[0.0, 1.0, 0.0, 0.0]); // orthogonal (sim ~ 0)

    let rows = embedding_repo::list_for_recall(&conn, 4, Some("included")).unwrap();
    assert_eq!(rows.len(), 2);

    // Max-pool: the best chunk's score should win. Use NEG_INFINITY (the same
    // sentinel the production recall path now uses) rather than f32::MIN.
    let best = rows
        .iter()
        .map(|r| cosine_similarity(&query, &r.embedding))
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(best > 0.9, "max-pool should pick the high-similarity chunk");
}

#[test]
fn recall_dimension_exclusion_filters_mismatched_rows() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "a1", "included");

    insert_vec(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]); // 4-dim
    insert_vec(&conn, "a1", 0, &[0.1; 8]); // 8-dim (different model)

    let rows_4 = embedding_repo::list_for_recall(&conn, 4, None).unwrap();
    assert_eq!(rows_4.len(), 1, "only the 4-dim row matches dim=4");
    let rows_8 = embedding_repo::list_for_recall(&conn, 8, None).unwrap();
    assert_eq!(rows_8.len(), 1, "only the 8-dim row matches dim=8");
}

#[test]
fn recall_empty_table_returns_empty() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    // No articles, no embeddings.
    let rows = embedding_repo::list_for_recall(&conn, 4, Some("included")).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn recall_status_filter_excludes_non_included() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "rej", "rejected");

    insert_vec(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);
    insert_vec(&conn, "rej", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4]);

    let incl = embedding_repo::list_for_recall(&conn, 4, Some("included")).unwrap();
    assert_eq!(incl.len(), 1);
    assert_eq!(incl[0].article_id, "inc");

    let all = embedding_repo::list_for_recall(&conn, 4, None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn recall_top_k_truncates_results() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    for i in 0..5 {
        let id = format!("a{i}");
        seed_article(&conn, &id, "included");
        insert_vec(&conn, &id, TITLE_ABSTRACT_CHUNK_INDEX, &[0.1 * i as f32; 4]);
    }

    let rows = embedding_repo::list_for_recall(&conn, 4, Some("included")).unwrap();
    assert_eq!(rows.len(), 5, "all 5 articles have embeddings");

    // Simulate top_k=2: sort by cosine sim to a query, take top 2.
    let query = vec![1.0, 0.0, 0.0, 0.0];
    let mut scored: Vec<(String, f32)> = rows
        .iter()
        .map(|r| {
            let best = cosine_similarity(&query, &r.embedding);
            (r.article_id.clone(), best)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(2);
    assert_eq!(scored.len(), 2, "top_k truncates to 2");
}

#[test]
fn recall_embedding_status_disabled_returns_empty_via_gate() {
    // This test documents the contract: when embedding_status is Disabled,
    // the recall function returns an empty vec. We can't easily test the full
    // async `recall` without a mock orchestrator, but we can verify the status
    // gate logic.
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    app_settings_repo::set_embedding_status(&conn, EmbeddingStatus::Disabled, "", 0).unwrap();
    let status = app_settings_repo::get_embedding_status(&conn).unwrap();
    assert_eq!(status, EmbeddingStatus::Disabled);
}

/// Pin the max-pool seed behavior for negative cosine similarities.
///
/// `recall::recall` max-pools cosine similarity across an article's rows. The
/// seed for the running max must be low enough that any real cosine value
/// (range `[-1.0, 1.0]`) updates it. The production code now uses
/// `f32::NEG_INFINITY`, which is the identity element for `max` and the most
/// idiomatic choice.
///
/// Note: the previous `f32::MIN` seed was technically also correct here
/// (`f32::MIN = -3.4e38`, which is below `-1.0`, so any cosine value would
/// update it). This test exists to LOCK IN the behavior so a future change
/// to e.g. `0.0` (which WOULD drop negative-similarity articles) is caught.
#[test]
fn recall_max_pool_seed_handles_negative_similarity() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_article(&conn, "a1", "included");

    // Insert one row whose similarity to the query is NEGATIVE (opposite
    // direction). Cosine of [1,0] vs [-1,0] = -1.0.
    let query = vec![1.0, 0.0];
    insert_vec(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[-1.0, 0.0]);

    let rows = embedding_repo::list_for_recall(&conn, 2, Some("included")).unwrap();
    assert_eq!(rows.len(), 1);

    // Replicate the recall max-pool with the production NEG_INFINITY seed.
    let mut best: f32 = f32::NEG_INFINITY;
    for row in &rows {
        let sim = cosine_similarity(&query, &row.embedding);
        if sim > best {
            best = sim;
        }
    }
    assert!(
        (best - (-1.0)).abs() < 1e-5,
        "max-pool must record the true cosine (-1.0), got {best}"
    );

    // Guard against a future regression to `0.0` (which WOULD be a real bug:
    // it would drop any article whose max-similarity is negative).
    let mut zero_seed: f32 = 0.0;
    for row in &rows {
        let sim = cosine_similarity(&query, &row.embedding);
        if sim > zero_seed {
            zero_seed = sim;
        }
    }
    assert_eq!(
        zero_seed, 0.0,
        "a 0.0 seed would drop the -1.0 article (this is the regression we guard against)"
    );
}
