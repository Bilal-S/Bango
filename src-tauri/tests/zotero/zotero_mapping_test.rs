//! Zotero item -> `RisRecord` mapping tests (Tier 1).
//!
//! Binding inventory: `docs/test-plans/zotero-tests.md`.

use bango_lib::zotero::mapping::{
    extract_year, map_creators, map_item_to_ris_record, map_item_type_to_ris_type,
    merge_child_notes, note_html_to_text, parse_pages, sanitize_zotero_tag,
};
use bango_lib::zotero::ZoteroItem;

fn parse_item(json: &str) -> ZoteroItem {
    serde_json::from_str(json).expect("item JSON should parse")
}

const JOURNAL_ARTICLE_JSON: &str = r#"{
    "key": "FLQYTTDI",
    "version": 15,
    "library": {"type": "user", "id": 0},
    "meta": {"parsedDate": "1957-04-01", "creatorSummary": "Bardeen et al."},
    "data": {
        "key": "FLQYTTDI",
        "version": 15,
        "itemType": "journalArticle",
        "title": "Theory of Superconductivity",
        "abstractNote": "Meissner and Ochsenfeld observed...",
        "creators": [
            {"creatorType": "author", "firstName": "J.", "lastName": "Bardeen"},
            {"creatorType": "author", "firstName": "Leon", "lastName": "Cooper"},
            {"creatorType": "editor", "firstName": "Ed", "lastName": "Editor"}
        ],
        "publicationTitle": "Physical Review",
        "volume": "106",
        "issue": "4",
        "pages": "162-172",
        "date": "April 1957",
        "ISSN": "0031-899X",
        "DOI": "https://doi.org/10.1103/PhysRev.106.162",
        "url": "https://example.com/paper",
        "language": "en",
        "publisher": "APS",
        "place": "New York",
        "extra": "Some extra notes",
        "tags": [
            {"tag": "physics", "type": 1},
            {"tag": "Machine Learning", "type": 0}
        ]
    }
}"#;

#[test]
fn map_item_type_table_covers_scholarly_types() {
    // Every scholarly itemType from the table maps to its canonical RIS code.
    let table = [
        ("journalArticle", "JOUR"),
        ("conferencePaper", "CONF"),
        ("preprint", "GEN"),
        ("book", "BOOK"),
        ("bookSection", "CHAP"),
        ("thesis", "THES"),
        ("report", "RPRT"),
        ("document", "GEN"),
        ("manuscript", "GEN"),
        ("encyclopediaArticle", "ENCYC"),
        ("dictionaryEntry", "DICT"),
        ("newspaperArticle", "NEWS"),
        ("magazineArticle", "MGZN"),
    ];
    for (item_type, expected) in table {
        assert_eq!(
            map_item_type_to_ris_type(item_type),
            Some(expected),
            "itemType {item_type} should map to {expected}"
        );
    }
}

#[test]
fn map_journal_article_to_ris_record() {
    let record =
        map_item_to_ris_record(&parse_item(JOURNAL_ARTICLE_JSON)).expect("journalArticle maps");
    assert_eq!(record.title.as_deref(), Some("Theory of Superconductivity"));
    assert_eq!(record.abstract_text.as_deref(), Some("Meissner and Ochsenfeld observed..."));
    assert_eq!(record.journal.as_deref(), Some("Physical Review"));
    assert_eq!(record.volume.as_deref(), Some("106"));
    assert_eq!(record.issue.as_deref(), Some("4"));
    assert_eq!(record.start_page.as_deref(), Some("162"));
    assert_eq!(record.end_page.as_deref(), Some("172"));
    assert_eq!(record.issn.as_deref(), Some("0031-899X"));
    assert_eq!(record.doi.as_deref(), Some("10.1103/physrev.106.162"));
    assert_eq!(record.reference_type.as_deref(), Some("JOUR"));
    assert_eq!(record.url.as_deref(), Some("https://example.com/paper"));
    assert_eq!(record.language.as_deref(), Some("en"));
    assert_eq!(record.publisher.as_deref(), Some("APS"));
    assert_eq!(record.publisher_city.as_deref(), Some("New York"));
    assert_eq!(record.notes.as_deref(), Some("Some extra notes"));
}

#[test]
fn map_authors_prefer_author_creators() {
    let item = parse_item(JOURNAL_ARTICLE_JSON);
    // Only the two `creatorType=author` entries map, as "Lastname, Firstname".
    assert_eq!(map_creators(&item.data.creators), vec!["Bardeen, J.", "Cooper, Leon"]);
    // Single-field (institutional) names are used verbatim.
    let institutional = r#"{"creators": [
        {"creatorType": "author", "name": "World Health Organization"}
    ]}"#;
    let value: serde_json::Value = serde_json::from_str(institutional).unwrap();
    let creators: Vec<bango_lib::zotero::ZoteroCreator> =
        serde_json::from_value(value["creators"].clone()).unwrap();
    assert_eq!(map_creators(&creators), vec!["World Health Organization"]);
}

