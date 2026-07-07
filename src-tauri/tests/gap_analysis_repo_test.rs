//! Integration tests for the Research Gap Analysis persistence layer.
//!
//! Mirrors the summary-repo pattern: in-memory SQLite via `run_migrations`,
//! save/get round-trip, clear -> absent. Also verifies the v004 migration
//! created the `gap_analysis` table and set `user_version = 4`.

use bango_lib::db::connection::create_connection;
use bango_lib::db::gap_analysis_repo;
use bango_lib::db::migration::run_migrations;

#[test]
fn save_get_round_trip() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Initially absent.
    assert!(gap_analysis_repo::get_gap_analysis(&conn).unwrap().is_none());

    // Save.
    gap_analysis_repo::save_gap_analysis(&conn, "# Gaps\n\nContent", "APA", "2026-07-07T00:00:00Z")
        .expect("save failed");

    // Read back.
    let saved =
        gap_analysis_repo::get_gap_analysis(&conn).expect("get failed").expect("row missing");
    assert_eq!(saved.gap_text, "# Gaps\n\nContent");
    assert_eq!(saved.citation_style, "APA");
    assert_eq!(saved.generated_at, "2026-07-07T00:00:00Z");

    // Upsert: save again overwrites the single row.
    gap_analysis_repo::save_gap_analysis(
        &conn,
        "# New Gaps\n\nNew content",
        "MLA",
        "2026-07-08T00:00:00Z",
    )
    .expect("upsert save failed");
    let updated =
        gap_analysis_repo::get_gap_analysis(&conn).expect("get failed").expect("row missing");
    assert_eq!(updated.gap_text, "# New Gaps\n\nNew content");
    assert_eq!(updated.citation_style, "MLA");
    assert_eq!(updated.generated_at, "2026-07-08T00:00:00Z");
}

#[test]
fn clear_makes_row_absent() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    gap_analysis_repo::save_gap_analysis(&conn, "content", "APA", "ts").expect("save failed");
    assert!(gap_analysis_repo::get_gap_analysis(&conn).unwrap().is_some());

    gap_analysis_repo::clear_gap_analysis(&conn).expect("clear failed");
    assert!(gap_analysis_repo::get_gap_analysis(&conn).unwrap().is_none());
}

/// v004 must create the `gap_analysis` table and bump `user_version` to 4.
#[test]
fn migration_v004_creates_gap_analysis_table_and_sets_user_version() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // user_version must be 4 (v001 + v002 + v003 + v004).
    let version: i64 =
        conn.query_row("PRAGMA user_version", [], |row| row.get(0)).expect("PRAGMA failed");
    assert_eq!(version, 4, "user_version must be 4 after migrations v001-v004");

    // The gap_analysis table must exist.
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='gap_analysis'",
            [],
            |row| row.get(0),
        )
        .expect("Query failed");
    assert_eq!(exists, 1, "gap_analysis table must exist after v004");

    // Its schema must include the expected columns.
    for col in ["id", "gap_text", "citation_style", "generated_at"] {
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM pragma_table_info('gap_analysis') WHERE name='{col}'"
                ),
                [],
                |row| row.get(0),
            )
            .expect("pragma_table_info failed");
        assert_eq!(count, 1, "gap_analysis.{col} column must exist");
    }
}

/// The single-row CHECK constraint must reject a second row (id must be 1).
#[test]
fn gap_analysis_enforces_single_row() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Insert id=1 succeeds.
    conn.execute(
        "INSERT INTO gap_analysis (id, gap_text, citation_style, generated_at) VALUES (1, 'a', 'APA', 'ts')",
        [],
    )
    .expect("insert id=1 failed");

    // Insert id=2 must fail (CHECK constraint: id must be 1).
    let result = conn.execute(
        "INSERT INTO gap_analysis (id, gap_text, citation_style, generated_at) VALUES (2, 'b', 'APA', 'ts')",
        [],
    );
    assert!(result.is_err(), "CHECK(id = 1) must reject id=2");
}
