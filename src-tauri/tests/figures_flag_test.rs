//! Tests for the persisted `has_figures_or_tables` flag.
//!
//! The flag is computed at attach time in `commands::full_text::attach_full_text_inner`
//! via `utils::sections::extract_captions` (the same detector
//! `generate_figure_descriptions` validates against). These tests verify:
//!
//! 1. Attaching a TXT whose text contains a `Figure 1.` caption line sets the
//!    flag to `true`.
//! 2. Attaching a TXT with plain prose (no caption lines) sets the flag to
//!    `false`.
//! 3. `clear_full_text` resets the flag to `false`.
//!
//! Uses the same in-memory DB + tempdir pattern as `batch_import_test.rs`.

use std::io::Write;
use std::path::PathBuf;

use bango_lib::commands::full_text::{attach_full_text_inner, compute_storage_dir};
use bango_lib::db::app_settings_repo::{set_setting, STORAGE_ROOT_KEY};
use bango_lib::db::article_repo;
use bango_lib::db::migration::run_migrations;
use bango_lib::models::article::NewArticle;
use rusqlite::Connection;
use tempfile::TempDir;

/// In-memory DB with all migrations applied.
fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Configure the storage root to point at a temp dir.
fn configure_storage_root(conn: &Connection, root: &std::path::Path) {
    set_setting(conn, STORAGE_ROOT_KEY, root.to_str()).unwrap();
    std::fs::create_dir_all(root.join("fulltext")).unwrap();
}

/// Insert a minimal article and return its id.
fn seed_article(conn: &Connection) -> String {
    let article = NewArticle {
        title: "Test Article".to_string(),
        abstract_text: "Abstract.".to_string(),
        ..Default::default()
    };
    article_repo::insert_article(conn, &article).expect("insert article").id
}

/// Write a `.txt` file with the given content under the temp dir's `fulltext/`
/// subdir and return its path.
fn write_txt(root: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = root.join("fulltext").join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "{content}").unwrap();
    path
}

#[test]
fn attach_sets_has_figures_or_tables_true_when_caption_present() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let id = seed_article(&conn);

    // Text containing a figure caption line (matches CAPTION_START_RE).
    let text = "Introduction.\n\nFigure 1. Study design overview showing the flow of participants.\n\nResults were significant.";
    let path = write_txt(tmp.path(), "captioned.txt", text);

    attach_full_text_inner(&conn, &id, &path, &storage_dir).expect("attach");

    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "article should have full text");
    assert!(article.has_figures_or_tables, "flag should be true when a figure caption is present");
}

#[test]
fn attach_sets_has_figures_or_tables_false_for_plain_prose() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let id = seed_article(&conn);

    // Plain prose with no caption lines.
    let text = "This study examined the effect of a sugar tax on beverage purchases.\n\nWe used a difference-in-differences design.";
    let path = write_txt(tmp.path(), "plain.txt", text);

    attach_full_text_inner(&conn, &id, &path, &storage_dir).expect("attach");

    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_full_text, "article should have full text");
    assert!(
        !article.has_figures_or_tables,
        "flag should be false for plain prose with no captions"
    );
}

#[test]
fn attach_sets_flag_true_for_table_caption() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let id = seed_article(&conn);

    // Text containing a table caption (extract_captions detects both Figure and Table).
    let text = "Methods.\n\nTable 1. Baseline characteristics of study participants by group.\n\nGroup A had 200 participants.";
    let path = write_txt(tmp.path(), "table.txt", text);

    attach_full_text_inner(&conn, &id, &path, &storage_dir).expect("attach");

    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_figures_or_tables, "flag should be true when a table caption is present");
}

#[test]
fn clear_full_text_resets_has_figures_or_tables() {
    let tmp = TempDir::new().unwrap();
    let conn = test_db();
    configure_storage_root(&conn, tmp.path());
    let storage_dir = compute_storage_dir(&conn).unwrap();

    let id = seed_article(&conn);

    // Attach with a caption so the flag goes true.
    let text = "Figure 1. Overview.";
    let path = write_txt(tmp.path(), "captioned.txt", text);
    attach_full_text_inner(&conn, &id, &path, &storage_dir).expect("attach");
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(article.has_figures_or_tables, "flag should be true after attach");

    // Clear; flag must reset to false.
    article_repo::clear_full_text(&conn, &id).expect("clear");
    let article = article_repo::get_article_by_id(&conn, &id).unwrap();
    assert!(!article.has_full_text, "has_full_text should be false after clear");
    assert!(!article.has_figures_or_tables, "flag should be false after clear_full_text");
}
