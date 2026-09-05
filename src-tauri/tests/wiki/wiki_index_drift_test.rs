//! Integration tests for the two-tier wiki index drift detection.
//!
//! Covers the core scenarios for `wiki_check_for_updates`:
//! - External body edit (count unchanged, content changed) -> rebuild.
//! - `touch` (mtime changed, content identical) -> dir-hash update only, no rebuild.
//! - External page add/delete -> rebuild (path-set mismatch).
//! - Internal edit via `rebuild_index_with_manifest` -> no false-positive drift
//!   on the next manifest read.

use bango_lib::db::migration;
use bango_lib::wiki::frontmatter;
use bango_lib::wiki::fts;
use rusqlite::Connection;
use std::collections::HashMap;
use tempfile::TempDir;

fn write_page(root: &std::path::Path, subdir: &str, slug: &str, title: &str, body: &str) {
    let dir = root.join("wiki").join(subdir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fm = frontmatter::Frontmatter::default();
    fm.set("slug", slug);
    fm.set("title", title);
    fm.set("type", "concept");
    fm.set("summary", &format!("{title} summary"));
    fm.set("status", "draft");
    fm.set("source_articles", "[]");
    fm.set("links", "[]");
    frontmatter::write_file(&dir.join(format!("{slug}.md")), &fm, body).unwrap();
}

/// Build a baseline: 2 pages on disk + a fully-built FTS5 index + manifest +
/// dir hash. Returns the tempdir (kept alive for the test body).
fn setup_baseline() -> (TempDir, Connection) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_page(root, "concepts", "sugar-tax", "Sugar Tax", "sugar tax levy on drinks");
    write_page(root, "concepts", "obesity", "Obesity", "obesity public health concern");

    let conn = Connection::open_in_memory().unwrap();
    migration::run_migrations(&conn).unwrap();
    fts::ensure_table(&conn).unwrap();
    fts::rebuild_index_with_manifest(&conn, root).unwrap();
    (tmp, conn)
}

#[test]
fn external_body_edit_triggers_rebuild() {
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Baseline: "sugar" is retrievable.
    assert!(!fts::search(&conn, "sugar", 5).unwrap().is_empty());

    // Simulate an external edit: overwrite the body WITHOUT touching the
    // index or manifest (exactly what an external editor does).
    write_page(root, "concepts", "sugar-tax", "Sugar Tax", "completely new content about levies");
    // The old content is still in the stale index.
    // (We can't easily assert the new content is missing because both bodies
    // mention "levy"; instead we assert the drift is detected.)

    // Recompute the directory fingerprint + per-file hashes (what the command does).
    let rows = fts::collect_page_rows(root).unwrap();
    let disk_dir_hash = fts::compute_directory_fingerprint(&rows).unwrap();
    let disk_file_hashes = fts::compute_file_hashes(&rows).unwrap();
    let stored_dir_hash = fts::get_dir_hash(&conn);
    let stored_manifest = fts::read_manifest(&conn).unwrap();

    // Tier 1: directory fingerprint changed (mtime drift).
    assert_ne!(Some(disk_dir_hash.as_str()), stored_dir_hash.as_deref());

    // Tier 2: per-file content hash changed.
    assert!(fts::manifest_drifted(&stored_manifest, &disk_file_hashes));

    // Rebuild + manifest write.
    fts::rebuild_index_with_manifest(&conn, root).unwrap();

    // Post-rebuild: dir hash + manifest are fresh, so a second check is a fast-path hit.
    let stored_dir_hash2 = fts::get_dir_hash(&conn);
    let rows2 = fts::collect_page_rows(root).unwrap();
    let disk_dir_hash2 = fts::compute_directory_fingerprint(&rows2).unwrap();
    assert_eq!(stored_dir_hash2.as_deref(), Some(disk_dir_hash2.as_str()));
}

#[test]
fn touch_only_does_not_trigger_rebuild() {
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Capture the baseline state.
    let baseline_manifest = fts::read_manifest(&conn).unwrap();
    let baseline_dir_hash = fts::get_dir_hash(&conn).unwrap();

    // Simulate `touch`: re-write the SAME content (so the content hash is
    // identical) but the mtime changes.
    let path = root.join("wiki/concepts/sugar-tax.md");
    let original = std::fs::read_to_string(&path).unwrap();
    // Sleep briefly so the mtime actually advances past the original.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&path, &original).unwrap();

    // Tier 1: directory fingerprint changed (mtime drift).
    let rows = fts::collect_page_rows(root).unwrap();
    let disk_dir_hash = fts::compute_directory_fingerprint(&rows).unwrap();
    assert_ne!(disk_dir_hash, baseline_dir_hash);

    // Tier 2: per-file content hashes are identical.
    let disk_file_hashes = fts::compute_file_hashes(&rows).unwrap();
    assert!(!fts::manifest_drifted(&baseline_manifest, &disk_file_hashes));

    // The command's logic: tier-1 drifted but tier-2 matches -> update only
    // the dir hash, do NOT rebuild. Simulate that update.
    fts::set_dir_hash(&conn, Some(&disk_dir_hash));

    // The manifest is unchanged (no rebuild happened).
    let post_manifest = fts::read_manifest(&conn).unwrap();
    assert_eq!(post_manifest.len(), baseline_manifest.len());
    for (k, v) in &baseline_manifest {
        assert_eq!(post_manifest.get(k), Some(v));
    }
}

