use bango_lib::db::article_repo;
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::reference_repo;
use bango_lib::models::article::NewArticle;
use bango_lib::models::reference::{MatchStatus, NewReferencePaper, ReferenceType};

fn make_paper(title: &str, doi: Option<&str>) -> NewReferencePaper {
    NewReferencePaper {
        title: Some(title.to_string()),
        abstract_text: None,
        authors: vec!["Author A".to_string()],
        publication_year: Some(2020),
        doi: doi.map(|d| d.to_string()),
        journal: Some("Test Journal".to_string()),
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec![],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        reference_type: None,
        date: None,
        notes: None,
        ris_extras: None,
        match_status: None,
        matched_article_id: None,
        import_source: None,
    }
}

fn make_article(title: &str) -> NewArticle {
    NewArticle {
        title: title.to_string(),
        abstract_text: "Abstract".to_string(),
        authors: vec!["Author".to_string()],
        publication_year: Some(2023),
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: vec![],
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        reference_type: None,
        date: None,
        author_address: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        ris_extras: None,
        import_source: None,
        data_length: None,
        token_estimate: None,
        num_cited: None,
        num_references: None,
        has_full_text: false,
        full_text_file_name: None,
    }
}

#[test]
fn test_insert_single_reference_paper() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let paper = make_paper("Machine Learning Basics", Some("10.1234/ml-basics"));
    let (inserted, was_created) =
        reference_repo::insert_or_find_paper(&conn, &paper).expect("insert_or_find failed");

    assert!(was_created, "First insert should create a new paper");
    assert_eq!(inserted.title, "Machine Learning Basics");
    assert_eq!(inserted.doi.as_deref(), Some("10.1234/ml-basics"));
}

#[test]
fn test_doi_dedup_same_doi_twice() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let paper = make_paper("Paper A", Some("10.1234/dedup-test"));
    let (first, was_created1) =
        reference_repo::insert_or_find_paper(&conn, &paper).expect("insert 1 failed");
    assert!(was_created1);

    // Insert same DOI with different title — should dedup
    let paper2 =
        NewReferencePaper { title: Some("Paper A Duplicate".to_string()), ..paper.clone() };
    let (second, was_created2) =
        reference_repo::insert_or_find_paper(&conn, &paper2).expect("insert 2 failed");
    assert!(!was_created2, "Second insert should NOT create a new paper");
    assert_eq!(first.id, second.id, "Should return the same paper ID");
    assert_eq!(second.title, "Paper A", "Should keep original title");
}

#[test]
fn test_create_link_reference_type() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article = article_repo::insert_article(&conn, &make_article("Test Article"))
        .expect("insert article failed");
    let (paper, _) = reference_repo::insert_or_find_paper(&conn, &make_paper("Cited Paper", None))
        .expect("insert paper failed");

    reference_repo::create_link(&conn, &article.id, &paper.id, &ReferenceType::Reference)
        .expect("create_link failed");

    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].paper.title, "Cited Paper");
}

#[test]
fn test_create_link_citation_type() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article = article_repo::insert_article(&conn, &make_article("Test Article"))
        .expect("insert article failed");
    let (paper, _) = reference_repo::insert_or_find_paper(&conn, &make_paper("Citing Paper", None))
        .expect("insert paper failed");

    reference_repo::create_link(&conn, &article.id, &paper.id, &ReferenceType::Citation)
        .expect("create_link failed");

    let citations = reference_repo::get_references_for_article(
        &conn,
        &article.id,
        Some(&ReferenceType::Citation),
    )
    .expect("get citations failed");
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].paper.title, "Citing Paper");

    // Should NOT appear when filtering for references
    let refs = reference_repo::get_references_for_article(
        &conn,
        &article.id,
        Some(&ReferenceType::Reference),
    )
    .expect("get refs failed");
    assert_eq!(refs.len(), 0, "Citation should not appear in reference filter");
}

#[test]
fn test_one_article_many_references() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article = article_repo::insert_article(&conn, &make_article("Survey Paper"))
        .expect("insert article failed");

    let papers: Vec<_> = (0..3)
        .map(|i| {
            let (p, _) = reference_repo::insert_or_find_paper(
                &conn,
                &make_paper(&format!("Ref Paper {}", i), Some(&format!("10.1234/ref-{}", i))),
            )
            .expect("insert paper failed");
            p
        })
        .collect();

    for paper in &papers {
        reference_repo::create_link(&conn, &article.id, &paper.id, &ReferenceType::Reference)
            .expect("create_link failed");
    }

    let refs = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get refs failed");
    assert_eq!(refs.len(), 3, "Should have 3 reference papers");
}

