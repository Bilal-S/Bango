//! Legacy schema upgrade integration tests.
//!
//! Exercises the full round-trip without a Tauri runtime:
//!   legacy `article_references` schema
//!     -> `export_legacy_project` (JSON backup)
//!     -> `rebuild_schema` (drop all + re-run migrations)
//!     -> `import_project` (reload user data into current schema)
//!
//! Also covers `check_schema` classification for Current / FreshDb / Legacy.

use rusqlite::Connection;

use bango_lib::db::migration::run_migrations;
use bango_lib::db::rebuild::rebuild_schema;
use bango_lib::db::schema_check::{check_schema, SchemaStatus};
use bango_lib::export::legacy_project::export_legacy_project;
use bango_lib::export::project::import_project;

/// Build the legacy v1 schema (commit 665ec93) into an in-memory DB.
fn legacy_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    // Minimal-but-representative legacy DDL matching the columns selected by
    // `dedup_legacy_references`. Other legacy tables (research_aims, etc.)
    // share their shapes with the current schema and are not the focus here.
    conn.execute_batch(
        "CREATE TABLE articles (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'working',
            title TEXT NOT NULL,
            abstract_text TEXT NOT NULL,
            authors TEXT NOT NULL,
            doi TEXT,
            journal TEXT,
            publication_year INTEGER,
            keywords TEXT
        );
        CREATE TABLE tags (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            color TEXT,
            source TEXT NOT NULL
        );
        CREATE TABLE article_tags (
            article_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            PRIMARY KEY (article_id, tag_id)
        );
        CREATE TABLE article_references (
            id TEXT PRIMARY KEY,
            parent_id TEXT NOT NULL,
            type INTEGER NOT NULL,
            match_status TEXT NOT NULL DEFAULT 'unmatched',
            matched_article_id TEXT,
            title TEXT,
            abstract_text TEXT,
            authors TEXT,
            publication_year INTEGER,
            doi TEXT,
            journal TEXT,
            volume TEXT,
            issue TEXT,
            start_page TEXT,
            end_page TEXT,
            keywords TEXT,
            url TEXT,
            language TEXT,
            publisher TEXT,
            publisher_city TEXT,
            publisher_address TEXT,
            issn TEXT,
            eissn TEXT,
            reference_type TEXT,
            date TEXT,
            notes TEXT,
            ris_extras TEXT,
            num_cited INTEGER,
            num_references INTEGER,
            import_source TEXT,
            imported_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (parent_id) REFERENCES articles(id) ON DELETE CASCADE
        );",
    )
    .unwrap();
    conn.execute("PRAGMA user_version = 1;", []).unwrap();
    conn
}

#[test]
fn classifies_current_schema_as_current() {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Current);
    // rebuild should be a no-op-safe operation on a current schema.
    rebuild_schema(&mut conn).unwrap();
    assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Current);
}

#[test]
fn classifies_fresh_db_as_fresh() {
    let conn = Connection::open_in_memory().unwrap();
    assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::FreshDb);
}

#[test]
fn classifies_legacy_schema_as_legacy() {
    let conn = legacy_db();
    assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Legacy);
}

