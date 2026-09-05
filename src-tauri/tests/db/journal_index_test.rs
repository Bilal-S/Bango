/// Tests for journal_index portal DB loading logic.
///
/// Validates that the ATTACH + bulk-copy SQL works correctly when
/// the bundled journal_index.db is found at runtime.
use rusqlite::Connection;

/// Helper: create a minimal portal DB with the journal_index table and sample rows.
fn create_portal_db(path: &std::path::Path, rows: &[(&str, &str, &str, &str)]) {
    let conn = Connection::open(path).expect("create portal db");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal_index (
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
        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_issn
            ON journal_index(issn) WHERE issn IS NOT NULL AND issn != '';
        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_eissn
            ON journal_index(eissn) WHERE eissn IS NOT NULL AND eissn != '';
        ",
    )
    .expect("create portal schema");

    for (id, title, issn, eissn) in rows {
        conn.execute(
            "INSERT INTO journal_index (id, journal_title, issn, eissn, is_system, source_file)
             VALUES (?1, ?2, ?3, ?4, 1, 'test.csv')",
            rusqlite::params![id, title, issn, eissn],
        )
        .expect("insert portal row");
    }
    drop(conn);
}

/// Helper: create an empty main DB with the journal_index table (mimics post-migration state).
fn create_main_db() -> Connection {
    let conn = Connection::open_in_memory().expect("create main db");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS journal_index (
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
        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_issn
            ON journal_index(issn) WHERE issn IS NOT NULL AND issn != '';
        CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_eissn
            ON journal_index(eissn) WHERE eissn IS NOT NULL AND eissn != '';
        ",
    )
    .expect("create main schema");
    conn
}

/// Simulates the load_journal_index_if_empty logic using ATTACH + INSERT.
fn load_from_portal(
    conn: &Connection,
    portal_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if already populated
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let portal_path_str = portal_path.to_string_lossy().to_string();
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS portal;",
        portal_path_str.replace('\'', "''")
    ))?;

    conn.execute_batch(
        "INSERT INTO journal_index
            (id, journal_title, issn, eissn, publisher_name,
             publisher_address, languages, web_of_science_categories,
             is_system, source_file, created_at, updated_at)
         SELECT
            id, journal_title, issn, eissn, publisher_name,
            publisher_address, languages, web_of_science_categories,
            1, source_file, created_at, updated_at
         FROM portal.journal_index;
         DETACH DATABASE portal;",
    )?;

    Ok(())
}

#[test]
fn test_journal_index_load_from_portal_db() {
    let dir = tempfile::tempdir().expect("temp dir");
    let portal_path = dir.path().join("journal_index.db");

    create_portal_db(
        &portal_path,
        &[
            ("j1", "Nature", "1234-5678", "9876-5432"),
            ("j2", "Science", "2345-6789", "8765-4321"),
            ("j3", "Cell", "3456-7890", ""),
        ],
    );

    let main = create_main_db();
    load_from_portal(&main, &portal_path).expect("load should succeed");

    let count: i64 =
        main.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 3, "should have loaded 3 journal records");

    // Verify data integrity
    let title: String = main
        .query_row("SELECT journal_title FROM journal_index WHERE id = 'j1'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(title, "Nature");

    let issn: String = main
        .query_row("SELECT issn FROM journal_index WHERE id = 'j2'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(issn, "2345-6789");

    // Verify is_system flag
    let is_system: i64 = main
        .query_row("SELECT is_system FROM journal_index WHERE id = 'j3'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(is_system, 1, "loaded records should be marked is_system = 1");
}

#[test]
fn test_journal_index_skip_if_already_populated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let portal_path = dir.path().join("journal_index.db");

    create_portal_db(&portal_path, &[("j1", "Nature", "1234-5678", "9876-5432")]);

    let main = create_main_db();

    // Pre-populate with a record
    main.execute(
        "INSERT INTO journal_index (id, journal_title, issn, eissn, is_system)
         VALUES ('existing', 'Existing Journal', '1111-1111', '', 0)",
        [],
    )
    .unwrap();

    load_from_portal(&main, &portal_path).expect("load should succeed");

    let count: i64 =
        main.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1, "should still have only 1 record (skipped load)");

    let title: String = main
        .query_row("SELECT journal_title FROM journal_index WHERE id = 'existing'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(title, "Existing Journal");
}

#[test]
fn test_journal_index_load_from_bundled_resource() {
    // Test against the actual bundled portal DB in src-tauri/resources/
    let portal_path = std::path::PathBuf::from("../src-tauri/resources/journal_index.db");

    if !portal_path.exists() {
        eprintln!("SKIP: bundled portal DB not found at {:?}", portal_path);
        return;
    }

    let main = create_main_db();
    load_from_portal(&main, &portal_path).expect("load from bundled portal DB should succeed");

    let count: i64 =
        main.query_row("SELECT COUNT(*) FROM journal_index", [], |row| row.get(0)).unwrap();
    assert!(count > 0, "bundled portal DB should contain journal records (got {count})");

    // Sanity check: first record should have a non-empty title
    let first_title: String = main
        .query_row("SELECT journal_title FROM journal_index LIMIT 1", [], |row| row.get(0))
        .unwrap();
    assert!(!first_title.trim().is_empty(), "journal title should not be empty");
}
