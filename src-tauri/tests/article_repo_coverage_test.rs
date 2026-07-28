//! Pre-refactor coverage tests for `article_repo` functions that have no
//! direct test today (only indirect coverage through command callers).
//!
//! These tests pin behavior BEFORE the v6 refactor splits `article_repo.rs`
//! into a directory module, so any split-time breakage (wrong `pub` visibility,
//! missed re-export, dropped helper) is caught immediately. They complement
//! the existing focused test files (`article_delete_test.rs`,
//! `article_metadata_test.rs`, `article_query_test.rs`,
//! `bulk_tag_label_test.rs`, `status_transition_screening_flags_test.rs`).
//!
//! Coverage targets (each is a public fn with zero existing direct test):
//! - `get_article_field_count` - drives dedup survivor selection
//! - `resolve_journal_links` - post-import journal_index_id backfill
//! - `rematch_all_journals` - bulk re-derivation + idempotency
//! - `override_ai_decision` - status + ai_decision + audit entry
//! - `update_article_criteria` - matched_inc/exc JSON round-trip
//! - `bulk_update_article_status` returns affected count

use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: insert one article (default 'duplicate' status) and return its id.
fn seed_article(conn: &rusqlite::Connection, title: &str) -> String {
    let article = NewArticle { title: title.to_string(), ..Default::default() };
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    inserted.id
}

// ─── get_article_field_count ──────────────────────────────────────────────

#[test]
fn get_article_field_count_counts_non_empty_fields() {
    // Drives dedup survivor selection: the article with the highest non-null
    // field count wins. Verifies the per-field counter that the dedup engine
    // relies on.
    let conn = setup_db();

    // Article with minimal metadata: only title + abstract (abstract_text is
    // NOT NULL, defaults to empty; empty does NOT count). So a bare article
    // has 0 populated optional fields.
    let id_minimal = seed_article(&conn, "Minimal");
    let count_minimal = article_repo::get_article_field_count(&conn, &id_minimal)
        .expect("get_article_field_count failed");
    assert_eq!(
        count_minimal, 0,
        "bare article (no abstract, no optional fields) should have count 0"
    );

    // Article with several populated optional fields.
    let rich = NewArticle {
        title: "Rich".to_string(),
        abstract_text: "Has an abstract.".to_string(),
        doi: Some("10.1001/test.2024".to_string()),
        journal: Some("Nature".to_string()),
        publication_year: Some(2024),
        language: Some("English".to_string()),
        keywords: vec!["obesity".to_string()],
        ..Default::default()
    };
    let inserted_rich = article_repo::insert_article(&conn, &rich).expect("insert rich failed");
    let count_rich = article_repo::get_article_field_count(&conn, &inserted_rich.id)
        .expect("get_article_field_count failed");
    // doi + journal + publication_year + language + keywords + abstract_text = 6.
    assert_eq!(
        count_rich, 6,
        "rich article should count 6 non-empty fields (doi, journal, year, lang, keywords, abstract)"
    );
}

// ─── override_ai_decision ──────────────────────────────────────────────────

#[test]
fn override_ai_decision_writes_decision_status_and_audit() {
    let conn = setup_db();
    let id = seed_article(&conn, "Override Me");
    article_repo::move_to_working(&conn, &id).expect("move to working");

    // Override to include.
    article_repo::override_ai_decision(
        &conn,
        &id,
        "include",
        "included",
        Some("User disagreed with AI exclude"),
    )
    .expect("override failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(
        article.ai_decision.as_ref().map(bango_lib::models::article::AiDecision::as_str),
        Some("include")
    );
    assert_eq!(article.status.as_str(), "included");
    assert_eq!(article.ai_reasoning.as_deref(), Some("User disagreed with AI exclude"));
    assert!(article.manual_override, "manual_override flag must be set");

    // Audit entry: action = 'manual_override', source = 'user'.
    let audit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_entries WHERE article_id = ?1 AND action = 'manual_override' AND source = 'user'",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .expect("audit count query failed");
    assert_eq!(audit_count, 1, "exactly one manual_override audit entry should exist");
}

// ─── update_article_criteria ───────────────────────────────────────────────

#[test]
fn update_article_criteria_round_trips_matched_ids() {
    let conn = setup_db();
    let id = seed_article(&conn, "Criteria Article");
    article_repo::move_to_working(&conn, &id).expect("move to working");

    let inc = vec!["inc-uuid-1".to_string(), "inc-uuid-2".to_string()];
    let exc = vec!["exc-uuid-1".to_string()];
    article_repo::update_article_criteria(&conn, &id, &inc, &exc).expect("update failed");

    let article = article_repo::get_article_by_id(&conn, &id).expect("get failed");
    assert_eq!(article.matched_inclusion_criteria, inc);
    assert_eq!(article.matched_exclusion_criteria, exc);
}

// ─── resolve_journal_links ────────────────────────────────────────────────