#[test]
fn test_two_articles_shared_paper() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article1 = article_repo::insert_article(&conn, &make_article("Article 1"))
        .expect("insert article1 failed");
    let article2 = article_repo::insert_article(&conn, &make_article("Article 2"))
        .expect("insert article2 failed");

    // Both articles reference the same paper (same DOI)
    let shared_doi = "10.1234/shared-paper";
    let (paper, _) =
        reference_repo::insert_or_find_paper(&conn, &make_paper("Shared Paper", Some(shared_doi)))
            .expect("insert paper failed");

    reference_repo::create_link(&conn, &article1.id, &paper.id, &ReferenceType::Reference)
        .expect("link1 failed");
    reference_repo::create_link(&conn, &article2.id, &paper.id, &ReferenceType::Reference)
        .expect("link2 failed");

    // Verify both articles see the shared paper
    let refs1 = reference_repo::get_references_for_article(&conn, &article1.id, None)
        .expect("get refs1 failed");
    let refs2 = reference_repo::get_references_for_article(&conn, &article2.id, None)
        .expect("get refs2 failed");
    assert_eq!(refs1.len(), 1);
    assert_eq!(refs2.len(), 1);
    assert_eq!(refs1[0].paper.id, refs2[0].paper.id);

    // Verify count
    let count1 = reference_repo::count_references_for_article(
        &conn,
        &article1.id,
        &ReferenceType::Reference,
    )
    .expect("count failed");
    let count2 = reference_repo::count_references_for_article(
        &conn,
        &article2.id,
        &ReferenceType::Reference,
    )
    .expect("count failed");
    assert_eq!(count1, 1);
    assert_eq!(count2, 1);
}

#[test]
fn test_delete_links_for_one_article() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article1 = article_repo::insert_article(&conn, &make_article("Article 1"))
        .expect("insert article1 failed");
    let article2 = article_repo::insert_article(&conn, &make_article("Article 2"))
        .expect("insert article2 failed");

    let (paper, _) =
        reference_repo::insert_or_find_paper(&conn, &make_paper("Shared", Some("10.1234/s")))
            .expect("insert paper failed");

    reference_repo::create_link(&conn, &article1.id, &paper.id, &ReferenceType::Reference)
        .expect("link1 failed");
    reference_repo::create_link(&conn, &article2.id, &paper.id, &ReferenceType::Reference)
        .expect("link2 failed");

    // Delete only article1's links
    reference_repo::delete_references_for_article(&conn, &article1.id).expect("delete failed");

    let refs1 = reference_repo::get_references_for_article(&conn, &article1.id, None)
        .expect("get refs1 failed");
    let refs2 = reference_repo::get_references_for_article(&conn, &article2.id, None)
        .expect("get refs2 failed");
    assert_eq!(refs1.len(), 0, "Article 1 links should be deleted");
    assert_eq!(refs2.len(), 1, "Article 2 links should still exist");
}

#[test]
fn test_filter_by_reference_type() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let article =
        article_repo::insert_article(&conn, &make_article("Test")).expect("insert article failed");

    let (paper1, _) =
        reference_repo::insert_or_find_paper(&conn, &make_paper("Ref Paper", Some("10.1234/ref")))
            .expect("insert paper1 failed");
    let (paper2, _) =
        reference_repo::insert_or_find_paper(&conn, &make_paper("Cit Paper", Some("10.1234/cit")))
            .expect("insert paper2 failed");

    reference_repo::create_link(&conn, &article.id, &paper1.id, &ReferenceType::Reference)
        .expect("link ref failed");
    reference_repo::create_link(&conn, &article.id, &paper2.id, &ReferenceType::Citation)
        .expect("link cit failed");

    // All
    let all = reference_repo::get_references_for_article(&conn, &article.id, None)
        .expect("get all failed");
    assert_eq!(all.len(), 2);

    // Only references
    let refs_only = reference_repo::get_references_for_article(
        &conn,
        &article.id,
        Some(&ReferenceType::Reference),
    )
    .expect("get refs failed");
    assert_eq!(refs_only.len(), 1);
    assert_eq!(refs_only[0].paper.title, "Ref Paper");

    // Only citations
    let cit_only = reference_repo::get_references_for_article(
        &conn,
        &article.id,
        Some(&ReferenceType::Citation),
    )
    .expect("get cit failed");
    assert_eq!(cit_only.len(), 1);
    assert_eq!(cit_only[0].paper.title, "Cit Paper");
}

