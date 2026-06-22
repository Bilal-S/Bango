use std::path::Path;

use bango_lib::commands::export_cmd::reset_project_inner;
use bango_lib::db::app_settings_repo::{get_setting, set_setting};
use bango_lib::db::migration::run_migrations;
use bango_lib::wiki::storage;
use rusqlite::Connection;
use tempfile::TempDir;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    conn
}

/// Configure a wiki root that points at a temp dir so the test does not touch
/// the user's real documents folder.
fn configure_wiki_root(conn: &Connection, root: &Path) {
    set_setting(conn, storage::WIKI_ROOT_DIR_KEY, root.to_str()).unwrap();
}

/// Build a wiki-root dir tree with some content so deletion is observable.
fn seed_wiki(root: &Path) {
    storage::scaffold_tree(root).unwrap();
    std::fs::write(root.join("AGENTS.md"), "# contract").unwrap();
    std::fs::write(root.join("wiki/concepts/sugar-tax.md"), "# Sugar Tax").unwrap();
    std::fs::write(root.join("raw/art-1.md"), "x").unwrap();
}

#[test]
fn reset_deletes_wiki_root_directory() {
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);
    assert!(wiki_root.exists());

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);

    reset_project_inner(&mut conn).unwrap();

    // The entire wiki-root directory is gone after reset.
    assert!(!wiki_root.exists());
}

#[test]
fn reset_clears_app_settings_after_rebuild() {
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);

    // The override exists before reset.
    assert_eq!(
        get_setting(&conn, storage::WIKI_ROOT_DIR_KEY).unwrap(),
        Some(wiki_root.to_string_lossy().to_string())
    );

    reset_project_inner(&mut conn).unwrap();

    // After reset, app_settings is dropped and recreated empty by migrations.
    assert!(get_setting(&conn, storage::WIKI_ROOT_DIR_KEY).unwrap().is_none());
}

#[test]
fn reset_succeeds_even_when_wiki_root_is_missing() {
    // If the wiki root was never scaffolded, reset must still succeed (the
    // resolve_root call creates an empty dir, which is then deleted).
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("never-existed");

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);

    reset_project_inner(&mut conn).unwrap();

    assert!(!wiki_root.exists());
}