#[test]
fn external_page_add_triggers_drift() {
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Add a 3rd page on disk (path-set change).
    write_page(root, "concepts", "exercise", "Exercise", "physical activity health");

    let rows = fts::collect_page_rows(root).unwrap();
    let disk_file_hashes = fts::compute_file_hashes(&rows).unwrap();
    let stored_manifest = fts::read_manifest(&conn).unwrap();

    // Path-set mismatch (3 disk rows vs 2 manifest rows).
    assert!(fts::manifest_drifted(&stored_manifest, &disk_file_hashes));
}

#[test]
fn external_page_delete_triggers_drift() {
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Delete one page on disk.
    std::fs::remove_file(root.join("wiki/concepts/obesity.md")).unwrap();

    let rows = fts::collect_page_rows(root).unwrap();
    let disk_file_hashes = fts::compute_file_hashes(&rows).unwrap();
    let stored_manifest = fts::read_manifest(&conn).unwrap();

    // Path-set mismatch (1 disk row vs 2 manifest rows).
    assert!(fts::manifest_drifted(&stored_manifest, &disk_file_hashes));
}

#[test]
fn rebuild_index_with_manifest_prevents_false_positive_drift() {
    // After an internal edit via `rebuild_index_with_manifest`, the next
    // drift check should NOT detect drift (fast-path tier-1 hit).
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Internal edit: rewrite a page via the manifest-aware rebuild.
    write_page(root, "concepts", "sugar-tax", "Sugar Tax", "updated by internal edit");
    fts::rebuild_index_with_manifest(&conn, root).unwrap();

    // Simulate the next check: recompute disk hashes and compare.
    let rows = fts::collect_page_rows(root).unwrap();
    let disk_dir_hash = fts::compute_directory_fingerprint(&rows).unwrap();
    let disk_file_hashes = fts::compute_file_hashes(&rows).unwrap();
    let stored_dir_hash = fts::get_dir_hash(&conn);
    let stored_manifest = fts::read_manifest(&conn).unwrap();

    // Tier 1: dir hash matches (fast path).
    assert_eq!(stored_dir_hash.as_deref(), Some(disk_dir_hash.as_str()));
    // Tier 2 (wouldn't even run in the command, but verify): manifest matches.
    assert!(!fts::manifest_drifted(&stored_manifest, &disk_file_hashes));
}

#[test]
fn empty_wiki_clears_stale_baseline() {
    // When pages are deleted from disk leaving an empty wiki, the check
    // should clear any stale dir hash + manifest rather than crashing.
    let (tmp, conn) = setup_baseline();
    let root = tmp.path();

    // Delete all wiki pages.
    std::fs::remove_file(root.join("wiki/concepts/sugar-tax.md")).unwrap();
    std::fs::remove_file(root.join("wiki/concepts/obesity.md")).unwrap();

    let rows = fts::collect_page_rows(root).unwrap();
    let dir_hash = fts::compute_directory_fingerprint(&rows);
    assert!(dir_hash.is_none(), "empty wiki should produce no dir hash");

    // The command clears the stale baseline.
    fts::set_dir_hash(&conn, None);
    fts::write_manifest(&conn, &[]).unwrap();
    assert!(fts::get_dir_hash(&conn).is_none());
    assert!(fts::read_manifest(&conn).unwrap().is_empty());
}

#[test]
fn directory_fingerprint_is_deterministic_across_readdir_order() {
    // The tier-1 hash sorts by rel_path, so two runs that observe the same
    // files in different readdir order produce the same digest.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_page(root, "concepts", "zebra", "Zebra", "z");
    write_page(root, "concepts", "alpha", "Alpha", "a");
    write_page(root, "concepts", "mango", "Mango", "m");

    let rows1 = fts::collect_page_rows(root).unwrap();
    // Manually reverse the order (simulates a different readdir order).
    let mut rows2 = rows1.clone();
    rows2.reverse();
    let hash1 = fts::compute_directory_fingerprint(&rows1).unwrap();
    let hash2 = fts::compute_directory_fingerprint(&rows2).unwrap();
    assert_eq!(hash1, hash2, "fingerprint must be order-independent");
}

#[test]
fn manifest_round_trip_preserves_all_entries() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_page(root, "concepts", "alpha", "Alpha", "a");
    write_page(root, "authors", "beta", "Beta", "b");

    let conn = Connection::open_in_memory().unwrap();
    migration::run_migrations(&conn).unwrap();

    let rows = fts::collect_page_rows(root).unwrap();
    let file_hashes = fts::compute_file_hashes(&rows).unwrap();
    fts::write_manifest(&conn, &file_hashes).unwrap();

    let read_back = fts::read_manifest(&conn).unwrap();
    assert_eq!(read_back.len(), file_hashes.len());
    let disk_map: HashMap<String, String> = file_hashes.into_iter().collect();
    for (path, hash) in &disk_map {
        assert_eq!(read_back.get(path), Some(hash));
    }
}
