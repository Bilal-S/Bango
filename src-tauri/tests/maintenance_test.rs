//! File-backed integration tests for `db::maintenance::vacuum_database`.
//!
//! These are intentionally NOT inline tests: the whole point is to observe
//! on-disk file sizes (`bango.db` + `bango.db-wal`), which requires a real
//! file-backed WAL-mode connection - an in-memory DB has no file to shrink.

use std::path::PathBuf;

use bango_lib::db::maintenance::vacuum_database;
use bango_lib::db::migration::run_migrations;
use rusqlite::Connection;
use tempfile::TempDir;

/// Open a file-backed WAL-mode connection mirroring the production setup in
/// `db::connection::create_connection_at` (WAL + foreign_keys + busy_timeout).
fn file_db(path: &PathBuf) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    )
    .unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Total on-disk byte size of the database files (`bango.db` + `bango.db-wal`
/// + `bango.db-shm`). The WAL file is where dropped-table work accumulates in
///   WAL mode, so the combined footprint is what VACUUM must shrink.
fn db_dir_size(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

#[test]
fn vacuum_reclaims_space_after_dropping_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bango.db");
    let conn = file_db(&db_path);

    // Seed a substantial amount of data so the file grows beyond the empty
    // baseline. Use `articles` (the largest, most typical user table) plus
    // `audit_entries` (historically the biggest space consumer in real
    // projects). Large `abstract_text` + `ai_reasoning` / `details` blobs force
    // page allocation that VACUUM will reclaim when the rows are dropped.
    let big_blob = "x".repeat(50_000);
    for i in 0..200 {
        conn.execute(
            "INSERT INTO articles (id, title, abstract_text, authors, status, ai_reasoning, \
             imported_at, changed_at) \
             VALUES (?1, ?2, ?3, '[]', 'working', ?4, datetime('now'), datetime('now'))",
            rusqlite::params![
                format!("art-{i}"),
                format!("Article number {i}"),
                &big_blob,
                &big_blob,
            ],
        )
        .unwrap();
    }
    for i in 0..200 {
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, action, source, details, timestamp) \
             VALUES (?1, ?2, 'status_change', 'user', ?3, datetime('now'))",
            rusqlite::params![format!("audit-{i}"), format!("art-{i}"), &big_blob,],
        )
        .unwrap();
    }

    // Checkpoint so the seeded WAL content is folded into bango.db before we
    // measure the "bloated" size. Without this the size could be understated
    // (the WAL still holds the inserts).
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_r| Ok(())).unwrap();

    let size_after_seed = db_dir_size(tmp.path());

    // Drop the tables we just populated (mirrors what rebuild_schema does).
    // Drop audit_entries first because its `article_id` FK references
    // articles(id); with foreign_keys=ON dropping articles first would
    // cascade-delete audit_entries, but explicit ordering keeps the intent
    // unambiguous.
    conn.execute_batch("DROP TABLE IF EXISTS audit_entries; DROP TABLE IF EXISTS articles;")
        .unwrap();

    // Measure BEFORE vacuum: SQLite retains the freed pages in the file.
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_r| Ok(())).unwrap();
    let size_before_vacuum = db_dir_size(tmp.path());

    // Run the helper under test.
    vacuum_database(&conn).unwrap();

    let size_after_vacuum = db_dir_size(tmp.path());

    // The helper must reclaim space: the post-vacuum footprint is strictly
    // smaller than the pre-vacuum footprint, and also strictly smaller than the
    // seeded (pre-drop) footprint. This proves both the VACUUM and the WAL
    // checkpoint (TRUNCATE) ran.
    assert!(
        size_after_vacuum < size_before_vacuum,
        "VACUUM did not shrink the DB: before = {size_before_vacuum}, after = {size_after_vacuum}"
    );
    assert!(
        size_after_vacuum < size_after_seed,
        "VACUUM did not reclaim the seeded data: seed = {size_after_seed}, after = {size_after_vacuum}"
    );
}

#[test]
fn vacuum_is_safe_on_fresh_empty_db() {
    // The helper must be a safe no-op on a freshly migrated empty database
    // (the reset_project_inner path calls it after rebuild_schema, which
    // produces exactly this state).
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bango.db");
    let conn = file_db(&db_path);

    vacuum_database(&conn).unwrap();

    // The schema is still intact and usable.
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}
