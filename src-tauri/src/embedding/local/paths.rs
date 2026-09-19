//! Artifact path resolution for Bango Local embeddings (pure; no I/O).
//!
//! Model files normally live under `{storage_root}/model/` so they follow the
//! user's Bango documents directory. When that directory is OneDrive-synced
//! (Windows Known Folder backup), large binary artifacts churn sync quota,
//! hit file locks during atomic renames, and can be dehydrated to cloud-only
//! placeholders - breaking the offline guarantee. In that case the model root
//! falls back to the OS local-app-data dir. The ONNX Runtime library always
//! lives in local app data: it is a redownloadable binary cache, never user
//! data.

use std::path::{Path, PathBuf};

/// Subdirectory under the storage root holding local embedding models.
pub const MODEL_DIR_NAME: &str = "model";

/// App-data subdirectory for Bango AI artifacts (fallback model store + runtimes).
pub const AI_DIR_NAME: &str = "ai";

/// Resolved artifact roots for the local embedding backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPaths {
    /// Root for model payloads (`<model_root>/<profile>/...`).
    pub model_root: PathBuf,
    /// Root for the ONNX Runtime library (versioned subdirectories).
    pub runtime_root: PathBuf,
    /// Whether the model root fell back to app data (OneDrive detected).
    pub used_fallback: bool,
}

/// Whether any path component marks a OneDrive-synced directory.
///
/// Matches `OneDrive` (personal) and `OneDrive - <tenant>` (business) as whole
/// path segments, case-insensitively. The tenant form requires the
/// `" - "` separator so similarly-named directories (`OneDrive Stuff`,
/// `OneDriveBackup`) do not trigger the fallback. Pure: no filesystem access.
#[must_use]
pub fn is_onedrive_path(path: &Path) -> bool {
    path.components().any(|component| {
        let segment = component.as_os_str().to_string_lossy();
        let segment = segment.trim().to_ascii_lowercase();
        segment == "onedrive" || segment.starts_with("onedrive - ")
    })
}

/// Resolve artifact roots from the storage root plus an explicit local-app-data
/// base. Pure: the base is a parameter so tests stay platform-independent.
///
/// - Storage root under OneDrive (base present): the model root falls back to
///   `{data_local}/Bango/ai/models` and `used_fallback` is `true`.
/// - Otherwise: the model root is `{storage_root}/model`.
/// - The runtime root always derives from `data_local` when available,
///   degrading to `{storage_root}/ai/runtimes` only when the platform exposes
///   no app-data dir (documented degradation, never a panic).
#[must_use]
pub fn resolve_ai_paths_with_base(storage_root: &Path, data_local: Option<&Path>) -> AiPaths {
    let onedrive = is_onedrive_path(storage_root);
    let (model_root, used_fallback) = match data_local.filter(|_| onedrive) {
        Some(base) => (base.join("Bango").join(AI_DIR_NAME).join("models"), true),
        None => (storage_root.join(MODEL_DIR_NAME), false),
    };
    let runtime_root = match data_local {
        Some(base) => base.join("Bango").join(AI_DIR_NAME).join("runtimes"),
        None => storage_root.join(AI_DIR_NAME).join("runtimes"),
    };
    AiPaths { model_root, runtime_root, used_fallback }
}

/// Resolve artifact roots using this machine's actual local-app-data dir.
#[must_use]
pub fn resolve_ai_paths(storage_root: &Path) -> AiPaths {
    resolve_ai_paths_with_base(storage_root, dirs::data_local_dir().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onedrive_personal_segment_detected() {
        assert!(is_onedrive_path(Path::new("/Users/alice/OneDrive/Documents/Bango")));
    }

    #[test]
    fn onedrive_business_segment_detected() {
        assert!(is_onedrive_path(Path::new("/Users/alice/OneDrive - Contoso/Documents/Bango")));
    }

    #[test]
    fn onedrive_detection_is_case_insensitive() {
        assert!(is_onedrive_path(Path::new("/Users/alice/ONEDRIVE/Documents")));
        assert!(is_onedrive_path(Path::new("/Users/alice/onedrive/Documents")));
    }

    #[test]
    fn plain_documents_path_not_flagged() {
        assert!(!is_onedrive_path(Path::new("/home/alice/Documents/Bango")));
        // Similarly-named directories must not match (whole-segment +
        // tenant-separator rules).
        assert!(!is_onedrive_path(Path::new("/home/alice/OneDriveBackup")));
        assert!(!is_onedrive_path(Path::new("/home/alice/OneDrive Stuff")));
        assert!(!is_onedrive_path(Path::new("/home/alice/OneDrive Backups")));
    }

    #[test]
    fn resolve_normal_root_uses_storage_model_dir() {
        let storage = Path::new("/home/alice/Documents/Bango");
        let data = Path::new("/home/alice/.local/share");
        let paths = resolve_ai_paths_with_base(storage, Some(data));
        assert_eq!(paths.model_root, storage.join("model"));
        assert!(!paths.used_fallback);
    }

    #[test]
    fn resolve_onedrive_root_falls_back_to_app_data() {
        let storage = Path::new("/Users/alice/OneDrive/Documents/Bango");
        let data = Path::new("/Users/alice/Library/Application Support");
        let paths = resolve_ai_paths_with_base(storage, Some(data));
        assert_eq!(paths.model_root, data.join("Bango").join("ai").join("models"));
        assert!(paths.used_fallback);
    }

    #[test]
    fn resolve_runtime_always_app_data() {
        let data = Path::new("/home/alice/.local/share");
        let expected = data.join("Bango").join("ai").join("runtimes");
        let paths =
            resolve_ai_paths_with_base(Path::new("/home/alice/Documents/Bango"), Some(data));
        assert_eq!(paths.runtime_root, expected);
        // Even under OneDrive the runtime stays out of the synced documents dir.
        let onedrive = Path::new("/Users/alice/OneDrive/Documents/Bango");
        let paths = resolve_ai_paths_with_base(onedrive, Some(data));
        assert_eq!(paths.runtime_root, expected);
    }

    #[test]
    fn resolve_missing_app_data_base_keeps_storage_root() {
        let storage = Path::new("/Users/alice/OneDrive/Documents/Bango");
        let paths = resolve_ai_paths_with_base(storage, None);
        // Degrades to the storage root without claiming a fallback fired.
        assert_eq!(paths.model_root, storage.join("model"));
        assert!(!paths.used_fallback);
        assert_eq!(paths.runtime_root, storage.join("ai").join("runtimes"));
    }
}
