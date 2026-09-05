//! Integration tests for `bango_lib::load_journal_index_from_path`.
//!
//! These tests cover the contract that the loader must satisfy, with a
//! particular focus on the **WAL-mode source regression** that motivated the
//! switch from `ATTACH DATABASE` to a two-connection (read-only source +
//! target transaction) approach on Windows.

use std::fs;
use std::path::PathBuf;

use bango_lib::db::migration::run_migrations;
use bango_lib::load_journal_index_from_path;
use rusqlite::Connection;
use tempfile::TempDir;

/// The exact `CREATE TABLE journal_index` statement from `v001_initial.rs`,
/// duplicated here so the source DB shape matches what the production loader
/// streams. Using the live migration's SQL directly would be cleaner, but
/// `UP_SQL` is private; embedding the table DDL keeps the test independent of
/// the migration module's internal visibility.
const SOURCE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS journal_index (
    id TEXT PRIMARY KEY,
    journal_title TEXT NOT NULL,
    issn TEXT,
    eissn TEXT,
    publisher_name TEXT,
    publisher_address TEXT,
    languages TEXT,
    web_of_science_categories TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    source_file TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";

/// A fresh in-memory target with migrations applied (so `journal_index`
/// exists and is empty).
fn empty_target() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory target");
    run_migrations(&conn).expect("run migrations on target");
    conn
}

/// Insert one journal_index row with arbitrary field values.
fn insert_row(
    conn: &Connection,
    id: &str,
    title: &str,
    issn: Option<&str>,
    eissn: Option<&str>,
    publisher: Option<&str>,
) {
    conn.execute(
        "INSERT INTO journal_index
            (id, journal_title, issn, eissn, publisher_name,
             publisher_address, languages, web_of_science_categories,
             is_system, source_file, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, 0, NULL, '2024-01-01', '2024-01-01')",
        rusqlite::params![id, title, issn, eissn, publisher],
    )
    .expect("insert source row");
}

/// Build a source DB file on disk in `delete` journal mode (the default when
/// a file is freshly created) populated with `n` test rows.
fn build_source_delete_mode(dir: &TempDir, n: usize) -> PathBuf {
    let path = dir.path().join("source-delete.db");
    let conn = Connection::open(&path).expect("open source");
    conn.execute_batch(SOURCE_SCHEMA).expect("create source schema");
    for i in 0..n {
        insert_row(
            &conn,
            &format!("src-{i}"),
            &format!("Journal #{i}"),
            Some(&format!("{i:04}-0000")),
            Some(&format!("{i:04}-1111")),
            Some("Test Publisher"),
        );
    }
    // `PRAGMA wal_checkpoint(TRUNCATE)` is a no-op in delete mode but keeps
    // the helper symmetric with the WAL variant.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").expect("checkpoint source");
    conn.close().expect("close source connection");
    path
}

/// Build a source DB file on disk in **WAL** journal mode populated with `n`
/// test rows, then close the writer cleanly so a valid `-wal` + `-shm`
/// sidecar pair exists. This is the regression fixture: it mirrors the shape
/// of the bundled production DB on Windows, where `ATTACH DATABASE` failed
/// to acquire the cross-database lock inside the target's transaction.
fn build_source_wal_mode(dir: &TempDir, n: usize) -> PathBuf {
    let path = dir.path().join("source-wal.db");
    let conn = Connection::open(&path).expect("open source");
    conn.execute_batch("PRAGMA journal_mode=WAL;").expect("set WAL mode");
    conn.execute_batch(SOURCE_SCHEMA).expect("create source schema");
    for i in 0..n {
        insert_row(
            &conn,
            &format!("wal-{i}"),
            &format!("WAL Journal #{i}"),
            Some(&format!("{i:04}-2222")),
            Some(&format!("{i:04}-3333")),
            Some("WAL Publisher"),
        );
    }
    // Checkpoint + close so the WAL is reconciled into the main file but the
    // WAL/SHM sidecars remain (mirroring a shipped bundled DB).
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").expect("checkpoint source");
    conn.close().expect("close source connection");
    path
}