#[test]
fn map_authors_fall_back_to_editors() {
    let json = r#"{
        "key": "K1", "version": 1, "meta": {},
        "data": {
            "itemType": "book", "title": "Edited Volume",
            "creators": [
                {"creatorType": "editor", "firstName": "Anna", "lastName": "Smith"},
                {"creatorType": "contributor", "firstName": "Bob", "lastName": "Jones"}
            ]
        }
    }"#;
    let item = parse_item(json);
    // No authors: editors are used; contributors never participate.
    assert_eq!(map_creators(&item.data.creators), vec!["Smith, Anna"]);
}

#[test]
fn map_parsed_date_to_year_and_date() {
    let item = parse_item(JOURNAL_ARTICLE_JSON);
    let record = map_item_to_ris_record(&item).expect("maps");
    assert_eq!(record.publication_year, Some(1957));
    assert_eq!(record.date.as_deref(), Some("April 1957"));
    // Year extraction handles the three parsedDate shapes.
    assert_eq!(extract_year("2020-03-15"), Some(2020));
    assert_eq!(extract_year("2020-03"), Some(2020));
    assert_eq!(extract_year("2020"), Some(2020));
    assert_eq!(extract_year("undated"), None);
}

#[test]
fn map_pages_split_start_end() {
    assert_eq!(parse_pages("1-10"), (Some("1".to_string()), Some("10".to_string())));
    assert_eq!(parse_pages("1--10"), (Some("1".to_string()), Some("10".to_string())));
    assert_eq!(parse_pages("1 - 10"), (Some("1".to_string()), Some("10".to_string())));
    assert_eq!(parse_pages("162–172"), (Some("162".to_string()), Some("172".to_string())));
    assert_eq!(parse_pages("5"), (Some("5".to_string()), None));
    assert_eq!(parse_pages(""), (None, None));
}

#[test]
fn map_doi_normalized() {
    // URL prefixes stripped, lowercased (ris::doi::normalize_doi).
    let item = parse_item(JOURNAL_ARTICLE_JSON);
    assert_eq!(
        map_item_to_ris_record(&item).expect("maps").doi.as_deref(),
        Some("10.1103/physrev.106.162")
    );
    // Placeholder DOIs are dropped entirely.
    let placeholder_json = r#"{
        "key": "K2", "version": 1, "meta": {},
        "data": {"itemType": "journalArticle", "title": "T", "DOI": "NA"}
    }"#;
    let item = parse_item(placeholder_json);
    assert_eq!(map_item_to_ris_record(&item).expect("maps").doi, None);
}

#[test]
fn map_tags_not_written_to_keywords() {
    let item = parse_item(JOURNAL_ARTICLE_JSON);
    let record = map_item_to_ris_record(&item).expect("maps");
    // Zotero tags flow to Bango tags post-insert, never into keywords.
    assert!(record.keywords.is_empty(), "keywords must stay empty: {:?}", record.keywords);
}

#[test]
fn unsupported_item_type_maps_to_none() {
    for unsupported in ["interview", "artwork", "webpage", "film"] {
        let json = format!(
            r#"{{"key": "K3", "version": 1, "meta": {{}},
                "data": {{"itemType": "{unsupported}", "title": "T"}}}}"#
        );
        let item = parse_item(&json);
        assert!(map_item_to_ris_record(&item).is_none(), "{unsupported} should map to None");
        assert_eq!(map_item_type_to_ris_type(unsupported), None);
    }
}

#[test]
fn attachment_and_note_item_types_skipped() {
    for skipped in ["attachment", "note", "annotation"] {
        let json = format!(
            r#"{{"key": "K4", "version": 1, "meta": {{}},
                "data": {{"itemType": "{skipped}", "title": "T"}}}}"#
        );
        let item = parse_item(&json);
        assert!(map_item_to_ris_record(&item).is_none(), "{skipped} should be skipped");
    }
}

#[test]
fn sanitize_zotero_tag_lowercase_hyphenated() {
    assert_eq!(sanitize_zotero_tag("Machine Learning").as_deref(), Some("machine-learning"));
    assert_eq!(sanitize_zotero_tag("AI & ML!").as_deref(), Some("ai-ml"));
    assert_eq!(sanitize_zotero_tag("under_scored tag").as_deref(), Some("under-scored-tag"));
    assert_eq!(sanitize_zotero_tag("--already--hyphens--").as_deref(), Some("already-hyphens"));
    assert_eq!(sanitize_zotero_tag("!!!"), None);
    assert_eq!(sanitize_zotero_tag("   "), None);
}

