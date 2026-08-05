//! Wiki storage: resolves `wiki-root/` under the project storage root (`{storage_root}/wiki-root`),
//! with an optional `wiki_root_dir` override. Default layout: `fulltext/`, `ris/`, `wiki-root/`.

use std::path::{Path, PathBuf};

use crate::db::app_settings_repo;
use crate::error::AppError;

/// The `app_settings` key for an optional explicit wiki-root override.
/// When unset (or empty), the wiki root is `{storage_root}/wiki-root`.
pub const WIKI_ROOT_DIR_KEY: &str = "wiki_root_dir";

/// Subdirectory name placed under the storage root.
pub const WIKI_ROOT_DIR_NAME: &str = "wiki-root";

/// Subdirectories created inside `wiki-root/`.
pub const SUBDIRS: &[&str] =
    &["raw", "wiki/concepts", "wiki/authors", "wiki/methods", "wiki/synthesis", "templates"];

/// Resolve the effective wiki-root: explicit override → `{storage_root}/wiki-root`. Creates dir if needed.
pub fn resolve_root(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let explicit = app_settings_repo::get_setting(conn, WIKI_ROOT_DIR_KEY)?;
    let root = if let Some(p) = explicit.filter(|p| !p.is_empty()) {
        PathBuf::from(p)
    } else {
        let storage_str = app_settings_repo::get_storage_root(conn)?;
        PathBuf::from(storage_str).join(WIKI_ROOT_DIR_NAME)
    };
    ensure_root_exists(&root)?;
    Ok(root)
}

/// Whether an explicit override has been configured (vs derived default).
pub fn has_explicit_override(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    Ok(app_settings_repo::get_setting(conn, WIKI_ROOT_DIR_KEY)?
        .map(|v| !v.is_empty())
        .unwrap_or(false))
}

/// Ensure the wiki-root directory exists. Does not create subdirs.
pub fn ensure_root_exists(root: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(root).map_err(|e| {
        AppError::Import(format!(
            "Failed to create wiki-root directory '{}': {}",
            root.display(),
            e
        ))
    })?;
    Ok(())
}

/// Scaffold the standard directory tree under `wiki-root/`. Idempotent.
pub fn scaffold_tree(root: &Path) -> Result<(), AppError> {
    ensure_root_exists(root)?;
    for sub in SUBDIRS {
        let path = root.join(sub);
        std::fs::create_dir_all(&path).map_err(|e| {
            AppError::Import(format!("Failed to create wiki subdir '{}': {}", path.display(), e))
        })?;
    }
    Ok(())
}

/// Compute the default wiki-root path from the storage root.
/// `compute_default_root("~/Documents/Bango")` -> `"~/Documents/Bango/wiki-root"`.
#[must_use]
pub fn compute_default_root(storage_root: &Path) -> PathBuf {
    storage_root.join(WIKI_ROOT_DIR_NAME)
}

/// Delete the entire wiki-root directory tree including `AGENTS.md`, `templates/`, `raw/`, `wiki/`.
/// Used by `reset_project` (Delete All Data). Unlike `wipe_generated`, nothing is preserved.
pub fn delete_wiki_root(root: &Path) -> Result<(), AppError> {
    if root.exists() {
        std::fs::remove_dir_all(root).map_err(|e| {
            AppError::Import(format!(
                "Failed to delete wiki-root directory '{}': {}",
                root.display(),
                e
            ))
        })?;
    }
    Ok(())
}

/// Wipe generated content (`raw/` + `wiki/`), keeping root, `AGENTS.md`, `templates/`, `log.md`.
/// Used by `wiki_delete_wiki`.
pub fn wipe_generated(root: &Path) -> Result<(), AppError> {
    for target in ["raw", "wiki"] {
        let path = root.join(target);
        if path.exists() {
            std::fs::remove_dir_all(&path).map_err(|e| {
                AppError::Import(format!("Failed to remove '{}': {}", path.display(), e))
            })?;
        }
    }
    // Recreate the empty dirs so the tree stays valid.
    scaffold_tree(root)?;
    Ok(())
}