#[test]
fn test_promote_unmatched_paper_to_article() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Create a source article
    let source = article_repo::insert_article(&conn, &make_article("Source Article"))
        .expect("insert source failed");

    // Create an unmatched reference paper
    let paper = make_paper("Unmatched Paper", Some("10.1234/promote-test"));
    let (inserted_paper, _) =
        reference_repo::insert_or_find_paper(&conn, &paper).expect("insert paper failed");

    // Link it as a reference
    reference_repo::create_link(&conn, &source.id, &inserted_paper.id, &ReferenceType::Reference)
        .expect("create_link failed");

    // Verify it starts as unmatched
    assert_eq!(inserted_paper.match_status, MatchStatus::Unmatched);
    assert!(inserted_paper.matched_article_id.is_none());

    // Create a new article from the paper's data (mimics what the Tauri command does)
    let new_article = article_repo::insert_article(
        &conn,
        &NewArticle {
            title: inserted_paper.title.clone(),
            abstract_text: inserted_paper.abstract_text.clone().unwrap_or_default(),
            authors: inserted_paper.authors.clone(),
            publication_year: inserted_paper.publication_year,
            doi: inserted_paper.doi.clone(),
            journal: inserted_paper.journal.clone(),
            ..make_article("placeholder")
        },
    )
    .expect("insert new article failed");

    // Promote the paper — links it to the new article
    reference_repo::promote_to_article(&conn, &inserted_paper.id, &new_article.id)
        .expect("promote_to_article failed");

    // Verify the paper's match status was updated
    let refs_after = reference_repo::get_references_for_article(&conn, &source.id, None)
        .expect("get refs failed");
    assert_eq!(refs_after.len(), 1);
    assert_eq!(refs_after[0].paper.match_status, MatchStatus::Imported);
    assert_eq!(refs_after[0].paper.matched_article_id.clone().unwrap(), new_article.id);

    // Verify the new article exists and has the right title
    let fetched =
        article_repo::get_article_by_id(&conn, &new_article.id).expect("get article failed");
    assert_eq!(fetched.title, "Unmatched Paper");
}

#[test]
fn test_promote_links_to_existing_article_by_doi() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    // Create an article with a known DOI
    let existing = article_repo::insert_article(
        &conn,
        &NewArticle {
            title: "Existing ML Paper".to_string(),
            abstract_text: "Existing abstract".to_string(),
            doi: Some("10.1234/existing-ml".to_string()),
            authors: vec!["Smith".to_string()],
            publication_year: Some(2021),
            journal: Some("AI Journal".to_string()),
            ..make_article("placeholder")
        },
    )
    .expect("insert existing article failed");
    // Move to working (articles default to 'duplicate')
    article_repo::move_to_working(&conn, &existing.id).expect("move_to_working failed");

    // Create a reference paper with the SAME DOI
    let paper = make_paper("Existing ML Paper", Some("10.1234/existing-ml"));
    let (inserted_paper, _) =
        reference_repo::insert_or_find_paper(&conn, &paper).expect("insert paper failed");

    // Auto-match should find the existing article
    let matched_id = reference_repo::auto_match_paper_to_article(&conn, &inserted_paper)
        .expect("auto_match failed");
    assert!(matched_id.is_some(), "Should find a match by DOI");
    assert_eq!(matched_id.as_deref().unwrap(), existing.id, "Should match the existing article");

    // Link the paper to the existing article (mimics promote command's linked path)
    reference_repo::update_paper_match(
        &conn,
        &inserted_paper.id,
        &MatchStatus::Matched,
        matched_id.as_deref(),
    )
    .expect("update_paper_match failed");

    // Verify the paper is now linked
    let refreshed =
        reference_repo::get_paper_by_id(&conn, &inserted_paper.id).expect("get paper failed");
    assert_eq!(refreshed.match_status, MatchStatus::Matched);
    assert_eq!(refreshed.matched_article_id.as_deref(), Some(existing.id.as_str()));

    // Verify no duplicate article was created (still only 1 article)
    let all = article_repo::get_all_articles(&conn).expect("get all failed");
    assert_eq!(all.len(), 1, "Should still have exactly 1 article");
}
