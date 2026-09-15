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

    reset_project_inner(&mut conn, false).unwrap();

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

    reset_project_inner(&mut conn, false).unwrap();

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

    reset_project_inner(&mut conn, false).unwrap();

    assert!(!wiki_root.exists());
}

#[test]
fn reset_runs_vacuum_without_error() {
    // reset_project_inner calls vacuum_database after the schema rebuild.
    // On an in-memory DB the VACUUM is a no-op for size but must execute
    // without error, proving the VACUUM step is wired in and runs cleanly
    // against the freshly-rebuilt schema. The space-reclaim behavior itself
    // is proven by `tests/db/maintenance_test.rs` against a file-backed DB.
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);

    // Must not panic / return Err: the VACUUM step runs after rebuild_schema
    // and after the WAL checkpoint inside vacuum_database.
    reset_project_inner(&mut conn, false).unwrap();

    // The schema is still usable after the post-reset VACUUM: a trivial query
    // against a rebuilt table returns the expected empty result.
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0);
}

// ── LLM config preservation (Start New Project) ──────────────────────

/// Seed a fully tuned `llm_config` row with an opaque key blob. Raw SQL (not
/// `save_config`) so no PBKDF2/crypto runs and the test stays fast; the reset
/// path preserves the stored blob verbatim, so any string works.
fn seed_llm_config(conn: &Connection) {
    conn.execute(
        "INSERT INTO llm_config (id, provider, endpoint_url, api_key_encrypted, model_name, \
         temperature, skip_temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
         VALUES (1, 'anthropic', 'https://api.anthropic.com/v1', 'opaque-key-blob', \
         'claude-sonnet-4-5', 0.4, 1, 5, 250, 200000)",
        [],
    )
    .unwrap();
}

fn llm_config_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM llm_config", [], |r| r.get(0)).unwrap()
}

#[test]
fn reset_wipes_llm_config_when_not_preserved() {
    // Delete All Data path: the full wipe stays in effect.
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);
    seed_llm_config(&conn);

    reset_project_inner(&mut conn, false).unwrap();

    assert_eq!(llm_config_count(&conn), 0, "Delete All Data must wipe llm_config");
}

#[test]
fn reset_preserve_llm_config_keeps_row_verbatim() {
    // Start New Project path: provider, encrypted key blob, and all tuning
    // survive the schema rebuild byte-identically.
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);
    seed_llm_config(&conn);

    reset_project_inner(&mut conn, true).unwrap();

    let (provider, key, endpoint, model, temperature, skip, concurrency, delay, ctx): (
        String,
        Option<String>,
        String,
        String,
        f64,
        i32,
        i32,
        i32,
        i32,
    ) = conn
        .query_row(
            "SELECT provider, api_key_encrypted, endpoint_url, model_name, temperature, \
             skip_temperature, max_concurrent_requests, request_delay_ms, context_window_tokens \
             FROM llm_config WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(provider, "anthropic");
    assert_eq!(key.as_deref(), Some("opaque-key-blob"), "encrypted blob preserved verbatim");
    assert_eq!(endpoint, "https://api.anthropic.com/v1");
    assert_eq!(model, "claude-sonnet-4-5");
    assert_eq!(temperature, 0.4);
    assert_eq!(skip, 1);
    assert_eq!(concurrency, 5);
    assert_eq!(delay, 250);
    assert_eq!(ctx, 200000);
}

#[test]
fn reset_preserve_llm_config_without_row_is_noop() {
    // Preserving with no existing row must not fabricate one.
    let tmp = TempDir::new().unwrap();
    let wiki_root = tmp.path().join("wiki-root");
    seed_wiki(&wiki_root);

    let mut conn = test_db();
    configure_wiki_root(&conn, &wiki_root);

    reset_project_inner(&mut conn, true).unwrap();

    assert_eq!(llm_config_count(&conn), 0);
}