#[test]
fn sanitize_zotero_tag_strips_inclusion_prefix() {
    assert_eq!(sanitize_zotero_tag("inclusion: humans").as_deref(), Some("humans"));
    assert_eq!(sanitize_zotero_tag("inclusion:humans").as_deref(), Some("humans"));
    assert_eq!(sanitize_zotero_tag("Exclusion: animals").as_deref(), Some("animals"));
    // Unprefixed text passes through untouched.
    assert_eq!(sanitize_zotero_tag("inclusion criteria").as_deref(), Some("inclusion-criteria"));
}

#[test]
fn sanitize_zotero_tag_truncates_to_35_chars() {
    let long = "systematic-review-of-machine-learning-applications-in-healthcare";
    let sanitized = sanitize_zotero_tag(long).expect("non-empty");
    assert!(
        sanitized.chars().count() <= 35,
        "must be <= 35 chars, got {} ({sanitized})",
        sanitized.chars().count()
    );
    // Truncation happens at the last word boundary (hyphen), never mid-word,
    // and never leaves a trailing hyphen.
    assert!(!sanitized.ends_with('-'), "no trailing hyphen: {sanitized}");
    assert!(long.starts_with(sanitized.as_str()), "must be a prefix: {sanitized}");
    // A 40-char single word with no boundary hard-truncates at 35.
    let single_word = "a".repeat(40);
    assert_eq!(sanitize_zotero_tag(&single_word).map(|s| s.chars().count()), Some(35));
}

fn note_item(
    key: &str,
    parent: &str,
    note: &str,
    date_added: &str,
) -> bango_lib::zotero::ZoteroNoteItem {
    serde_json::from_str(&format!(
        r#"{{"key":"{key}","version":1,"data":{{"itemType":"note","note":{note},"parentItem":"{parent}","tags":[],"dateAdded":"{date_added}","dateModified":"{date_added}"}}}}"#
    ))
    .unwrap()
}

#[test]
fn merge_child_notes_orders_by_date_and_formats_blocks() {
    // Out-of-order dateAdded values: the merge is chronological (oldest first).
    let owned = vec![
        note_item(
            "N2",
            "ITEM1",
            "\"<p>Second note</p><p>Later body.</p>\"",
            "2026-02-02T10:00:00Z",
        ),
        note_item("N1", "ITEM1", "\"First note<br/>line two &amp; more\"", "2026-01-01T09:00:00Z"),
        // An empty note contributes nothing.
        note_item("N3", "ITEM1", "\"<p></p>\"", "2026-03-03T10:00:00Z"),
    ];
    let notes: Vec<&bango_lib::zotero::ZoteroNoteItem> = owned.iter().collect();
    let merged = merge_child_notes(&notes).expect("non-empty");
    assert_eq!(merged, "First note\n---\nline two & more\n\nSecond note\n---\nLater body.");
    // A title-only note emits no body lines.
    let single_owned = vec![note_item("N4", "ITEM1", "\"Just a title\"", "2026-01-01T00:00:00Z")];
    let single: Vec<&bango_lib::zotero::ZoteroNoteItem> = single_owned.iter().collect();
    assert_eq!(merge_child_notes(&single).as_deref(), Some("Just a title\n---"));
    // No notes with text -> None.
    let empty_owned = vec![note_item("N5", "ITEM1", "\"   \"", "2026-01-01T00:00:00Z")];
    let empty: Vec<&bango_lib::zotero::ZoteroNoteItem> = empty_owned.iter().collect();
    assert!(merge_child_notes(&empty).is_none());
    assert!(merge_child_notes(&[]).is_none());
}

#[test]
fn note_html_to_text_strips_tags_and_decodes_entities() {
    // Block and line-break tags become newlines; other tags drop.
    assert_eq!(note_html_to_text("<p>Hello <b>world</b></p><p>Second</p>"), "Hello world\nSecond");
    assert_eq!(note_html_to_text("a<br/>b<br />c"), "a\nb\nc");
    // Named + numeric entities decode; unknown entities pass through.
    assert_eq!(note_html_to_text("&amp; &lt; &gt; &quot; &#39; &nbsp;X&#x27;"), "& < > \" '  X'");
    assert_eq!(note_html_to_text("a &unknown; b"), "a &unknown; b");
    // Consecutive newlines (paragraph gaps) collapse to one - no blank lines.
    assert_eq!(note_html_to_text("<p>a</p><br/><br/><p>b</p>"), "a\nb");
    assert_eq!(note_html_to_text("  <p>trimmed</p>  "), "trimmed");
    assert_eq!(note_html_to_text(""), "");
}
