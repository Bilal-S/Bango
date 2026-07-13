//! Integration tests for the wiki static-site export (`wiki_generate_export` +
//! `wiki_zip_export`).
//!
//! These tests exercise the pure helper functions (`generate_export_inner`,
//! `copy_wiki_markdown_tree`, `copy_user_doc_markdown`) directly, avoiding the
//! Tauri `State<DbState>` wrapper that cannot be unit-tested.

use std::fs;

use bango_lib::commands::wiki_cmd::{generate_export_inner, ExportFile, SiteExportBundle};
use tempfile::TempDir;

/// Build a minimal wiki-root tree for testing.
fn make_wiki_root() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // wiki/ with pages + log.md (should be excluded from markdown/).
    fs::create_dir_all(root.join("wiki/concepts")).unwrap();
    fs::write(root.join("wiki/concepts/sugar-tax.md"), "---\nslug: sugar-tax\n---\n# Sugar Tax")
        .unwrap();
    fs::write(root.join("wiki/log.md"), "# Audit Log (should be excluded)").unwrap();

    // raw/ with one article-export (should NOT be copied) + one user doc
    // (should be copied).
    fs::create_dir_all(root.join("raw")).unwrap();
    fs::write(
        root.join("raw/art-123.md"),
        "---\nid: art-123\ntype: source\nslug: art-123\n---\nArticle body",
    )
    .unwrap();
    fs::write(
        root.join("raw/user-notes.md"),
        "---\nid: user-notes\ntype: source\nslug: user-notes\nsource_kind: user_text\n---\nUser notes",
    )
    .unwrap();

    tmp
}

/// Build a simple export bundle for testing.
fn test_bundle() -> SiteExportBundle {
    SiteExportBundle {
        files: vec![
            ExportFile {
                path: "index.html".to_string(),
                content: "<html>Index</html>".to_string(),
            },
            ExportFile {
                path: "pages/concepts/sugar-tax.html".to_string(),
                content: "<html>Sugar Tax</html>".to_string(),
            },
            ExportFile {
                path: "style.css".to_string(),
                content: "body { color: black; }".to_string(),
            },
        ],
        project_title: "Test Wiki".to_string(),
    }
}

#[test]
fn generate_export_writes_all_files() {
    let wiki_root = make_wiki_root();

    let result = generate_export_inner(wiki_root.path(), &test_bundle()).unwrap();

    // All HTML/CSS files from the bundle are written.
    assert!(result.index_path.ends_with("index.html"));
    assert!(fs::read_to_string(wiki_root.path().join("wiki-export/index.html"))
        .unwrap()
        .contains("Index"));
    assert!(fs::read_to_string(wiki_root.path().join("wiki-export/pages/concepts/sugar-tax.html"))
        .unwrap()
        .contains("Sugar Tax"));
    assert!(fs::read_to_string(wiki_root.path().join("wiki-export/style.css"))
        .unwrap()
        .contains("color: black"));
}

#[test]
fn markdown_tree_excludes_log_and_articles() {
    let wiki_root = make_wiki_root();
    generate_export_inner(wiki_root.path(), &test_bundle()).unwrap();

    let export_dir = wiki_root.path().join("wiki-export");

    // wiki/concepts/sugar-tax.md IS included (wiki-generated, safe).
    assert!(export_dir.join("markdown/concepts/sugar-tax.md").exists());

    // log.md is excluded.
    assert!(!export_dir.join("markdown/log.md").exists());

    // raw/art-123.md is NOT copied (article text, copyright).
    assert!(!export_dir.join("markdown/sources/art-123.md").exists());
}

#[test]
fn user_docs_markdown_included() {
    let wiki_root = make_wiki_root();
    generate_export_inner(wiki_root.path(), &test_bundle()).unwrap();

    let export_dir = wiki_root.path().join("wiki-export");

    // raw/user-notes.md IS copied (source_kind: user_*).
    assert!(export_dir.join("markdown/sources/user-notes.md").exists());

    // Verify content round-trip.
    let user_notes = fs::read_to_string(export_dir.join("markdown/sources/user-notes.md")).unwrap();
    assert!(user_notes.contains("User notes"));
}

#[test]
fn generate_export_clears_previous_output() {
    let wiki_root = make_wiki_root();

    // First generation.
    let bundle1 = SiteExportBundle {
        files: vec![ExportFile {
            path: "old-file.html".to_string(),
            content: "<html>Old</html>".to_string(),
        }],
        project_title: "Old".to_string(),
    };
    generate_export_inner(wiki_root.path(), &bundle1).unwrap();
    let export_dir = wiki_root.path().join("wiki-export");
    assert!(export_dir.join("old-file.html").exists());

    // Second generation: old file must be gone (directory was cleared).
    generate_export_inner(wiki_root.path(), &test_bundle()).unwrap();
    assert!(!export_dir.join("old-file.html").exists());
    assert!(export_dir.join("index.html").exists());
}
