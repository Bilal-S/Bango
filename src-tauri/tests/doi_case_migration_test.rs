//! Migration v009 (DOI canonicalization) integration tests.
//!
//! Simulates a legacy v8 database - old BINARY unique DOI index plus
//! mixed-case, prefixed, whitespace-wrapped, and placeholder DOI data - then
//! applies v009 through the real transactional runner and asserts healing,
//! duplicate merging with match-state preservation, counter recount, index
//! behavior, and idempotency. Inventory: `docs/test-plans/doi-case-tests.md`.

use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::migrations::v009_doi_canonicalization;
use rusqlite::Connection;

/// Rewind a fully-migrated DB to the v8 state (old BINARY partial DOI index)
/// so legacy mixed-shape values can be inserted before re-running v009.
/// Leaves `PRAGMA foreign_keys = OFF` for legacy seeding.
fn rewind_to_v8(conn: &Connection) {
    conn.execute_batch(
        "DROP INDEX uq_ref_papers_doi;
         CREATE UNIQUE INDEX uq_ref_papers_doi
             ON reference_papers(doi) WHERE doi IS NOT NULL;
         PRAGMA foreign_keys = OFF;",
    )
    .expect("rewind to v8 schema");
    conn.pragma_update(None, "user_version", 8).expect("rewind user_version");
}

/// Build a legacy-shaped DB: full migration chain applied, then rewound to
/// the v8 state (old BINARY DOI unique index) with mixed-case data inserted.
fn legacy_db() -> Connection {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    rewind_to_v8(&conn);

    // Legacy articles: mixed case, URL/scheme prefixes, whitespace, placeholder.
    conn.execute_batch(
        "INSERT INTO articles (id, title, abstract_text, authors, doi) VALUES
            ('a1', 'Article One',   'Abstract one',   '[]', '10.1/AbC'),
            ('a2', 'Article Two',   'Abstract two',   '[]', 'https://doi.org/10.2/x'),
            ('a3', 'Article Three', 'Abstract three', '[]', 'doi:10.3/Y'),
            ('a4', 'Article Four',  'Abstract four',  '[]', ' NA '),
            ('a5', 'Article Five',  'Abstract five',  '[]', 'HTTPS://DX.DOI.ORG/10.5/Qq');",
    )
    .expect("seed legacy articles");

    // Legacy reference papers: rp2 is a matched case-variant of rp1's DOI
    // (better rank, higher rowid); rp4 prefix+wraps rp3's DOI; rp7 is a
    // placeholder.
    conn.execute_batch(
        "INSERT INTO reference_papers (id, title, authors, doi, match_status, matched_article_id)
         VALUES
            ('rp1', 'Paper One',   '[\"A\"]', '10.1/abc', 'unmatched', NULL),
            ('rp2', 'Paper One B', '[\"B\"]', '10.1/AbC', 'matched',   'a2'),
            ('rp3', 'Paper Three', '[\"C\"]', '10.9/x',  'unmatched', NULL),
            ('rp4', 'Paper Four',  '[\"D\"]', ' https://DOI.org/10.9/x ', 'unmatched', NULL),
            ('rp7', 'Placeholder', '[\"E\"]', 'NA',      'unmatched', NULL);",
    )
    .expect("seed legacy papers");

    // Links: l1/l2 collide after remap (same parent + type); l3 remaps cleanly.
    conn.execute_batch(
        "INSERT INTO article_reference_links (id, parent_article_id, reference_paper_id, type)
         VALUES
            ('l1', 'a1', 'rp2', 1),
            ('l2', 'a1', 'rp1', 1),
            ('l3', 'a2', 'rp4', 0);",
    )
    .expect("seed legacy links");

    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");
    conn
}

