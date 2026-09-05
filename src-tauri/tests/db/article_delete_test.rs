use bango_lib::db::app_settings_repo;
use bango_lib::db::article_repo;
use bango_lib::db::article_repo::ArticleQuery;
use bango_lib::db::audit_repo;
use bango_lib::db::chunk_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::reference_repo;
use bango_lib::error::AppError;
use bango_lib::models::article::NewArticle;
use bango_lib::models::reference::{NewReferencePaper, ReferenceType};

/// Helper: create an in-memory DB with migrations applied.
fn setup_db() -> rusqlite::Connection {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");
    conn
}

/// Helper: insert one article and return its id.
fn seed_article(conn: &rusqlite::Connection, title: &str) -> String {
    let article = NewArticle { title: title.to_string(), ..Default::default() };
    let inserted = article_repo::insert_article(conn, &article).expect("insert failed");
    inserted.id
}

/// Helper: insert one reference paper and return its id.
fn seed_reference_paper(conn: &rusqlite::Connection, title: &str) -> String {
    let paper = NewReferencePaper { title: Some(title.to_string()), ..Default::default() };
    let (inserted, _was_created) =
        reference_repo::insert_or_find_paper(conn, &paper).expect("insert paper failed");
    inserted.id
}

/// Helper: count rows in a table matching the WHERE clause `clause` (empty
/// string = no WHERE). Returns the row count.
fn count_rows(conn: &rusqlite::Connection, table: &str, clause: &str) -> i64 {
    let sql = if clause.is_empty() {
        format!("SELECT COUNT(*) FROM {table}")
    } else {
        format!("SELECT COUNT(*) FROM {table} WHERE {clause}")
    };
    conn.query_row(&sql, [], |row| row.get(0))
        .unwrap_or_else(|_| panic!("Failed to count rows in {table}"))
}

