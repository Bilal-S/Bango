//! Bango article -> Zotero item JSON export mapping tests (Tier 5).
//! Pure; binding inventory: `docs/test-plans/zotero-tests.md`.

use bango_lib::models::article::{Article, ArticleStatus};
use bango_lib::zotero::export_mapping::{
    build_attachment_title, build_item_json, join_pages, map_creators_for_export,
    map_ris_type_to_item_type, merge_tags,
};

fn base_article() -> Article {
    Article {
        id: "a1".into(),
        sequence_id: 1,
        status: ArticleStatus::Working,
        screening_error: false,
        title: String::new(),
        abstract_text: String::new(),
        authors: Vec::new(),
        publication_year: None,
        doi: None,
        journal: None,
        volume: None,
        issue: None,
        start_page: None,
        end_page: None,
        keywords: Vec::new(),
        url: None,
        language: None,
        publisher: None,
        publisher_city: None,
        publisher_address: None,
        issn: None,
        eissn: None,
        journal_index_id: None,
        reference_type: None,
        date: None,
        author_address: None,
        affiliation: None,
        accession_number: None,
        custom_field3: None,
        journal_abbreviation: None,
        journal_iso_abbreviation: None,
        notes: None,
        web_of_science_db: None,
        user_notes: None,
        ris_extras: None,
        duplicate_of: None,
        ai_decision: None,
        ai_reasoning: None,
        ai_confidence: None,
        matched_inclusion_criteria: Vec::new(),
        matched_exclusion_criteria: Vec::new(),
        tags: Vec::new(),
        labels: Vec::new(),
        manual_override: false,
        import_source: None,
        imported_at: String::new(),
        changed_at: String::new(),
        screened_at: None,
        data_length: None,
        token_estimate: None,
        actual_tokens: None,
        full_text: None,
        full_text_ai_summary: None,
        num_cited: None,
        num_references: None,
        has_citation_details: false,
        has_reference_details: false,
        has_full_text: false,
        full_text_file_name: None,
        has_figures_or_tables: false,
        is_translated: false,
        translation_status: "none".into(),
        translation_error: None,
        translated_at: None,
    }
}

fn article() -> Article {
    Article {
        id: "a1".into(),
        sequence_id: 1,
        title: "A Study of Exports".into(),
        abstract_text: "An abstract.".into(),
        authors: vec!["Doe, Jane".into(), "Smith, John".into()],
        publication_year: Some(2020),
        doi: Some("https://doi.org/10.1/ABC".into()),
        journal: Some("Journal of Tests".into()),
        volume: Some("7".into()),
        issue: Some("2".into()),
        start_page: Some("10".into()),
        end_page: Some("20".into()),
        keywords: vec!["keyword-one".into()],
        url: Some("https://example.com".into()),
        language: Some("en".into()),
        publisher: Some("Publisher".into()),
        publisher_city: Some("Berlin".into()),
        issn: Some("1234-5678".into()),
        reference_type: Some("JOUR".into()),
        date: Some("March 2020".into()),
        notes: Some("Internal note text".into()),
        user_notes: Some("PRIVATE user notes".into()),
        tags: vec!["machine-learning".into(), "Physics".into()],
        translation_status: "none".into(),
        ..base_article()
    }
}

#[test]
fn reverse_type_table_maps_ris_to_item_types() {
    let table = [
        ("JOUR", "journalArticle"),
        ("CONF", "conferencePaper"),
        ("BOOK", "book"),
        ("CHAP", "bookSection"),
        ("THES", "thesis"),
        ("RPRT", "report"),
        ("GEN", "document"),
        ("ENCYC", "encyclopediaArticle"),
        ("DICT", "dictionaryEntry"),
        ("NEWS", "newspaperArticle"),
        ("MGZN", "magazineArticle"),
    ];
    for (ris, item_type) in table {
        assert_eq!(map_ris_type_to_item_type(Some(ris)), item_type, "{ris} -> {item_type}");
    }
    // Unknown or None -> journalArticle.
    assert_eq!(map_ris_type_to_item_type(None), "journalArticle");
    assert_eq!(map_ris_type_to_item_type(Some("ELEC")), "journalArticle");
    assert_eq!(map_ris_type_to_item_type(Some("jour")), "journalArticle");
}

#[test]
fn map_article_to_journal_item_json() {
    let item = build_item_json(&article(), "TARGET");
    assert_eq!(item["itemType"], "journalArticle");
    assert_eq!(item["title"], "A Study of Exports");
    assert_eq!(item["abstractNote"], "An abstract.");
    assert_eq!(item["publicationTitle"], "Journal of Tests");
    assert_eq!(item["volume"], "7");
    assert_eq!(item["issue"], "2");
    assert_eq!(item["pages"], "10-20");
    assert_eq!(item["ISSN"], "1234-5678");
    // Canonical DOI form.
    assert_eq!(item["DOI"], "10.1/abc");
    assert_eq!(item["date"], "March 2020");
    assert_eq!(item["url"], "https://example.com");
    assert_eq!(item["language"], "en");
    assert_eq!(item["publisher"], "Publisher");
    assert_eq!(item["place"], "Berlin");
    assert_eq!(item["extra"], "Internal note text");
    assert_eq!(item["collections"], serde_json::json!(["TARGET"]));
    assert_eq!(item["creators"].as_array().map(Vec::len), Some(2));
}

