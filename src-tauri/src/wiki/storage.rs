//! Wiki storage resolution.
//!
//! Resolves the `wiki-root/` directory as a **sibling** of the project's
//! `fulltext_storage_dir` under the Bango documents root (or an optional
//! explicit override in `app_settings` under key `wiki_root_dir`).
//!
//! Default layout:
//! ```text
//! ~/Documents/Bango/
//! ├── fulltext/      <- article PDFs + text extracts (existing)
//! └── wiki-root/     <- LLM wiki (this module)
//! ```
//!
//! When the user sets a custom `fulltext_storage_dir` whose last path
//! component is `fulltext`, the wiki-root is placed in that dir's parent
//! (e.g. `/x/y/fulltext/` -> `/x/y/wiki-root/`). If the custom dir does not
//! end in `fulltext`, the wiki-root is placed inside it.

use std::path::{Path, PathBuf};

use crate::db::app_settings_repo;
use crate::error::AppError;

/// The `app_settings` key for an optional explicit wiki-root override.
/// When unset (or empty), the wiki root is derived from the Bango documents
/// root (see [`derive_bango_root`]).
pub const WIKI_ROOT_DIR_KEY: &str = "wiki_root_dir";

/// Subdirectory name placed under the Bango documents root.
pub const WIKI_ROOT_DIR_NAME: &str = "wiki-root";

/// The conventional last component of the fulltext storage dir.
const FULLTEXT_DIR_NAME: &str = "fulltext";

/// Subdirectories created inside `wiki-root/`.
pub const SUBDIRS: &[&str] =
    &["raw", "wiki/concepts", "wiki/authors", "wiki/methods", "wiki/synthesis", "templates"];

/// Derive the Bango documents root from the fulltext storage dir.
///
/// If the storage dir's last component is `fulltext`, the root is its parent
/// (so the wiki becomes a sibling of `fulltext/`). Otherwise the storage dir
/// itself is treated as the root.
#[must_use]
pub fn derive_bango_root(fulltext_storage_dir: &Path) -> PathBuf {
    if fulltext_storage_dir.file_name().and_then(|n| n.to_str()) == Some(FULLTEXT_DIR_NAME) {
        fulltext_storage_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| fulltext_storage_dir.to_path_buf())
    } else {
        fulltext_storage_dir.to_path_buf()
    }
}

/// Resolve the effective wiki-root directory.
///
/// Order: explicit `wiki_root_dir` setting -> `{bango_documents_root}/wiki-root`
/// (where the Bango root is derived from `fulltext_storage_dir`).
/// Ensures the directory exists.
pub fn resolve_root(conn: &rusqlite::Connection) -> Result<PathBuf, AppError> {
    let explicit = app_settings_repo::get_setting(conn, WIKI_ROOT_DIR_KEY)?;
    let root = if let Some(p) = explicit.filter(|p| !p.is_empty()) {
        PathBuf::from(p)
    } else {
        let storage_str = app_settings_repo::get_fulltext_storage_dir(conn)?;
        derive_bango_root(Path::new(&storage_str)).join(WIKI_ROOT_DIR_NAME)
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

/// Scaffold the full standard directory tree under `wiki-root/`.
/// Idempotent: existing directories are kept.
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

/// Compute the default wiki-root path from the fulltext storage dir.
///
/// `compute_default_root("~/Documents/Bango/fulltext")` ->
/// `"~/Documents/Bango/wiki-root"`.
#[must_use]
pub fn compute_default_root(fulltext_storage_dir: &Path) -> PathBuf {
    derive_bango_root(fulltext_storage_dir).join(WIKI_ROOT_DIR_NAME)
}

/// Delete the entire wiki-root directory tree (including `AGENTS.md`,
/// `templates/`, `raw/`, and `wiki/`). Used by `reset_project` (Delete All
/// Data) so a full project reset also clears the on-disk wiki. Unlike
/// `wipe_generated`, nothing is preserved.
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

/// Wipe the generated content (`raw/` + `wiki/`), keeping the root, `AGENTS.md`,
/// `templates/`, and `log.md`. Used by `wiki_delete_wiki`.
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

/// Count `.md` files under a subdirectory (non-recursive top level for `raw`,
/// recursive for `wiki`). Returns 0 if the dir does not exist yet.
pub fn count_markdown(root: &Path, subdir: &str, recursive: bool) -> usize {
    let base = root.join(subdir);
    if !base.exists() {
        return 0;
    }
    walk_markdown(&base, recursive).len()
}

/// Collect all `.md` file paths under `base`, optionally recursive.
fn walk_markdown(base: &Path, recursive: bool) -> Vec<PathBuf> {
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
    fn derive_bango_root_strips_fulltext_suffix() {
        // ~/Documents/Bango/fulltext -> ~/Documents/Bango
        let root = derive_bango_root(Path::new("/home/user/Documents/Bango/fulltext"));
        assert_eq!(root, Path::new("/home/user/Documents/Bango"));
    }

    #[test]
    fn derive_bango_root_keeps_non_fulltext_dir() {
        // Custom dir without `fulltext` suffix: treated as the root itself.
        let root = derive_bango_root(Path::new("/my/custom/storage"));
        assert_eq!(root, Path::new("/my/custom/storage"));
    }

    #[test]
    fn derive_bango_root_handles_bare_fulltext() {
        // Edge case: the path is exactly `fulltext` -> parent is empty (current dir).
        let root = derive_bango_root(Path::new("fulltext"));
        // parent of "fulltext" is "" which normalizes to "."
        assert_eq!(root, Path::new(""));
    }

    #[test]
    fn compute_default_root_places_wiki_as_sibling_of_fulltext() {
        // The key contract: wiki-root is a SIBLING of fulltext, not nested inside it.
        let storage = Path::new("/home/user/Documents/Bango/fulltext");
        let root = compute_default_root(storage);
        assert_eq!(root, Path::new("/home/user/Documents/Bango/wiki-root"));
        // Explicitly NOT inside fulltext:
        assert!(!root.starts_with("/home/user/Documents/Bango/fulltext"));
    }

    #[test]
    fn compute_default_root_custom_non_fulltext_storage() {
        let storage = Path::new("/data/bango-store");
        let root = compute_default_root(storage);
        assert_eq!(root, Path::new("/data/bango-store/wiki-root"));
    }
}