#[test]
fn load_copies_all_rows_from_delete_mode_source() {
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_delete_mode(&tmp, 3);
    let target = empty_target();

    load_journal_index_from_path(&target, &source).expect("load succeeds");

    let count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 3, "all source rows copied");

    // Spot-check data integrity on the middle row.
    let (title, issn, eissn, publisher): (String, Option<String>, Option<String>, Option<String>) =
        target
            .query_row(
                "SELECT journal_title, issn, eissn, publisher_name \
                 FROM journal_index WHERE id = 'src-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("fetch row");
    assert_eq!(title, "Journal #1");
    assert_eq!(issn.as_deref(), Some("0001-0000"));
    assert_eq!(eissn.as_deref(), Some("0001-1111"));
    assert_eq!(publisher.as_deref(), Some("Test Publisher"));
}

#[test]
fn load_skips_when_target_already_populated() {
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_delete_mode(&tmp, 3);
    let target = empty_target();

    // Pre-seed the target with one row so the loader's short-circuit fires.
    insert_row(&target, "pre-existing", "Existing Journal", None, None, None);
    let before: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(before, 1);

    load_journal_index_from_path(&target, &source).expect("load is Ok (no-op)");

    let after: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(after, 1, "target unchanged: source rows NOT copied over");
    let pre_existing_count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index WHERE id = 'pre-existing'", [], |row| {
            row.get(0)
        })
        .expect("query pre-existing");
    assert_eq!(pre_existing_count, 1, "pre-existing row preserved");
}

#[test]
fn load_marks_rows_as_system() {
    // The loader forces `is_system = 1` on every transferred row so the
    // target can distinguish bundled (system) rows from any user-added rows.
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_delete_mode(&tmp, 2);
    let target = empty_target();

    load_journal_index_from_path(&target, &source).expect("load succeeds");

    let non_system_count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index WHERE is_system = 0", [], |row| row.get(0))
        .expect("count non-system");
    assert_eq!(non_system_count, 0, "every transferred row must be is_system=1");
}

#[test]
fn load_is_idempotent_on_rerun() {
    // Calling the loader a second time after the table is populated must not
    // duplicate rows or error. The short-circuit (count > 0) handles the
    // normal case; the `INSERT OR IGNORE` is defense-in-depth if the count
    // check ever lets a duplicate through.
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_delete_mode(&tmp, 3);
    let target = empty_target();

    load_journal_index_from_path(&target, &source).expect("first load");
    load_journal_index_from_path(&target, &source).expect("second load is a no-op");

    let count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 3, "still 3 rows after second load (no duplicates)");
}

#[test]
fn load_handles_missing_source_file() {
    // A nonexistent source path is treated as "nothing to load" and returns
    // `Ok(())`. This matches the production startup contract: a missing
    // bundled DB degrades journal matching silently rather than crashing.
    let tmp = TempDir::new().expect("temp dir");
    let missing = tmp.path().join("does-not-exist.db");
    let target = empty_target();

    load_journal_index_from_path(&target, &missing).expect("missing source is Ok");

    let count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 0, "target still empty after missing-source load");
}

#[test]
fn load_succeeds_with_wal_mode_source() {
    // **The key Windows regression.** The bundled source DB may be in WAL
    // journal mode (with `-wal`/`-shm` sidecars). The previous `ATTACH
    // DATABASE` implementation failed on this shape because SQLite could not
    // acquire the cross-database lock within the target's transaction. The
    // two-connection (read-only source) approach must succeed.
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_wal_mode(&tmp, 3);
    let target = empty_target();

    load_journal_index_from_path(&target, &source)
        .expect("WAL-mode source load must succeed (regression)");

    let count: i64 = target
        .query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 3, "all WAL source rows copied");

    // Spot-check that WAL-specific data transferred correctly.
    let title: String = target
        .query_row("SELECT journal_title FROM journal_index WHERE id = 'wal-0'", [], |row| {
            row.get(0)
        })
        .expect("fetch wal row");
    assert_eq!(title, "WAL Journal #0");
}

#[test]
fn load_is_read_only_on_source() {
    // Sanity check that the `SQLITE_OPEN_READ_ONLY` flag held: after a load,
    // the source file's byte content must be identical to a snapshot taken
    // before the load. This is the contract that lets us ship a read-only
    // bundled DB and guarantee the installer artifact is never mutated.
    let tmp = TempDir::new().expect("temp dir");
    let source = build_source_delete_mode(&tmp, 2);
    let before = fs::read(&source).expect("read source before");

    let target = empty_target();
    load_journal_index_from_path(&target, &source).expect("load succeeds");

    let after = fs::read(&source).expect("read source after");
    assert_eq!(before, after, "source file bytes unchanged after load (READ_ONLY held)");
}
