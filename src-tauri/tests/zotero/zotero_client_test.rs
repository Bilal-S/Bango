//! Zotero client path-resolution + response-parsing tests (Tier 1).
//!
//! Pure string/JSON logic - no HTTP, no `#[cfg(windows)]` - so every branch
//! runs on every platform. Binding inventory: `docs/test-plans/zotero-tests.md`.

use bango_lib::zotero::client::resolve_attachment_path;
use bango_lib::zotero::mapping::group_attachments_by_parent;
use bango_lib::zotero::{ZoteroChildItem, ZoteroCollection, ZoteroError, ZoteroItem};

#[test]
fn resolve_attachment_path_unix_plain() {
    let path = resolve_attachment_path("file:///home/u/Zotero/storage/KEY/paper.pdf")
        .expect("plain unix path resolves");
    assert_eq!(path, std::path::PathBuf::from("/home/u/Zotero/storage/KEY/paper.pdf"));
}

#[test]
fn resolve_attachment_path_unix_percent_encoded() {
    let path = resolve_attachment_path("file:///home/u/z/a%20b.pdf")
        .expect("percent-encoded path resolves");
    assert_eq!(path, std::path::PathBuf::from("/home/u/z/a b.pdf"));
}

#[test]
fn resolve_attachment_path_unicode_filename() {
    // Non-ASCII percent-encoded filename decodes (the `url` crate encodes
    // raw non-ASCII paths, so both spellings must decode the same way).
    let encoded = resolve_attachment_path(
        "file:///home/u/z/storage/KEY/%E6%97%A5%E6%9C%AC%E8%AA%9E%20%E8%AB%96%E6%96%87.pdf",
    )
    .expect("encoded unicode path resolves");
    let raw = resolve_attachment_path("file:///home/u/z/storage/KEY/日本語 論文.pdf")
        .expect("raw unicode path resolves");
    let expected = std::path::PathBuf::from("/home/u/z/storage/KEY/日本語 論文.pdf");
    assert_eq!(encoded, expected);
    assert_eq!(raw, expected);
}

#[test]
fn resolve_attachment_path_windows_drive_letter() {
    // Runs on all platforms: pure string logic, no Path::is_absolute checks.
    let path =
        resolve_attachment_path("file:///C:/Users/u/z/a.pdf").expect("drive-letter path resolves");
    assert_eq!(path, std::path::PathBuf::from("C:/Users/u/z/a.pdf"));
    // Percent-encoded drive-letter form decodes first.
    let spaced = resolve_attachment_path("file:///C:/Users/u/z/a%20b.pdf")
        .expect("spaced drive-letter path resolves");
    assert_eq!(spaced, std::path::PathBuf::from("C:/Users/u/z/a b.pdf"));
}

#[test]
fn resolve_attachment_path_windows_unc() {
    let path = resolve_attachment_path("file://server/share/a.pdf").expect("UNC path resolves");
    assert_eq!(path, std::path::PathBuf::from(r"\\server\share\a.pdf"));
}

#[test]
fn parse_collections_response() {
    // Shape captured live from Zotero 10.0.1 `GET /users/0/collections`.
    let json = r#"[
        {
            "key": "G3HHVJ7V", "version": 2,
            "meta": {"numCollections": 0, "numItems": 3},
            "data": {"key": "G3HHVJ7V", "version": 2, "name": "Another Collection",
                     "parentCollection": false, "relations": {}}
        },
        {
            "key": "R525FRRJ", "version": 1,
            "meta": {"numCollections": 0, "numItems": 3},
            "data": {"key": "R525FRRJ", "version": 1, "name": "More Stuff",
                     "parentCollection": "L5NHKFM6", "relations": {}}
        },
        {
            "key": "NOKEY", "version": 1,
            "meta": {},
            "data": {"key": "NOKEY", "version": 1, "name": "Absent Parent"}
        }
    ]"#;
    let collections: Vec<ZoteroCollection> = serde_json::from_str(json).expect("parses");
    assert_eq!(collections.len(), 3);
    // `parentCollection: false` and absent both map to a null parent.
    assert_eq!(collections[0].key, "G3HHVJ7V");
    assert_eq!(collections[0].data.name, "Another Collection");
    assert_eq!(collections[0].data.parent_collection, None);
    assert_eq!(collections[1].data.name, "More Stuff");
    assert_eq!(collections[1].data.parent_collection.as_deref(), Some("L5NHKFM6"));
    assert_eq!(collections[2].data.parent_collection, None);
}

