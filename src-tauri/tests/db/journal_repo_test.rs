//! Coverage for db::journal_repo (match_journal, resolve_journal_id, get_journal_info).
use bango_lib::db::connection::create_connection;
use bango_lib::db::journal_repo;
use bango_lib::db::migration::run_migrations;
use rusqlite::params;

/// Insert a journal_index row directly.
fn add_journal(
    conn: &rusqlite::Connection,
    id: &str,
    title: &str,
    issn: Option<&str>,
    eissn: Option<&str>,
) {
    conn.execute(
        "INSERT INTO journal_index (id, journal_title, issn, eissn, is_system, source_file)
         VALUES (?1, ?2, ?3, ?4, 1, 'test.csv')",
        params![id, title, issn, eissn],
    )
    .expect("insert journal");
}

/// Mark an article as included and link it to a journal_index id.
fn link_included_article(
    conn: &rusqlite::Connection,
    id: &str,
    journal_index_id: &str,
    year: i32,
    cited: i64,
) {
    conn.execute(
        "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors,
            publication_year, journal_index_id, num_cited, data_length, token_estimate)
         VALUES (?1, 1, 'included', 't', '', '[]', ?2, ?3, ?4, 0, 0)",
        params![id, year, journal_index_id, cited],
    )
    .expect("insert article");
}

#[test]
fn match_journal_by_issn() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), Some("9876-5432"));

    let id = journal_repo::match_journal(&conn, Some("1234-5678"), None, None).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_by_eissn_when_issn_absent() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), Some("9876-5432"));

    let id = journal_repo::match_journal(&conn, None, Some("9876-5432"), None).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_by_eissn_when_issn_unknown() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), Some("9876-5432"));

    // ISSN doesn't match; falls through to eISSN
    let id = journal_repo::match_journal(&conn, Some("0000-0000"), Some("9876-5432"), None)
        .expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_by_name_case_insensitive_and_trimmed() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    let id = journal_repo::match_journal(&conn, None, None, Some("  nature  ")).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_returns_none_when_no_match() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), None);

    let id =
        journal_repo::match_journal(&conn, Some("9999-9999"), None, Some("Cell")).expect("match");
    assert!(id.is_none());
}

#[test]
fn match_journal_empty_strings_are_skipped() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    // Empty issn/eissn/name should all be skipped, falling to name match
    let id = journal_repo::match_journal(&conn, Some(""), Some(""), Some("Nature")).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));

    // All empty -> None
    let id2 = journal_repo::match_journal(&conn, Some(""), Some(""), Some("")).expect("match");
    assert!(id2.is_none());
}

#[test]
fn match_journal_all_none_returns_none() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    let id = journal_repo::match_journal(&conn, None, None, None).expect("match");
    assert!(id.is_none());
}

#[test]
fn resolve_journal_id_wraps_match() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), None);

    assert_eq!(
        journal_repo::resolve_journal_id(&conn, Some("1234-5678"), None, None),
        Some("j1".to_string())
    );
    // No match returns None (never errors)
    assert_eq!(journal_repo::resolve_journal_id(&conn, Some("0000-0000"), None, None), None);
}

#[test]
fn get_journal_info_returns_none_for_unknown_id() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");

    let info = journal_repo::get_journal_info(&conn, "does-not-exist").expect("info");
    assert!(info.is_none());
}

#[test]
fn get_journal_info_returns_metadata_and_aggregates() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), Some("9876-5432"));

    // Two included articles linked to this journal across years
    link_included_article(&conn, "a1", "j1", 2020, 10);
    link_included_article(&conn, "a2", "j1", 2022, 5);

    let info = journal_repo::get_journal_info(&conn, "j1").expect("info").expect("some");
    assert_eq!(info.id, "j1");
    assert_eq!(info.journal_title, "Nature");
    assert_eq!(info.issn.as_deref(), Some("1234-5678"));
    assert_eq!(info.eissn.as_deref(), Some("9876-5432"));
    assert_eq!(info.article_count, 2);
    assert_eq!(info.first_year, Some(2020));
    assert_eq!(info.last_year, Some(2022));
    assert_eq!(info.citations_total, 15);
    assert_eq!(info.pubs_by_year.len(), 2);
    assert_eq!(info.pubs_by_year[0].year, 2020);
    assert_eq!(info.pubs_by_year[0].count, 1);
    assert_eq!(info.pubs_by_year[1].year, 2022);
    assert_eq!(info.pubs_by_year[1].count, 1);
}

#[test]
fn get_journal_info_excludes_non_included_articles() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    // A working (not included) article linked to the journal
    conn.execute(
        "INSERT INTO articles (id, sequence_id, status, title, abstract_text, authors,
            publication_year, journal_index_id, num_cited, data_length, token_estimate)
         VALUES ('a1', 1, 'working', 't', '', '[]', 2020, 'j1', 99, 0, 0)",
        [],
    )
    .expect("insert working");

    let info = journal_repo::get_journal_info(&conn, "j1").expect("info").expect("some");
    assert_eq!(info.article_count, 0, "working articles excluded");
    assert_eq!(info.citations_total, 0);
    assert!(info.pubs_by_year.is_empty());
    assert_eq!(info.first_year, None);
}

// ── normalize_issn (pure helper) ───────────────────────────────────────

#[test]
fn normalize_issn_strips_ebsco_suffix() {
    assert_eq!(journal_repo::normalize_issn("13665545 (ISSN)"), "1366-5545");
}

#[test]
fn normalize_issn_handles_semicolon_suffix() {
    assert_eq!(journal_repo::normalize_issn("0378-5955 ; Print"), "0378-5955");
}

#[test]
fn normalize_issn_inserts_hyphen_in_unhyphenated() {
    assert_eq!(journal_repo::normalize_issn("12345678"), "1234-5678");
}