#[test]
fn migration_heals_mixed_case_and_prefixed_dois() {
    let conn = legacy_db();
    run_migrations(&conn).expect("v009 must apply to the legacy DB");

    let article_doi = |id: &str| -> Option<String> {
        conn.query_row("SELECT doi FROM articles WHERE id = ?1", [id], |r| r.get(0))
            .expect("article row")
    };
    assert_eq!(article_doi("a1").as_deref(), Some("10.1/abc"), "mixed case must lowercase");
    assert_eq!(article_doi("a2").as_deref(), Some("10.2/x"), "https prefix must strip");
    assert_eq!(article_doi("a3").as_deref(), Some("10.3/y"), "doi: scheme prefix must strip");
    assert_eq!(article_doi("a4"), None, "placeholder must NULL out");
    assert_eq!(article_doi("a5").as_deref(), Some("10.5/qq"), "mixed-case dx prefix must strip");

    let rp3_doi: String = conn
        .query_row("SELECT doi FROM reference_papers WHERE id = 'rp3'", [], |r| r.get(0))
        .expect("rp3");
    assert_eq!(rp3_doi, "10.9/x", "prefixed + whitespace-wrapped paper DOI must heal");
}

#[test]
fn migration_merges_case_variant_duplicate_papers() {
    let conn = legacy_db();
    run_migrations(&conn).expect("v009 must apply");

    // rp1 (case variant of rp2) and rp4 (prefix variant of rp3) are gone;
    // rp2, rp3, and the placeholder rp7 survive.
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM reference_papers ORDER BY id")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(ids, vec!["rp2", "rp3", "rp7"], "case/prefix variants must merge");

    // l1 (already on the survivor) stays; l2 (collision with l1 after remap)
    // is absorbed + deleted; l3 remaps from rp4 to the survivor rp3.
    let links: Vec<(String, String, i64)> = conn
        .prepare("SELECT id, reference_paper_id, type FROM article_reference_links ORDER BY id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(links, vec![("l1".into(), "rp2".into(), 1), ("l3".into(), "rp3".into(), 0)]);
}

#[test]
fn migration_merge_preserves_match_state_and_counts() {
    let conn = legacy_db();
    run_migrations(&conn).expect("v009 must apply");

    // rp2 (matched, higher rowid) must win the 10.1/abc group over rp1
    // (unmatched, lower rowid): rank beats insertion order.
    let (status, matched_article): (String, Option<String>) = conn
        .query_row(
            "SELECT match_status, matched_article_id FROM reference_papers WHERE id = 'rp2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("rp2");
    assert_eq!(status, "matched", "survivor keeps its matched status");
    assert_eq!(matched_article.as_deref(), Some("a2"), "matched_article_id survives the merge");

    // Counters are recounted from the surviving links.
    let counts = |id: &str| -> (i64, i64) {
        conn.query_row(
            "SELECT citation_count, reference_count FROM reference_papers WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts")
    };
    assert_eq!(counts("rp2"), (0, 1), "rp2 keeps exactly its 1 reference link");
    assert_eq!(counts("rp3"), (1, 0), "rp3 counts the remapped citation link");
}

#[test]
fn migration_rebuilds_doi_index_case_insensitive() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");

    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, doi) VALUES ('r1', 'T', '[]', '10.1/abc')",
        [],
    )
    .expect("canonical insert");

    // A case-variant DOI must now violate the LOWER(doi) unique index.
    let err = conn
        .execute(
            "INSERT INTO reference_papers (id, title, authors, doi) VALUES ('r2', 'T2', '[]', '10.1/ABC')",
            [],
        )
        .expect_err("case-variant insert must violate the unique index");
    assert!(err.to_string().contains("UNIQUE"), "expected UNIQUE constraint failure, got: {err}");
}

