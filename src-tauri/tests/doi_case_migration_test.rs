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

/// Build a legacy-shaped DB: full migration chain applied, then rewound to
/// the v8 state (old BINARY DOI unique index) with mixed-case data inserted.
fn legacy_db() -> Connection {
    let conn = create_connection().expect("connection");
    run_migrations(&conn).expect("migrations");

    // Rewind to the v8 schema state: restore the old BINARY unique index so
    // legacy mixed-case variants can be inserted.
    conn.execute_batch(
        "DROP INDEX uq_ref_papers_doi;
         CREATE UNIQUE INDEX uq_ref_papers_doi
             ON reference_papers(doi) WHERE doi IS NOT NULL;
         PRAGMA foreign_keys = OFF;",
    )
    .expect("rewind to v8 schema");
    conn.pragma_update(None, "user_version", 8).expect("rewind user_version");

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