#[test]
fn full_legacy_upgrade_round_trip_preserves_data() {
    let mut conn = legacy_db();

    // Seed a parent article + a tag + a reference + a citation. Two reference
    // rows share the same DOI to verify dedup into a single reference_paper.
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors, doi, journal, publication_year)
         VALUES ('art-1', 'included', 'Parent Paper', 'Abstract', '[\"Doe J\"]', '10.1/parent', 'Nature', 2021)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO tags (id, name, source) VALUES ('tag-1', 'ml', 'user_created')", [])
        .unwrap();
    conn.execute("INSERT INTO article_tags (article_id, tag_id) VALUES ('art-1', 'tag-1')", [])
        .unwrap();
    // type 1 = reference (cited by parent), type 0 = citation (cites parent)
    conn.execute(
        "INSERT INTO article_references (id, parent_id, type, title, doi, authors, publication_year)
         VALUES ('ref-1', 'art-1', 1, 'Shared Ref', '10.9/shared', '[\"Smith A\"]', 2018)",
        [],
    )
    .unwrap();
    // A second article citing the parent; its reference row shares the DOI above.
    conn.execute(
        "INSERT INTO article_references (id, parent_id, type, title, doi, authors, publication_year)
         VALUES ('ref-2', 'art-1', 0, 'Shared Ref', '10.9/shared', '[\"Smith A\"]', 2018)",
        [],
    )
    .unwrap();
    // A unique citation.
    conn.execute(
        "INSERT INTO article_references (id, parent_id, type, title, doi, authors, publication_year)
         VALUES ('ref-3', 'art-1', 0, 'Unique Cite', '10.9/unique', '[\"Jones B\"]', 2019)",
        [],
    )
    .unwrap();

    // 1. Export legacy DB to current-format JSON.
    let backup_json = export_legacy_project(&conn).unwrap();

    // 2. Rebuild the schema (drops article_references, creates current tables).
    rebuild_schema(&mut conn).unwrap();

    // The legacy table must be gone; current tables must exist.
    let legacy_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='article_references'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(legacy_count, 0);
    let ref_papers_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='reference_papers'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ref_papers_count, 1);

    // Schema is now Current.
    assert_eq!(check_schema(&conn).unwrap(), SchemaStatus::Current);

    // 3. Reload user data via import_project.
    import_project(&conn, &backup_json).unwrap();

    // Article preserved.
    let article_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0)).unwrap();
    assert_eq!(article_count, 1);
    let title: String =
        conn.query_row("SELECT title FROM articles WHERE id = 'art-1'", [], |r| r.get(0)).unwrap();
    assert_eq!(title, "Parent Paper");

    // Tag + link preserved.
    let tag_count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap();
    assert_eq!(tag_count, 1);
    let at_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM article_tags", [], |r| r.get(0)).unwrap();
    assert_eq!(at_count, 1);

    // The three legacy rows deduplicated into TWO reference_papers (shared DOI
    // collapsed ref-1 + ref-2 into one paper; ref-3 is distinct).
    let paper_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM reference_papers", [], |r| r.get(0)).unwrap();
    assert_eq!(paper_count, 2, "shared-DOI rows must dedup to one paper");

    // All three links preserved (with remapped paper ids).
    let link_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM article_reference_links", [], |r| r.get(0)).unwrap();
    assert_eq!(link_count, 3);
}

// ── legacy_upgrade_needed decision function ──
// This is the pure logic layer 1 of loop-safety relies on. It must prefer the
// live probe over the snapshot fallback, and only fall back when the live
// probe itself errored.

use bango_lib::commands::startup::legacy_upgrade_needed;
use bango_lib::error::AppError;

#[test]
fn legacy_upgrade_needed_returns_true_when_live_probe_is_legacy() {
    // Snapshot says Current, but live DB says Legacy -> must run upgrade.
    assert!(legacy_upgrade_needed(Ok(SchemaStatus::Legacy), SchemaStatus::Current));
}

#[test]
fn legacy_upgrade_needed_returns_false_when_live_probe_is_current() {
    // This is the loop-breaker: snapshot still says Legacy (frozen at setup),
    // but the live DB is now Current after the upgrade -> must NOT re-run.
    assert!(!legacy_upgrade_needed(Ok(SchemaStatus::Current), SchemaStatus::Legacy));
}

#[test]
fn legacy_upgrade_needed_returns_false_when_live_probe_is_fresh() {
    assert!(!legacy_upgrade_needed(Ok(SchemaStatus::FreshDb), SchemaStatus::Legacy));
}

#[test]
fn legacy_upgrade_needed_falls_back_to_snapshot_when_live_probe_errors() {
    // If the live probe errors, fall back to the snapshot. Legacy snapshot ->
    // run (fail-safe).
    let err =
        AppError::Database(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), None));
    assert!(legacy_upgrade_needed(Err(err), SchemaStatus::Legacy));
    // Current snapshot -> skip.
    let err2 =
        AppError::Database(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(1), None));
    assert!(!legacy_upgrade_needed(Err(err2), SchemaStatus::Current));
}