#[test]
fn migration_idempotent_on_canonical_data() {
    let conn = legacy_db();
    run_migrations(&conn).expect("v009 must apply");

    let snapshot = |conn: &Connection| -> (Vec<(String, Option<String>)>, Vec<(String, String)>) {
        let papers: Vec<(String, Option<String>)> = conn
            .prepare("SELECT id, doi FROM reference_papers ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let links: Vec<(String, String)> = conn
            .prepare("SELECT id, reference_paper_id FROM article_reference_links ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        (papers, links)
    };

    let before = snapshot(&conn);
    conn.execute_batch(v009_doi_canonicalization::UP_SQL)
        .expect("re-running UP_SQL on canonical data must succeed");
    let after = snapshot(&conn);
    assert_eq!(before, after, "re-run must change nothing");
}

// ─── Healing equivalence with the Rust helper (doifixes2 findings 2a-2d) ───

#[test]
fn migration_nulls_prefix_placeholder_dois() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    rewind_to_v8(&conn);
    conn.execute_batch(
        "INSERT INTO articles (id, title, abstract_text, authors, doi) VALUES
            ('p1', 'P1', 'A', '[]', 'doi: NA'),
            ('p2', 'P2', 'A', '[]', 'doi: -');
         INSERT INTO reference_papers (id, title, authors, doi) VALUES
            ('rp-p1', 'P1', '[]', 'doi: N/A');",
    )
    .expect("seed prefix placeholders");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    run_migrations(&conn).expect("v009 must null prefix placeholders");

    // Placeholders behind a scheme prefix filter to NULL (strip-then-filter
    // order, matching `ris::doi::normalize_doi`), not 'doi: na'.
    for (table, id) in [("articles", "p1"), ("articles", "p2"), ("reference_papers", "rp-p1")] {
        let doi: Option<String> = conn
            .query_row(&format!("SELECT doi FROM {table} WHERE id = ?1"), [id], |r| r.get(0))
            .expect("query");
        assert_eq!(doi, None, "{table}.{id} prefix placeholder must heal to NULL");
    }
}

#[test]
fn migration_heals_double_prefixed_doi() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    rewind_to_v8(&conn);
    conn.execute_batch(
        "INSERT INTO articles (id, title, abstract_text, authors, doi) VALUES
            ('d1', 'D1', 'A', '[]', 'https://doi.org/doi:10.1/x'),
            ('d2', 'D2', 'A', '[]', 'doi:https://doi.org/10.2/y');",
    )
    .expect("seed double prefixes");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    run_migrations(&conn).expect("v009 must apply");

    // Exactly ONE strip with the helper's precedence: URL prefixes beat
    // `doi:`, so d1 keeps the inner scheme; d2 strips only the scheme.
    let d1: String =
        conn.query_row("SELECT doi FROM articles WHERE id = 'd1'", [], |r| r.get(0)).expect("d1");
    assert_eq!(d1, "doi:10.1/x", "URL prefix strip must not cascade to the inner scheme");
    let d2: String =
        conn.query_row("SELECT doi FROM articles WHERE id = 'd2'", [], |r| r.get(0)).expect("d2");
    assert_eq!(d2, "https://doi.org/10.2/y", "scheme strip must not cascade to the inner URL");
}

#[test]
fn migration_heals_multispace_scheme_separator() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    rewind_to_v8(&conn);
    conn.execute(
        "INSERT INTO articles (id, title, abstract_text, authors, doi)
         VALUES ('m1', 'M1', 'A', '[]', 'doi:  10.1/x')",
        [],
    )
    .expect("seed multispace");
    conn.execute("PRAGMA foreign_keys = ON", []).expect("re-enable fk");

    run_migrations(&conn).expect("v009 must apply");
    let doi: String =
        conn.query_row("SELECT doi FROM articles WHERE id = 'm1'", [], |r| r.get(0)).expect("m1");
    assert_eq!(doi, "10.1/x", "any whitespace run after the scheme must trim clean");
}

#[test]
fn doi_lookup_uses_lower_doi_index() {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");
    conn.execute(
        "INSERT INTO reference_papers (id, title, authors, doi) VALUES ('r1', 'T', '[]', '10.1/abc')",
        [],
    )
    .expect("seed paper");

    let plan = |sql: &str| -> String {
        let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).expect("prepare explain");
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("plan rows")
            .collect::<Result<_, _>>()
            .expect("plan");
        rows.join(" | ")
    };

    // The three shipped lookup shapes must SEEK the LOWER(doi) index, not
    // SCAN. Guards the non-partial index: a `WHERE doi IS NOT NULL` partial
    // clause defeats SQLite's planner for expression equality.
    let shapes = [
        "SELECT * FROM reference_papers WHERE LOWER(doi) = LOWER('10.1/abc') LIMIT 1",
        "SELECT * FROM reference_papers WHERE LOWER(doi) = LOWER('10.1/abc') AND match_status = 'unmatched' LIMIT 1",
        "SELECT id FROM reference_papers WHERE LOWER(doi) = LOWER('10.1/abc') LIMIT 1",
    ];
    for sql in shapes {
        let p = plan(sql);
        assert!(p.contains("USING INDEX uq_ref_papers_doi"), "expected index seek, got: {p}");
        assert!(!p.contains("SCAN"), "DOI lookup must not full-scan, got: {p}");
    }
}