#[test]
fn normalize_issn_passes_clean_through() {
    assert_eq!(journal_repo::normalize_issn("2572-3170"), "2572-3170");
}

#[test]
fn normalize_issn_preserves_trailing_x_check_digit() {
    assert_eq!(journal_repo::normalize_issn("1234-567X"), "1234-567X");
    assert_eq!(journal_repo::normalize_issn("1234567x"), "1234-567X");
}

#[test]
fn normalize_issn_rejects_isbn_length() {
    assert_eq!(journal_repo::normalize_issn("9783161484100"), "");
}

#[test]
fn normalize_issn_rejects_garbage() {
    assert_eq!(journal_repo::normalize_issn("not-an-issn"), "");
    assert_eq!(journal_repo::normalize_issn("123"), "");
}

#[test]
fn normalize_issn_empty_and_whitespace() {
    assert_eq!(journal_repo::normalize_issn(""), "");
    assert_eq!(journal_repo::normalize_issn("   "), "");
}

// ── normalize_journal_name (pure helper) ───────────────────────────────

#[test]
fn normalize_journal_name_folds_ampersand_to_and() {
    assert_eq!(
        journal_repo::normalize_journal_name("Production & Operations Management"),
        journal_repo::normalize_journal_name("Production and Operations Management")
    );
}

#[test]
fn normalize_journal_name_strips_parenthetical_issn_suffix() {
    assert_eq!(journal_repo::normalize_journal_name("Sensors (2076-3387)"), "sensors");
}

#[test]
fn normalize_journal_name_folds_colon_and_dash() {
    // "Transportation Research Part E: Logistics" normalizes so the colon and
    // dash become spaces, then whitespace collapses.
    let a = journal_repo::normalize_journal_name("Transportation Research Part E: Logistics");
    assert!(a.contains("logistics"));
    assert!(!a.contains(':'));
}

// ── match_journal: ISSN cross-check (Bug A) ────────────────────────────

#[test]
fn match_journal_cross_check_issn_in_eissn() {
    // Article has issn=2572-3170 but the journal_index row stores it in the
    // eissn column (issn is NULL). Before the fix this failed; the cross-check
    // tier now finds it.
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Business Strategy & Development", None, Some("2572-3170"));

    let id = journal_repo::match_journal(&conn, Some("2572-3170"), None, None).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_cross_check_eissn_in_issn() {
    // Article has eissn=1059-1478 but the journal_index row stores it in issn.
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Production and Operations Management", Some("1059-1478"), None);

    let id = journal_repo::match_journal(&conn, None, Some("1059-1478"), None).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

// ── match_journal: ISSN normalization (Bug B) ──────────────────────────

#[test]
fn match_journal_normalizes_ebsco_dirty_issn() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Journal of Clinical Nursing", Some("1366-5545"), None);

    // Article ISSN is the raw EBSCO form with `(ISSN)` suffix + no hyphen.
    let id =
        journal_repo::match_journal(&conn, Some("13665545 (ISSN)"), None, None).expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

// ── match_journal: symbol-safe name tier (Bug C) ───────────────────────

#[test]
fn match_journal_symbol_safe_name_ampersand() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Production and Operations Management", None, None);

    // Article journal uses `&`; index uses `and`. Must match via the
    // normalize_journal_name equality tier.
    let id =
        journal_repo::match_journal(&conn, None, None, Some("Production & Operations Management"))
            .expect("match");
    assert_eq!(id.as_deref(), Some("j1"));
}

#[test]
fn match_journal_no_substring_fallback_for_similar_names() {
    // The automatic path must NOT silently pick the wrong journal among
    // similar names. Two journals share the "Journal of Health Economics"
    // prefix; a query for the shorter title must return None rather than
    // linking the longer one (substring matching is reserved for the
    // interactive `search_journal_index`).
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Journal of Health Economics", None, None);
    add_journal(&conn, "j2", "Journal of Health Economics and Policy", None, None);

    let id = journal_repo::match_journal(&conn, None, None, Some("Journal of Health Economics"))
        .expect("match");
    // Exact normalized equality still matches j1.
    assert_eq!(id.as_deref(), Some("j1"));

    // A query that is NOT exactly equal to either title must not match.
    let id2 = journal_repo::match_journal(&conn, None, None, Some("Journal of Health Econ"))
        .expect("match");
    assert!(id2.is_none(), "automatic path must not substring-match");
}

// ── search_journal_index (interactive autocomplete) ────────────────────

#[test]
fn search_journal_index_issn_match() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", Some("1234-5678"), Some("9876-5432"));

    let rows = journal_repo::search_journal_index(&conn, "1234-5678", None).expect("search");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "j1");
    assert_eq!(rows[0].journal_title, "Nature");
}

#[test]
fn search_journal_index_partial_name() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature Communications", None, None);
    add_journal(&conn, "j2", "Nature Methods", None, None);

    // "Communic" is >= 4 chars, so the LIKE tier fires and returns candidates.
    let rows = journal_repo::search_journal_index(&conn, "Communic", None).expect("search");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "j1");
}

#[test]
fn search_journal_index_short_query_noop() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    // 1-char query: not an ISSN, below the LIKE min length -> empty.
    let rows = journal_repo::search_journal_index(&conn, "N", None).expect("search");
    assert!(rows.is_empty());
}

#[test]
fn search_journal_index_no_match_returns_empty() {
    let conn = create_connection().expect("conn");
    run_migrations(&conn).expect("migrate");
    add_journal(&conn, "j1", "Nature", None, None);

    let rows = journal_repo::search_journal_index(&conn, "Quantum Journal of Nothing", None)
        .expect("search");
    assert!(rows.is_empty());
}