#[test]
fn resolve_journal_links_skips_non_jour_and_already_linked() {
    let conn = setup_db();

    // Seed a journal_index row so a JOUR article can match it, plus a second
    // row referenced by the already-linked article (the FK requires a real row).
    conn.execute(
        "INSERT INTO journal_index (id, journal_title, issn, eissn, publisher_name, is_system) \
         VALUES ('ji-1', 'The Lancet', '0099-5355', '1528-4050', 'Elsevier', 1)",
        [],
    )
    .expect("seed journal_index failed");
    conn.execute(
        "INSERT INTO journal_index (id, journal_title, is_system) \
         VALUES ('pre-existing', 'Pre-existing Journal', 1)",
        [],
    )
    .expect("seed pre-existing journal_index failed");

    // JOUR article with matching ISSN -> should be linked.
    let jour = NewArticle {
        title: "Journal article".to_string(),
        reference_type: Some("JOUR".to_string()),
        issn: Some("0099-5355".to_string()),
        ..Default::default()
    };
    let jour_inserted = article_repo::insert_article(&conn, &jour).expect("insert JOUR failed");
    article_repo::move_to_working(&conn, &jour_inserted.id).expect("move JOUR");

    // Non-JOUR article with the same ISSN -> should be skipped.
    let book = NewArticle {
        title: "Book chapter".to_string(),
        reference_type: Some("CHAP".to_string()),
        issn: Some("0099-5355".to_string()),
        ..Default::default()
    };
    let book_inserted = article_repo::insert_article(&conn, &book).expect("insert CHAP failed");
    article_repo::move_to_working(&conn, &book_inserted.id).expect("move CHAP");

    // Article already linked (journal_index_id preset) -> should be skipped.
    let linked = NewArticle {
        title: "Already linked".to_string(),
        reference_type: Some("JOUR".to_string()),
        issn: Some("0099-5355".to_string()),
        journal_index_id: Some("pre-existing".to_string()),
        ..Default::default()
    };
    let linked_inserted =
        article_repo::insert_article(&conn, &linked).expect("insert linked failed");
    article_repo::move_to_working(&conn, &linked_inserted.id).expect("move linked");

    // Resolve over all three.
    let articles = article_repo::get_all_articles(&conn).expect("get_all failed for resolve input");
    let resolved = article_repo::resolve_journal_links(&conn, &articles);
    assert_eq!(resolved, 1, "only the unlinked JOUR article should be resolved");

    // Verify the JOUR article now has a link, the CHAP article does not, and
    // the already-linked article is unchanged.
    let jour_after = article_repo::get_article_by_id(&conn, &jour_inserted.id).expect("get JOUR");
    assert_eq!(jour_after.journal_index_id.as_deref(), Some("ji-1"));

    let book_after = article_repo::get_article_by_id(&conn, &book_inserted.id).expect("get CHAP");
    assert!(book_after.journal_index_id.is_none(), "non-JOUR must not be linked");

    let linked_after =
        article_repo::get_article_by_id(&conn, &linked_inserted.id).expect("get linked");
    assert_eq!(
        linked_after.journal_index_id.as_deref(),
        Some("pre-existing"),
        "already-linked article must be untouched"
    );
}

// ─── rematch_all_journals ──────────────────────────────────────────────────

#[test]
fn rematch_all_journals_links_eligible_articles_and_is_idempotent() {
    let conn = setup_db();

    // Seed a journal_index row.
    conn.execute(
        "INSERT INTO journal_index (id, journal_title, issn, eissn, publisher_name, is_system) \
         VALUES ('ji-rematch', 'Nature', '0028-0836', '1476-4687', 'Springer Nature', 1)",
        [],
    )
    .expect("seed journal_index failed");

    // Eligible JOUR article: no journal_index_id, matching ISSN.
    let eligible = NewArticle {
        title: "Eligible".to_string(),
        reference_type: Some("JOUR".to_string()),
        issn: Some("0028-0836".to_string()),
        ..Default::default()
    };
    let eligible_id = article_repo::insert_article(&conn, &eligible).expect("insert eligible").id;

    // Ineligible: no JOUR type (should be skipped by the WHERE clause).
    let _ineligible = NewArticle {
        title: "Book".to_string(),
        reference_type: Some("CHAP".to_string()),
        issn: Some("0028-0836".to_string()),
        ..Default::default()
    };
    article_repo::insert_article(&conn, &_ineligible).expect("insert ineligible");

    // First rematch: should link exactly 1 (the eligible JOUR article).
    let resolved = article_repo::rematch_all_journals(&conn).expect("rematch failed");
    assert_eq!(resolved, 1, "exactly one article should be newly linked");

    let after = article_repo::get_article_by_id(&conn, &eligible_id).expect("get eligible");
    assert_eq!(after.journal_index_id.as_deref(), Some("ji-rematch"));

    // Second rematch: idempotent - the now-linked article is filtered out by
    // `WHERE journal_index_id IS NULL`, so the count must be 0.
    let resolved_again = article_repo::rematch_all_journals(&conn).expect("rematch second");
    assert_eq!(resolved_again, 0, "rematch must be idempotent");
}

// ─── bulk_update_article_status: affected count ──────────────────────────

#[test]
fn bulk_update_article_status_returns_affected_count() {
    // The existing status_transition_screening_flags_test.rs covers the
    // screening-flags reset semantics; this test pins the return value
    // contract (number of rows actually updated) which the command layer
    // surfaces in the UI toast.
    let conn = setup_db();

    let ids: Vec<String> = (0..3)
        .map(|i| {
            let id = seed_article(&conn, &format!("A{}", i));
            article_repo::move_to_working(&conn, &id).expect("move to working");
            id
        })
        .collect();

    // Bulk move to included: all 3 rows updated.
    let affected =
        article_repo::bulk_update_article_status(&conn, &ids, "included").expect("bulk update");
    assert_eq!(affected, 3, "all 3 articles should be updated");

    // Bulk move the same ids to included again: still 3 (UPDATE always matches
    // by id, regardless of the old status).
    let affected_again =
        article_repo::bulk_update_article_status(&conn, &ids, "included").expect("bulk again");
    assert_eq!(affected_again, 3);

    // Empty input -> 0 affected, no error.
    let empty = article_repo::bulk_update_article_status(&conn, &[], "working").expect("empty");
    assert_eq!(empty, 0);
}