#[test]
fn map_creators_split_lastname_firstname() {
    let creators = map_creators_for_export(&["Doe, Jane".to_string(), "Smith, John".to_string()]);
    assert_eq!(creators[0]["firstName"], "Jane");
    assert_eq!(creators[0]["lastName"], "Doe");
    assert_eq!(creators[0]["creatorType"], "author");
    assert_eq!(creators[1]["lastName"], "Smith");
}

#[test]
fn map_creators_single_token_uses_name() {
    let creators = map_creators_for_export(&["World Health Organization".to_string()]);
    assert_eq!(creators[0]["name"], "World Health Organization");
    assert!(creators[0].get("firstName").is_none());
}

#[test]
fn map_creators_drop_malformed_entries() {
    // Empty and whitespace-only entries are dropped; ", " is malformed too.
    let creators = map_creators_for_export(&["".to_string(), "   ".to_string(), " , ".to_string()]);
    assert!(creators.is_empty());
}

#[test]
fn map_pages_join_start_end() {
    assert_eq!(join_pages(Some("1"), Some("10")).as_deref(), Some("1-10"));
    assert_eq!(join_pages(Some("5"), None).as_deref(), Some("5"));
    assert_eq!(join_pages(None, Some("7")).as_deref(), Some("7"));
    assert_eq!(join_pages(None, None), None);
    assert_eq!(join_pages(Some(""), Some("9")).as_deref(), Some("9"));
}

#[test]
fn map_non_journal_types_drop_invalid_fields() {
    // Conference: volume/issue stay, journal/ISSN drop.
    let conference =
        build_item_json(&Article { reference_type: Some("CONF".into()), ..article() }, "T");
    assert_eq!(conference["itemType"], "conferencePaper");
    assert!(conference.get("publicationTitle").is_none());
    assert!(conference.get("ISSN").is_none());
    assert_eq!(conference["volume"], "7");
    assert_eq!(conference["issue"], "2");

    // Book: the common subset only.
    let book = build_item_json(&Article { reference_type: Some("BOOK".into()), ..article() }, "T");
    assert_eq!(book["itemType"], "book");
    assert!(book.get("publicationTitle").is_none());
    assert!(book.get("ISSN").is_none());
    assert!(book.get("volume").is_none());
    assert!(book.get("issue").is_none());
}

#[test]
fn map_tags_and_keywords_merge_deduped() {
    let tags = merge_tags(
        &["Physics".to_string(), "machine-learning".to_string()],
        &["physics".to_string(), "keyword-one".to_string()],
    );
    // Case-insensitive dedupe, order-preserving.
    let names: Vec<&str> = tags.iter().filter_map(|t| t["tag"].as_str()).collect();
    assert_eq!(names, vec!["Physics", "machine-learning", "keyword-one"]);
}

#[test]
fn map_notes_to_extra_user_notes_excluded() {
    let item = build_item_json(&article(), "T");
    assert_eq!(item["extra"], "Internal note text");
    // user_notes and labels never export.
    let serialized = item.to_string();
    assert!(!serialized.contains("PRIVATE user notes"));
}

#[test]
fn build_attachment_title_lastname_and_word_boundary_truncation() {
    let title = "The awakening of sundown phenomena in coastal regions";
    assert_eq!(
        build_attachment_title(
            &["Jones, Mary".to_string(), "Smith, Anna".to_string()],
            title,
            "pdf"
        ),
        "Jones - The awakening of sundown.pdf"
    );
    // Short titles pass through untouched; a leading extension dot normalizes.
    assert_eq!(
        build_attachment_title(&["Doe, Jane".to_string()], "A study", ".pdf"),
        "Doe - A study.pdf"
    );
}

#[test]
fn build_attachment_title_single_token_author_uses_whole_name() {
    assert_eq!(
        build_attachment_title(&["World Health Organization".to_string()], "Malaria report", "txt"),
        "World Health Organization - Malaria report.txt"
    );
}

#[test]
fn build_attachment_title_no_author_or_title_fallbacks() {
    // No author -> title only; blank author entries are skipped.
    assert_eq!(
        build_attachment_title(&["".to_string(), "   ".to_string()], "Short title", "pdf"),
        "Short title.pdf"
    );
    // Blank title -> Untitled.
    assert_eq!(
        build_attachment_title(&["Doe, Jane".to_string()], "   ", "pdf"),
        "Doe - Untitled.pdf"
    );
    // A single word longer than 30 chars hard-cuts at 30.
    let long_word = "a".repeat(40);
    assert_eq!(build_attachment_title(&[], &long_word, "pdf"), format!("{}.pdf", "a".repeat(30)));
}
