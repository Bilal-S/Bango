//! Integration tests for `db::embedding_repo` storage CRUD.
//!
//! Covers insert/replace idempotency, delete (per-article + cascade), count,
//! hash lookup, and the recall list filtered by dimensions + status. Uses an
//! in-memory SQLite DB with the full migration chain so the FK to `articles`
//! and the `ON DELETE CASCADE` contract are exercised.

use bango_lib::db::connection::create_connection;
use bango_lib::db::embedding_repo::{self, NewEmbeddingRow, TITLE_ABSTRACT_CHUNK_INDEX};
use bango_lib::db::migration::run_migrations;
use bango_lib::embedding::text;
use rusqlite::Connection;

fn seed_article(conn: &Connection, id: &str, status: &str) {
    conn.execute(
        "INSERT INTO articles (id, title, authors, abstract_text, status, import_source) \
         VALUES (?1, ?2, 'Author', 'Abstract', ?3, 'test')",
        rusqlite::params![id, format!("Title {id}"), status],
    )
    .expect("seed article");
}

/// Convenience helper: insert a row with defaults for the metadata fields so
/// the test bodies stay focused on the storage behavior under test.
fn insert(
    conn: &Connection,
    article_id: &str,
    chunk_index: i32,
    embedding: &[f32],
    dimensions: i32,
    input_hash: &str,
) {
    embedding_repo::insert_embedding(
        conn,
        &NewEmbeddingRow {
            article_id,
            chunk_index,
            embedding,
            dimensions,
            input_hash,
            model_name: "m",
            provider: "p",
            generated_at: 1,
        },
    )
    .expect("insert");
}

#[test]
fn insert_and_count_row() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 0);
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 8], 8, "hash1");
    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 1);
    assert_eq!(embedding_repo::count_embeddings_for_article(&conn, "a1").unwrap(), 1);
}

#[test]
fn insert_or_replace_is_idempotent_on_pk() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    // Same (article_id, chunk_index) -> replaces, count stays 1.
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h1");
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.2; 4], 4, "h2");
    assert_eq!(
        embedding_repo::count_embeddings_for_article(&conn, "a1").unwrap(),
        1,
        "INSERT OR REPLACE on the same PK replaces, not duplicates"
    );

    // Different chunk_index -> separate row.
    insert(&conn, "a1", 0, &[0.3; 4], 4, "h3");
    assert_eq!(embedding_repo::count_embeddings_for_article(&conn, "a1").unwrap(), 2);
}

#[test]
fn delete_embeddings_for_article() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");
    seed_article(&conn, "a2", "included");

    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h");
    insert(&conn, "a1", 0, &[0.1; 4], 4, "h");
    insert(&conn, "a2", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h");

    embedding_repo::delete_embeddings_for_article(&conn, "a1").unwrap();
    assert_eq!(embedding_repo::count_embeddings_for_article(&conn, "a1").unwrap(), 0);
    assert_eq!(embedding_repo::count_embeddings_for_article(&conn, "a2").unwrap(), 1);
}

#[test]
fn cascade_delete_when_article_deleted() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    // Migrations + connection enable PRAGMA foreign_keys = ON, so deleting the
    // parent article cascades to article_embeddings.
    seed_article(&conn, "a1", "included");
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h");
    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 1);

    conn.execute("DELETE FROM articles WHERE id = 'a1'", []).expect("delete article");
    assert_eq!(
        embedding_repo::count_embeddings(&conn).unwrap(),
        0,
        "ON DELETE CASCADE removes embedding rows"
    );
}

#[test]
fn get_input_hash_round_trip() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    // Title+abstract row (sentinel -1).
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "hash-ta");
    // Chunk row (chunk_index 0).
    insert(&conn, "a1", 0, &[0.1; 4], 4, "hash-c0");

    assert_eq!(
        embedding_repo::get_input_hash(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX).unwrap(),
        Some("hash-ta".to_string())
    );
    assert_eq!(
        embedding_repo::get_input_hash(&conn, "a1", 0).unwrap(),
        Some("hash-c0".to_string())
    );
    // Missing row -> None.
    assert_eq!(embedding_repo::get_input_hash(&conn, "a1", 99).unwrap(), None);
}

#[test]
fn list_hashes_for_article() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h0");
    insert(&conn, "a1", 0, &[0.1; 4], 4, "h1");
    insert(&conn, "a1", 1, &[0.1; 4], 4, "h2");

    let mut hashes = embedding_repo::list_hashes_for_article(&conn, "a1").unwrap();
    hashes.sort();
    assert_eq!(hashes.len(), 3);
    assert_eq!(hashes[0], (TITLE_ABSTRACT_CHUNK_INDEX, "h0".to_string()));
    assert_eq!(hashes[1], (0, "h1".to_string()));
    assert_eq!(hashes[2], (1, "h2".to_string()));
}

#[test]
fn list_for_recall_filters_by_dimension() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    // 4-dim row + 8-dim row (simulates a provider switch leaving stale dims).
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h4");
    insert(&conn, "a1", 0, &[0.1; 8], 8, "h8");

    let rows4 = embedding_repo::list_for_recall(&conn, 4, None).unwrap();
    assert_eq!(rows4.len(), 1, "only the 4-dim row matches dim=4");
    let rows8 = embedding_repo::list_for_recall(&conn, 8, None).unwrap();
    assert_eq!(rows8.len(), 1, "only the 8-dim row matches dim=8");
}

#[test]
fn list_for_recall_filters_by_status() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "inc", "included");
    seed_article(&conn, "wk", "working");

    insert(&conn, "inc", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h");
    insert(&conn, "wk", TITLE_ABSTRACT_CHUNK_INDEX, &[0.1; 4], 4, "h");

    let incl = embedding_repo::list_for_recall(&conn, 4, Some("included")).unwrap();
    assert_eq!(incl.len(), 1);
    assert_eq!(incl[0].article_id, "inc");

    let all = embedding_repo::list_for_recall(&conn, 4, None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn list_for_recall_decodes_embedding_blob() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    let original = vec![0.1, -0.2, 0.3, 0.4];
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &original, 4, "h");

    let rows = embedding_repo::list_for_recall(&conn, 4, None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].embedding.len(), 4);
    for (a, b) in original.iter().zip(rows[0].embedding.iter()) {
        assert!((a - b).abs() < 1e-6, "decoded blob matches");
    }
}

#[test]
fn empty_table_count_is_zero() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    assert_eq!(embedding_repo::count_embeddings(&conn).unwrap(), 0);
    assert!(embedding_repo::list_for_recall(&conn, 8, None).unwrap().is_empty());
}

#[test]
fn text_helpers_serialize_matches_repo_blob() {
    // Defense-in-depth: the byte stream the repo stores must be byte-identical
    // to what text::serialize_embedding produces, so the manual deserialize in
    // the recall path round-trips.
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrations");
    seed_article(&conn, "a1", "included");

    let original = vec![0.5, -0.25, 0.0, 1.0];
    insert(&conn, "a1", TITLE_ABSTRACT_CHUNK_INDEX, &original, 4, "h");

    // Read the raw blob directly and confirm text::deserialize decodes it.
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT embedding FROM article_embeddings WHERE article_id = 'a1' AND chunk_index = -1",
            [],
            |row| row.get(0),
        )
        .expect("read blob");
    let decoded = text::deserialize_embedding(&blob, 4).expect("decode");
    assert_eq!(decoded.len(), 4);
    for (a, b) in original.iter().zip(decoded.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}