#[test]
fn parse_items_response() {
    let json = r#"[
        {
            "key": "FLQYTTDI", "version": 15,
            "library": {"type": "user", "id": 0},
            "meta": {"parsedDate": "1957-04-01", "creatorSummary": "Bardeen et al."},
            "data": {
                "key": "FLQYTTDI", "version": 15, "itemType": "journalArticle",
                "title": "Theory of Superconductivity", "abstractNote": "...",
                "creators": [{"creatorType": "author", "firstName": "J.", "lastName": "Bardeen"}],
                "publicationTitle": "Physical Review", "volume": "106", "issue": "4",
                "pages": "162-172", "date": "April 1957", "ISSN": "0031-899X",
                "DOI": "10.1103/PhysRev.106.162", "tags": [{"tag": "physics", "type": 1}]
            }
        },
        {
            "key": "J3JPQHFW", "version": 3,
            "meta": {},
            "data": {
                "key": "J3JPQHFW", "version": 3, "itemType": "attachment",
                "linkMode": "imported_url", "contentType": "application/pdf",
                "filename": "Bardeen et al. - 1957.pdf", "parentItem": "FLQYTTDI"
            }
        }
    ]"#;
    let items: Vec<ZoteroItem> = serde_json::from_str(json).expect("parses");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].meta.parsed_date.as_deref(), Some("1957-04-01"));
    assert_eq!(items[0].data.item_type, "journalArticle");
    assert_eq!(items[0].data.doi.as_deref(), Some("10.1103/PhysRev.106.162"));
    assert_eq!(items[0].data.tags[0].tag, "physics");
    assert_eq!(items[0].data.tags[0].tag_type, Some(1));
    // Child partitioning signal: parentItem present only on the attachment.
    assert_eq!(items[0].data.parent_item, None);
    assert_eq!(items[1].data.parent_item.as_deref(), Some("FLQYTTDI"));
}

#[test]
fn parse_attachment_list_response() {
    // Shape captured live from `GET /users/0/items?itemType=attachment`.
    let json = r#"[
        {
            "key": "MBDB7E7F", "version": 9,
            "meta": {},
            "data": {"key": "MBDB7E7F", "version": 9, "itemType": "attachment",
                     "linkMode": "imported_url", "contentType": "application/pdf",
                     "filename": "Gray - 2024 - A cultural history.pdf",
                     "parentItem": "YRGS6H4H", "tags": []}
        },
        {
            "key": "J3JPQHFW", "version": 3,
            "meta": {},
            "data": {"key": "J3JPQHFW", "version": 3, "itemType": "attachment",
                     "linkMode": "imported_file", "contentType": "text/plain",
                     "filename": "notes.txt", "parentItem": "FLQYTTDI", "tags": []}
        }
    ]"#;
    let children: Vec<ZoteroChildItem> = serde_json::from_str(json).expect("parses");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].data.parent_item.as_deref(), Some("YRGS6H4H"));
    assert_eq!(children[0].data.link_mode.as_deref(), Some("imported_url"));
    assert_eq!(children[0].data.content_type.as_deref(), Some("application/pdf"));

    // Grouped by data.parentItem (the preview/import attachment map).
    let grouped = group_attachments_by_parent(&children);
    assert_eq!(grouped.len(), 2);
    let gray = grouped.get("YRGS6H4H").expect("grouped by parent");
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0].data.filename.as_deref(), Some("Gray - 2024 - A cultural history.pdf"));
    assert!(grouped.contains_key("FLQYTTDI"));
}

#[test]
fn resolve_attachment_path_non_file_scheme_rejected() {
    let err = resolve_attachment_path("http://example.com/a.pdf")
        .expect_err("http Locations must be rejected");
    assert!(matches!(err, ZoteroError::NonFileScheme(_)), "got: {err:?}");
    let err = resolve_attachment_path("https://example.com/a.pdf").expect_err("https rejected");
    assert!(matches!(err, ZoteroError::NonFileScheme(_)), "got: {err:?}");
}

#[test]
fn parse_note_list_response() {
    // Shape captured from `GET /users/0/items?itemType=note` (Zotero 10):
    // note HTML in data.note, the parent in data.parentItem, ISO-8601
    // creation timestamps in data.dateAdded/dateModified.
    let json = r#"[
        {
            "key": "N1N1N1N1", "version": 4,
            "data": {"key": "N1N1N1N1", "version": 4, "itemType": "note",
                     "note": "<p>First <b>note</b></p>", "parentItem": "YRGS6H4H",
                     "tags": [], "dateAdded": "2026-01-02T03:04:05Z",
                     "dateModified": "2026-01-02T03:04:05Z"}
        },
        {
            "key": "N2N2N2N2", "version": 7,
            "data": {"key": "N2N2N2N2", "version": 7, "itemType": "note",
                     "note": "Second note", "parentItem": "FLQYTTDI",
                     "tags": [], "dateAdded": "2026-02-03T04:05:06Z",
                     "dateModified": "2026-02-03T04:05:06Z"}
        }
    ]"#;
    let notes: Vec<bango_lib::zotero::ZoteroNoteItem> = serde_json::from_str(json).expect("parses");
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].data.note.as_deref(), Some("<p>First <b>note</b></p>"));
    assert_eq!(notes[0].data.parent_item.as_deref(), Some("YRGS6H4H"));
    assert_eq!(notes[0].data.date_added.as_deref(), Some("2026-01-02T03:04:05Z"));
    assert_eq!(notes[1].data.date_modified.as_deref(), Some("2026-02-03T04:05:06Z"));

    // Grouped by data.parentItem (the import notes map).
    let grouped = bango_lib::zotero::mapping::group_notes_by_parent(&notes);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped["YRGS6H4H"].len(), 1);
    assert_eq!(grouped["FLQYTTDI"].len(), 1);
}