/// Count `.md` files under a subdirectory (non-recursive for `raw`, recursive for `wiki`).
/// Returns 0 if the dir does not exist.
pub fn count_markdown(root: &Path, subdir: &str, recursive: bool) -> usize {
    let base = root.join(subdir);
    if !base.exists() {
        return 0;
    }
    walk_markdown(&base, recursive).len()
}

/// Collect all `.md` file paths under `base`, optionally recursive.
pub(crate) fn walk_markdown(base: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(base) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                out.extend(walk_markdown(&path, true));
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn scaffold_tree_creates_all_subdirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        scaffold_tree(&root).unwrap();
        for sub in SUBDIRS {
            assert!(root.join(sub).exists(), "missing subdir: {sub}");
        }
    }

    #[test]
    fn scaffold_tree_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        scaffold_tree(&root).unwrap();
        // second call must not error
        scaffold_tree(&root).unwrap();
    }

    #[test]
    fn delete_wiki_root_removes_entire_tree() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        scaffold_tree(&root).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# contract").unwrap();
        std::fs::write(root.join("templates/concept.md"), "# template").unwrap();
        std::fs::write(root.join("raw/art-1.md"), "x").unwrap();
        std::fs::write(root.join("wiki/concepts/c.md"), "y").unwrap();

        delete_wiki_root(&root).unwrap();

        // The entire wiki-root directory (and everything under it) is gone.
        assert!(!root.exists());
    }

    #[test]
    fn delete_wiki_root_is_noop_when_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("does-not-exist");
        // Does not error when the directory does not exist.
        delete_wiki_root(&root).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn wipe_generated_keeps_templates_and_agents_md() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        scaffold_tree(&root).unwrap();
        std::fs::write(root.join("AGENTS.md"), "# contract").unwrap();
        std::fs::write(root.join("templates/concept.md"), "# template").unwrap();
        std::fs::write(root.join("raw/art-1.md"), "x").unwrap();
        std::fs::write(root.join("wiki/concepts/c.md"), "y").unwrap();

        wipe_generated(&root).unwrap();

        assert!(root.join("AGENTS.md").exists());
        assert!(root.join("templates/concept.md").exists());
        // raw + wiki were wiped then recreated as empty dirs
        assert!(root.join("raw").exists());
        assert!(root.join("wiki").exists());
        assert_eq!(count_markdown(&root, "raw", false), 0);
        assert_eq!(count_markdown(&root, "wiki", true), 0);
    }

    #[test]
    fn count_markdown_handles_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        assert_eq!(count_markdown(&root, "raw", false), 0);
    }

    #[test]
    fn count_markdown_recursive_and_flat() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("wiki-root");
        scaffold_tree(&root).unwrap();
        std::fs::write(root.join("wiki/concepts/a.md"), "x").unwrap();
        std::fs::write(root.join("wiki/concepts/b.md"), "y").unwrap();
        std::fs::write(root.join("wiki/index.md"), "z").unwrap();
        assert_eq!(count_markdown(&root, "wiki", true), 3);
    }

    #[test]
    fn compute_default_root_places_wiki_under_storage_root() {
        // The key contract: wiki-root is a child of the storage root, as a
        // sibling of `fulltext/` and `ris/`, never nested inside either.
        let storage = Path::new("/home/user/Documents/Bango");
        let root = compute_default_root(storage);
        assert_eq!(root, Path::new("/home/user/Documents/Bango/wiki-root"));
        assert!(root.starts_with("/home/user/Documents/Bango"));
        assert!(!root.starts_with("/home/user/Documents/Bango/fulltext"));
        assert!(!root.starts_with("/home/user/Documents/Bango/ris"));
    }

    #[test]
    fn compute_default_root_custom_storage() {
        let storage = Path::new("/data/bango-store");
        let root = compute_default_root(storage);
        assert_eq!(root, Path::new("/data/bango-store/wiki-root"));
    }
}
