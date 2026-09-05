//! Integration tests for the `ensure_initialized` self-healing guard.
//!
//! `ensure_initialized` writes `AGENTS.md` (the wiki contract file) when it is
//! missing, so the ingest/rebuild pipelines transparently recover an
//! uninitialized wiki instead of leaving generated pages invisible behind the
//! wiki-view "Initialize" empty-state gate.

use bango_lib::commands::wiki_cmd::ensure_initialized;
use tempfile::TempDir;

#[test]
fn ensure_initialized_creates_agents_md_when_missing() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Fresh root: no AGENTS.md.
    assert!(!root.join("AGENTS.md").exists());

    let created = ensure_initialized(root).unwrap();
    assert!(created, "should report it created AGENTS.md");

    let agents = root.join("AGENTS.md");
    assert!(agents.exists(), "AGENTS.md should now exist on disk");
    let content = std::fs::read_to_string(&agents).unwrap();
    assert!(!content.is_empty(), "AGENTS.md must not be empty");
}

#[test]
fn ensure_initialized_is_idempotent_when_agents_md_exists() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // First call creates it.
    let created1 = ensure_initialized(root).unwrap();
    assert!(created1);

    // Capture the content after the first write.
    let agents = root.join("AGENTS.md");
    let content1 = std::fs::read_to_string(&agents).unwrap();

    // Second call must NOT overwrite (idempotent) and must report false.
    let created2 = ensure_initialized(root).unwrap();
    assert!(!created2, "second call should report no creation");

    let content2 = std::fs::read_to_string(&agents).unwrap();
    assert_eq!(content1, content2, "existing AGENTS.md must not be overwritten");
}

#[test]
fn ensure_initialized_does_not_overwrite_user_edited_agents_md() {
    // If a user (or a future lint run) edited AGENTS.md, the self-heal guard
    // must preserve their version rather than clobbering it with the bundled
    // default.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let agents = root.join("AGENTS.md");
    std::fs::write(&agents, "# My custom wiki contract\n\nLocal rules.").unwrap();

    let created = ensure_initialized(root).unwrap();
    assert!(!created, "should not recreate an existing AGENTS.md");

    let content = std::fs::read_to_string(&agents).unwrap();
    assert!(
        content.contains("My custom wiki contract"),
        "user-edited AGENTS.md must be preserved, got: {content}"
    );
}