/// Helper: fetch the `(match_status, matched_article_id, reference_count,
/// citation_count)` snapshot for a reference paper by id. Used instead of a
/// non-existent `get_reference_paper(id)` helper.
fn paper_status(conn: &rusqlite::Connection, paper_id: &str) -> (String, Option<String>, i64, i64) {
    conn.query_row(
        "SELECT match_status, matched_article_id, reference_count, citation_count \
         FROM reference_papers WHERE id = ?1",
        rusqlite::params![paper_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )
    .unwrap_or_else(|_| panic!("Failed to fetch paper {paper_id}"))
}

// ─── Basic deletion ─────────────────────────────────────────────────────

#[test]
fn delete_article_removes_row() {
    let conn = setup_db();
    let id = seed_article(&conn, "To Delete");

    article_repo::delete_article(&conn, &id).expect("delete failed");

    // The article row is gone.
    assert_eq!(count_rows(&conn, "articles", &format!("id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "articles", ""), 0);
}

#[test]
fn delete_nonexistent_article_returns_not_found() {
    let conn = setup_db();
    let err = article_repo::delete_article(&conn, "does-not-exist").unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)), "expected NotFound, got {err:?}");
}

// ─── Cascade cleanup ────────────────────────────────────────────────────

#[test]
fn delete_cascades_tags_labels_audit() {
    let conn = setup_db();
    let id = seed_article(&conn, "Tagged Article");

    // Attach tags + labels (via the repo so the junction rows exist).
    article_repo::update_article_tags(&conn, &id, &["alpha".into(), "beta".into()])
        .expect("tags failed");
    article_repo::update_article_labels(&conn, &id, &["important".into()]).expect("labels failed");
    // Add an audit entry.
    audit_repo::create_entry(
        &conn,
        &id,
        "status_change",
        None,
        Some("included"),
        Some("manual"),
        "user",
    )
    .expect("audit failed");

    // Sanity: rows exist before delete.
    assert_eq!(count_rows(&conn, "article_tags", &format!("article_id = '{id}'")), 2);
    assert_eq!(count_rows(&conn, "article_labels", &format!("article_id = '{id}'")), 1);
    assert_eq!(count_rows(&conn, "audit_entries", &format!("article_id = '{id}'")), 1); // only status_change (single insert does not write an import audit)

    article_repo::delete_article(&conn, &id).expect("delete failed");

    // ON DELETE CASCADE cleared the junction + audit rows.
    assert_eq!(count_rows(&conn, "article_tags", &format!("article_id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "article_labels", &format!("article_id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "audit_entries", &format!("article_id = '{id}'")), 0);
}

#[test]
fn delete_cascades_chunks_and_translation_archive() {
    let conn = setup_db();
    let id = seed_article(&conn, "Full Text Article");

    // Insert a chunk row directly (mimics attach_full_text chunk population).
    conn.execute(
        "INSERT INTO article_chunks (article_id, chunk_index, section, content, word_count) \
         VALUES (?1, 0, 'Methods', 'some content', 2)",
        rusqlite::params![id],
    )
    .expect("insert chunk failed");
    // Insert an original-content archive row (v003 translation archive).
    conn.execute(
        "INSERT INTO article_original_content (article_id, original_title, source_language) \
         VALUES (?1, 'Titre', 'French')",
        rusqlite::params![id],
    )
    .expect("insert original content failed");

    assert_eq!(count_rows(&conn, "article_chunks", &format!("article_id = '{id}'")), 1);
    assert_eq!(count_rows(&conn, "article_original_content", &format!("article_id = '{id}'")), 1);

    article_repo::delete_article(&conn, &id).expect("delete failed");

    // ON DELETE CASCADE cleared the chunks + original content archive.
    assert_eq!(count_rows(&conn, "article_chunks", &format!("article_id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "article_original_content", &format!("article_id = '{id}'")), 0);
}

#[test]
fn delete_cascades_biblio_article_authors_terms() {
    let conn = setup_db();
    let id = seed_article(&conn, "Biblio Article");

    // Insert a biblio author + a biblio_article_authors link (CASCADE on article_id).
    let author_id = "auth-1";
    conn.execute(
        "INSERT INTO biblio_authors (id, normalized_name, display_name) VALUES (?1, ?2, ?3)",
        rusqlite::params![author_id, "smith j", "Smith J"],
    )
    .expect("insert biblio author failed");
    conn.execute(
        "INSERT INTO biblio_article_authors (article_id, author_id, author_order) VALUES (?1, ?2, 0)",
        rusqlite::params![id, author_id],
    )
    .expect("insert biblio_article_authors failed");
    // Insert a biblio term + link.
    let term_id = "term-1";
    conn.execute(
        "INSERT INTO biblio_terms (id, normalized_term, raw_term) VALUES (?1, ?2, ?3)",
        rusqlite::params![term_id, "obesity", "obesity"],
    )
    .expect("insert biblio term failed");
    conn.execute(
        "INSERT INTO biblio_article_terms (article_id, term_id) VALUES (?1, ?2)",
        rusqlite::params![id, term_id],
    )
    .expect("insert biblio_article_terms failed");

    assert_eq!(count_rows(&conn, "biblio_article_authors", &format!("article_id = '{id}'")), 1);
    assert_eq!(count_rows(&conn, "biblio_article_terms", &format!("article_id = '{id}'")), 1);

    article_repo::delete_article(&conn, &id).expect("delete failed");

    // CASCADE removed the article-scoped biblio join rows. The biblio author /
    // term entities themselves are NOT deleted (they may be shared) - only the
    // article links go.
    assert_eq!(count_rows(&conn, "biblio_article_authors", &format!("article_id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "biblio_article_terms", &format!("article_id = '{id}'")), 0);
    assert_eq!(count_rows(&conn, "biblio_authors", &format!("id = '{author_id}'")), 1);
    assert_eq!(count_rows(&conn, "biblio_terms", &format!("id = '{term_id}'")), 1);
}

// ─── duplicate_of (no ON DELETE clause - manual null-out) ────────────────

#[test]
fn delete_nulls_duplicate_of_pointers() {
    let conn = setup_db();
    let survivor_id = seed_article(&conn, "Surviving Article");
    let dup_id = seed_article(&conn, "Duplicate Article");

    // Mark dup_id as a duplicate of survivor_id.
    article_repo::mark_as_duplicate(&conn, &dup_id, &survivor_id)
        .expect("mark_as_duplicate failed");
    let dup: bango_lib::models::article::Article =
        article_repo::get_article_by_id(&conn, &dup_id).expect("get dup failed");
    assert_eq!(dup.duplicate_of.as_deref(), Some(survivor_id.as_str()));

    // Delete the SURVIVOR. The dup's duplicate_of must be nulled (no ON DELETE).
    article_repo::delete_article(&conn, &survivor_id).expect("delete survivor failed");

    let dup_after: bango_lib::models::article::Article =
        article_repo::get_article_by_id(&conn, &dup_id).expect("get dup after failed");
    assert!(
        dup_after.duplicate_of.is_none(),
        "duplicate_of should be NULL after the surviving article was deleted"
    );
}

// ─── matched_article_id (no ON DELETE clause - manual clear) ─────────────

#[test]
fn delete_clears_matched_article_id_on_reference_papers() {
    let conn = setup_db();
    let id = seed_article(&conn, "Matched Article");
    let paper_id = seed_reference_paper(&conn, "Some Reference");

    // Simulate the paper being promoted to / matched with this article.
    reference_repo::update_paper_match(
        &conn,
        &paper_id,
        &bango_lib::models::reference::MatchStatus::Matched,
        Some(&id),
    )
    .expect("update_paper_match failed");

    article_repo::delete_article(&conn, &id).expect("delete article failed");

    // The reference paper still exists (it was not orphaned via links, so the
    // orphan sweep does not touch it), but its matched_article_id is NULL and
    // match_status is back to 'unmatched'.
    let (status, matched_id, _ref_count, _cit_count) = paper_status(&conn, &paper_id);
    assert!(matched_id.is_none(), "matched_article_id should be cleared");
    assert_eq!(status, "unmatched");
}

// ─── Reference links + orphan cleanup ────────────────────────────────────

#[test]
fn delete_decrements_reference_counters_and_deletes_links() {
    let conn = setup_db();
    let id = seed_article(&conn, "Article with Refs");
    let paper_id = seed_reference_paper(&conn, "Cited Paper");

    reference_repo::create_link(&conn, &id, &paper_id, &ReferenceType::Reference)
        .expect("create link failed");

    // Counter incremented to 1.
    let (_status, _matched, ref_count, _cit) = paper_status(&conn, &paper_id);
    assert_eq!(ref_count, 1);

    article_repo::delete_article(&conn, &id).expect("delete article failed");

    // The link row is gone (CASCADE) and the orphan sweep deleted the paper
    // (it was unmatched with zero remaining links).
    assert_eq!(
        count_rows(&conn, "article_reference_links", &format!("parent_article_id = '{id}'")),
        0
    );
    assert_eq!(
        count_rows(&conn, "reference_papers", &format!("id = '{paper_id}'")),
        0,
        "orphaned reference paper should be deleted"
    );
}

#[test]
fn delete_preserves_shared_reference_paper() {
    let conn = setup_db();
    let id1 = seed_article(&conn, "Article 1");
    let id2 = seed_article(&conn, "Article 2");
    let shared_paper_id = seed_reference_paper(&conn, "Shared Cited Paper");

    // Both articles link to the SAME paper.
    reference_repo::create_link(&conn, &id1, &shared_paper_id, &ReferenceType::Reference)
        .expect("link 1 failed");
    reference_repo::create_link(&conn, &id2, &shared_paper_id, &ReferenceType::Reference)
        .expect("link 2 failed");

    // reference_count is 2 (shared).
    let (_status, _matched, ref_count, _cit) = paper_status(&conn, &shared_paper_id);
    assert_eq!(ref_count, 2);

    // Delete article 1. The paper should survive because article 2 still links it.
    article_repo::delete_article(&conn, &id1).expect("delete article 1 failed");

    assert_eq!(
        count_rows(&conn, "reference_papers", &format!("id = '{shared_paper_id}'")),
        1,
        "shared reference paper must be preserved when other articles still link it"
    );
    let (_status2, _matched2, ref_count_after, _cit2) = paper_status(&conn, &shared_paper_id);
    assert_eq!(ref_count_after, 1, "counter decremented to 1");
}

// ─── Staleness flags ─────────────────────────────────────────────────────

#[test]
fn delete_sets_biblio_and_wiki_refresh_flags() {
    let conn = setup_db();
    let id = seed_article(&conn, "Flag Article");

    // Flags start false (absent key).
    assert!(!app_settings_repo::get_biblio_needs_refresh(&conn).unwrap_or(false));
    assert!(!app_settings_repo::get_wiki_needs_refresh(&conn).unwrap_or(false));

    article_repo::delete_article(&conn, &id).expect("delete failed");

    // Corpus changed: both flags set.
    assert!(app_settings_repo::get_biblio_needs_refresh(&conn).unwrap_or(false));
    assert!(app_settings_repo::get_wiki_needs_refresh(&conn).unwrap_or(false));
}

// ─── Article counts after delete ─────────────────────────────────────────

#[test]
fn delete_updates_article_counts() {
    let conn = setup_db();
    let id1 = seed_article(&conn, "A1");
    let id2 = seed_article(&conn, "A2");
    article_repo::move_to_working(&conn, &id1).expect("move failed");
    article_repo::move_to_working(&conn, &id2).expect("move failed");

    let counts_before = article_repo::get_article_counts(&conn).expect("counts failed");
    assert_eq!(counts_before.working, 2);

    article_repo::delete_article(&conn, &id1).expect("delete failed");

    let counts_after = article_repo::get_article_counts(&conn).expect("counts after failed");
    assert_eq!(counts_after.working, 1, "working count should drop by 1");
    assert_eq!(counts_after.all, 1, "all count should drop by 1");
}

// ─── Query reflects the deletion ─────────────────────────────────────────

#[test]
fn delete_reflected_in_query_articles() {
    let conn = setup_db();
    let id1 = seed_article(&conn, "Keep");
    let id2 = seed_article(&conn, "Delete");
    article_repo::move_to_working(&conn, &id1).expect("move failed");
    article_repo::move_to_working(&conn, &id2).expect("move failed");

    let query = ArticleQuery {
        status: Some("working".into()),
        search: None,
        sort_by: None,
        sort_dir: None,
        year_from: None,
        year_to: None,
        manual_override_only: false,
        screening_errors_only: false,
        author: None,
        journal: None,
        tags: vec![],
        labels: vec![],
        excluded_tags: vec![],
        excluded_labels: vec![],
        doi: None,
        doi_empty: false,
        matched_criteria: vec![],
        criteria_unknown: false,
        criteria_empty: false,
        exclusion_criteria_empty: false,
        limit: None,
        offset: None,
    };
    let before = article_repo::query_articles(&conn, &query).expect("query failed");
    assert_eq!(before.len(), 2);

    article_repo::delete_article(&conn, &id2).expect("delete failed");

    let after = article_repo::query_articles(&conn, &query).expect("query after failed");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, id1);
}

// ─── Full-text chunk repo helper parity (smoke) ──────────────────────────

#[test]
fn delete_article_with_chunks_uses_cascade_consistent_with_chunk_repo() {
    // Ensures the chunk_repo helper and the raw cascade agree on cleanup.
    let conn = setup_db();
    let id = seed_article(&conn, "Chunked");

    // Populate via the chunk repo helper (mirrors attach_full_text path).
    let chunk = bango_lib::utils::chunking::Chunk {
        section: Some("Methods".to_string()),
        chunk_index: 0,
        text: "sample chunk text".to_string(),
        word_count: 3,
    };
    chunk_repo::replace_chunks_for_article(&conn, &id, std::slice::from_ref(&chunk))
        .expect("replace chunks failed");
    assert_eq!(chunk_repo::count_chunks_for_article(&conn, &id).unwrap_or(0), 1);

    article_repo::delete_article(&conn, &id).expect("delete failed");

    assert_eq!(chunk_repo::count_chunks_for_article(&conn, &id).unwrap_or(0), 0);
}
