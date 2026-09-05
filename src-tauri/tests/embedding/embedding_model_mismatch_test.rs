//! Integration tests for the embedding model-mismatch detection
//! (`commands::embedding::first_mismatched_model` + the
//! `get_embedding_model_mismatch` command's repo helpers). Covers the pure
//! mismatch predicate + the `list_distinct_model_names` + `delete_all_embeddings`
//! repo round-trip.
//!
//! The pure predicate is `#[must_use]` so it is unit-testable in isolation
//! without a DB. The repo helpers run against an in-memory SQLite DB with the
//! full migration chain.

use bango_lib::commands::embedding::first_mismatched_model;
use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow, TITLE_ABSTRACT_CHUNK_INDEX};
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;

fn seed_embedding(conn: &Connection, article_id: &str, model: &str) {
    // Seed a minimal article row so the FK constraint is satisfied.
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, 'T', 'A', 'Abs', 'included', 'test')",
        rusqlite::params![article_id],
    )
    .expect("seed article");
    embedding_repo::insert_embedding(
        conn,
        &NewEmbeddingRow {
            article_id,
            chunk_index: TITLE_ABSTRACT_CHUNK_INDEX,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: "h",
            model_name: model,
            provider: "p",
            generated_at: 1,
        },
    )
    .expect("insert embedding");
}

// ── first_mismatched_model (pure) ─────────────────────────────────────────

#[test]
fn no_mismatch_when_stored_matches_current() {
    let stored = vec!["text-embedding-3-small".to_string()];
    assert_eq!(first_mismatched_model(&stored, Some("text-embedding-3-small")), None);
}

#[test]
fn no_mismatch_when_nothing_stored() {
    // Empty stored vec = nothing to compare = no mismatch.
    let stored: Vec<String> = Vec::new();
    assert_eq!(first_mismatched_model(&stored, Some("text-embedding-3-small")), None);
}

#[test]
fn mismatch_when_stored_differs_from_current() {
    let stored = vec!["text-embedding-3-large".to_string()];
    let mismatch = first_mismatched_model(&stored, Some("text-embedding-3-small"));
    assert_eq!(mismatch.as_deref(), Some("text-embedding-3-large"));
}

#[test]
fn mismatch_case_insensitive() {
    // Model names are ASCII; the predicate uses `eq_ignore_ascii_case` so
    // `TEXT-EMBEDDING-3-SMALL` vs `text-embedding-3-small` is NOT a mismatch.
    let stored = vec!["TEXT-EMBEDDING-3-SMALL".to_string()];
    assert_eq!(first_mismatched_model(&stored, Some("text-embedding-3-small")), None);
}

#[test]
fn mismatch_returns_first_offending_model_when_multiple_stored() {
    // When multiple distinct stored models exist (e.g. the user switched twice
    // without regenerating), the first non-matching one is reported.
    let stored = vec!["text-embedding-3-small".to_string(), "text-embedding-3-large".to_string()];
    let mismatch = first_mismatched_model(&stored, Some("text-embedding-3-small"));
    assert_eq!(mismatch.as_deref(), Some("text-embedding-3-large"));
}

#[test]
fn mismatch_when_current_set_but_stored_empty() {
    // A stored empty model name (pre-feature row, or corrupt) is a mismatch
    // when the current model is known so the row is flagged for regeneration
    // + the column backfilled.
    let stored = vec![String::new()];
    let mismatch = first_mismatched_model(&stored, Some("text-embedding-3-small"));
    assert_eq!(mismatch.as_deref(), Some(""));
}

#[test]
fn no_mismatch_when_both_current_and_stored_empty() {
    // Edge case: nothing has been probed yet + nothing stored. Not a mismatch.
    let stored = vec![String::new()];
    assert_eq!(first_mismatched_model(&stored, None), None);
}

// ── list_distinct_model_names + delete_all_embeddings (repo) ──────────────

#[test]
fn list_distinct_model_names_returns_unique_values() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_embedding(&conn, "a1", "text-embedding-3-small");
    seed_embedding(&conn, "a2", "text-embedding-3-small");
    seed_embedding(&conn, "a3", "text-embedding-3-large");

    let names = embedding_repo::list_distinct_model_names(&conn).unwrap();
    // Two distinct model names regardless of the three rows.
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"text-embedding-3-small".to_string()));
    assert!(names.contains(&"text-embedding-3-large".to_string()));
}

#[test]
fn list_distinct_model_names_empty_when_table_empty() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    let names = embedding_repo::list_distinct_model_names(&conn).unwrap();
    assert!(names.is_empty());
}

#[test]
fn list_distinct_model_names_omits_null_and_empty() {
    // Defense-in-depth: a corrupt row with an empty model_name is filtered out
    // of the distinct list (the pure helper has a separate empty-model arm
    // that handles this case via the stored-vec contents, not via the repo).
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_embedding(&conn, "a1", "text-embedding-3-small");
    // Insert a row with an empty model_name directly.
    embedding_repo::insert_embedding(
        &conn,
        &NewEmbeddingRow {
            article_id: "a1",
            chunk_index: 0,
            embedding: &[0.1; 4],
            dimensions: 4,
            input_hash: "h0",
            model_name: "",
            provider: "p",
            generated_at: 1,
        },
    )
    .unwrap();
    let names = embedding_repo::list_distinct_model_names(&conn).unwrap();
    assert_eq!(names, vec!["text-embedding-3-small".to_string()]);
}

#[test]
fn delete_all_embeddings_clears_every_row() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();
    seed_embedding(&conn, "a1", "m");
    seed_embedding(&conn, "a2", "m");
    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 2);

    embedding_repo::delete_all_embeddings(&conn).unwrap();
    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 0);
}
