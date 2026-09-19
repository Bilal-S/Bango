//! Installation state probe for the Bango Local embedding components.
//!
//! Layered verification: the fast startup check touches only the filesystem
//! (profile directory + installation manifest present) - full SHA-256
//! verification runs at install/update/repair/verify time in the component
//! manager, never on every launch. The install lifecycle is deliberately
//! DERIVED, not persisted: `installing` comes from the install command's
//! running flag, `unsupported` from the target gate, and everything else
//! from these filesystem probes - a crashed install self-heals into
//! `NotInstalled`/`RepairRequired` with no stale persisted state to clean
//! up.

use std::path::Path;

use crate::embedding::local::manifest::ComponentManifest;
use crate::embedding::local::profile::LOCAL_PROFILE_DIR;

/// Lifecycle state of the local embedding components.
///
/// The command layer derives `installing` (running flag) and `unsupported`
/// (target gate) on top of these probe states; in-progress phases stay
/// event-driven (`embedding:component` progress events). This is the final
/// design - persisted transitions were considered and rejected (a persisted
/// `downloading` would be stale-on-restart noise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalEmbeddingState {
    /// No installation found at the resolved model root.
    NotInstalled,
    /// Some installation artifacts exist but the fast health check fails
    /// (missing/short file, or missing manifest) - re-running the install
    /// repairs it.
    RepairRequired,
    /// A profile directory with an installation manifest exists.
    Ready,
}

impl LocalEmbeddingState {
    /// Serialized form for command payloads (`not_installed` |
    /// `repair_required` | `ready`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::RepairRequired => "repair_required",
            Self::Ready => "ready",
        }
    }

    /// Human phrase describing a not-ready state for user-facing error
    /// messages (never a Debug-formatted enum name like `RepairRequired`).
    #[must_use]
    pub fn not_ready_phrase(self) -> &'static str {
        match self {
            Self::RepairRequired => "some files are missing or incomplete",
            Self::NotInstalled => "the components are missing",
            Self::Ready => "the components are ready",
        }
    }
}

/// Installation manifest file name inside the profile directory
/// (`<model_root>/<profile>/manifest.json`).
pub const INSTALL_MANIFEST_NAME: &str = "manifest.json";

/// Cheap gate for the service router: can the local backend possibly run?
/// Checks only the expected profile directory's manifest (no file sizes, no
/// manifest parse - the engine tier owns the authoritative load).
#[must_use]
pub fn probe_installation_state(model_root: &Path) -> LocalEmbeddingState {
    if model_root.join(LOCAL_PROFILE_DIR).join(INSTALL_MANIFEST_NAME).is_file() {
        LocalEmbeddingState::Ready
    } else {
        LocalEmbeddingState::NotInstalled
    }
}

/// The plan-§5 fast startup check (no hashing, no network): Ready only when
/// the installation manifest exists, RECORDS THE ACTIVE PROFILE, and every
/// expected file is present with the exact pinned size. Partial presence
/// (manifest OR files), a foreign or unparseable installed manifest, or a
/// stranded `.staging/replaced-*` park from a crashed promote all report
/// `RepairRequired`; nothing at all reports `NotInstalled`. Full SHA-256
/// verification stays an explicit action (`download::verify_installed`).
#[must_use]
pub fn assess_installation(model_root: &Path, manifest: &ComponentManifest) -> LocalEmbeddingState {
    let final_dir = model_root.join(LOCAL_PROFILE_DIR);
    let manifest_ok = installed_manifest_records_profile(&final_dir, &manifest.profile);
    let files_ok = manifest.files.iter().all(|f| {
        std::fs::metadata(final_dir.join(&f.name)).is_ok_and(|m| m.is_file() && m.len() == f.size)
    });
    match (manifest_ok, files_ok) {
        (true, true) => LocalEmbeddingState::Ready,
        (false, false) => {
            // A crashed promote parks the working copy under
            // `.staging/replaced-*`: artifacts exist even though the profile
            // directory is gone, so this is repairable, not absent.
            if has_stranded_replaced_park(model_root) {
                LocalEmbeddingState::RepairRequired
            } else {
                LocalEmbeddingState::NotInstalled
            }
        }
        _ => LocalEmbeddingState::RepairRequired,
    }
}

/// Whether the installed `manifest.json` parses and records `active_profile`.
/// Missing/unparseable/foreign-profile manifests return `false`.
fn installed_manifest_records_profile(final_dir: &Path, active_profile: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(final_dir.join(INSTALL_MANIFEST_NAME)) else {
        return false;
    };
    match serde_json::from_str::<ComponentManifest>(&text) {
        Ok(installed) => installed.profile == active_profile,
        Err(_) => false,
    }
}

/// Whether a crashed promote left a parked working install under
/// `.staging/replaced-*`.
fn has_stranded_replaced_park(model_root: &Path) -> bool {
    let Ok(entries) =
        std::fs::read_dir(model_root.join(crate::embedding::local::download::STAGING_DIR_NAME))
    else {
        return false;
    };
    entries
        .filter_map(std::result::Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with("replaced-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_not_installed_when_model_root_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        assert_eq!(probe_installation_state(&root), LocalEmbeddingState::NotInstalled);
    }

    #[test]
    fn state_not_installed_without_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        std::fs::create_dir_all(root.join("embeddinggemma-300m-q4")).expect("mkdir");
        assert_eq!(probe_installation_state(&root), LocalEmbeddingState::NotInstalled);
    }

    #[test]
    fn state_ready_when_profile_manifest_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let profile = root.join("embeddinggemma-300m-q4");
        std::fs::create_dir_all(&profile).expect("mkdir");
        std::fs::write(profile.join(INSTALL_MANIFEST_NAME), "{}").expect("write manifest");
        assert_eq!(probe_installation_state(&root), LocalEmbeddingState::Ready);
    }

    #[test]
    fn state_ignores_unrelated_and_staging_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        // A foreign profile and a leftover .staging directory must not count
        // as an installation of the expected profile.
        for name in ["bge-small-en", ".staging"] {
            let other = root.join(name);
            std::fs::create_dir_all(&other).expect("mkdir");
            std::fs::write(other.join(INSTALL_MANIFEST_NAME), "{}").expect("write manifest");
        }
        assert_eq!(probe_installation_state(&root), LocalEmbeddingState::NotInstalled);
    }

    /// Two tiny in-memory manifests for the assess tests.
    fn tiny_manifest() -> crate::embedding::local::manifest::ComponentManifest {
        crate::embedding::local::manifest::ComponentManifest {
            profile: "builtin/embeddinggemma-300m-q4@r1".to_string(),
            model: "Test".to_string(),
            license: "Test".to_string(),
            license_url: "https://example.invalid".to_string(),
            source_revision: "abc1234".to_string(),
            files: vec![
                crate::embedding::local::manifest::ManifestFile {
                    name: "a.bin".to_string(),
                    url: "https://example.invalid/a".to_string(),
                    size: 4,
                    sha256: Some("0".repeat(64)),
                },
                crate::embedding::local::manifest::ManifestFile {
                    name: "b.bin".to_string(),
                    url: "https://example.invalid/b".to_string(),
                    size: 6,
                    sha256: None,
                },
            ],
            runtime: None,
        }
    }

    fn write_installed(
        root: &std::path::Path,
        manifest: &crate::embedding::local::manifest::ComponentManifest,
    ) {
        let profile = root.join("embeddinggemma-300m-q4");
        std::fs::create_dir_all(&profile).expect("mkdir");
        std::fs::write(
            profile.join(INSTALL_MANIFEST_NAME),
            serde_json::to_string(manifest).expect("manifest json"),
        )
        .expect("write manifest");
        for file in &manifest.files {
            std::fs::write(profile.join(&file.name), vec![0u8; file.size as usize])
                .expect("write file");
        }
    }

    #[test]
    fn assess_ready_for_complete_install() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        write_installed(&root, &manifest);
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::Ready);
    }

    #[test]
    fn assess_reports_repair_required_for_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        write_installed(&root, &manifest);
        std::fs::remove_file(root.join("embeddinggemma-300m-q4").join(&manifest.files[1].name))
            .expect("remove file");
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::RepairRequired);
    }

    #[test]
    fn assess_reports_repair_required_for_short_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        write_installed(&root, &manifest);
        std::fs::write(root.join("embeddinggemma-300m-q4").join(&manifest.files[0].name), b"xx")
            .expect("truncate file");
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::RepairRequired);
    }

    #[test]
    fn assess_not_installed_when_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::NotInstalled);
    }

    #[test]
    fn assess_reports_repair_required_for_profile_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        write_installed(&root, &manifest);
        // Rewrite the installed manifest with a foreign profile revision
        // (an r1 install inspected against a future r2 pin): the fast check
        // must flag it even though every file matches its pinned size.
        let mut stale = manifest.clone();
        stale.profile = "builtin/embeddinggemma-300m-q4@r2".to_string();
        std::fs::write(
            root.join("embeddinggemma-300m-q4").join(INSTALL_MANIFEST_NAME),
            serde_json::to_string(&stale).expect("manifest json"),
        )
        .expect("rewrite manifest");
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::RepairRequired);
    }

    #[test]
    fn assess_reports_repair_required_for_stranded_replaced_install() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("model");
        let manifest = tiny_manifest();
        // Simulate a crash between park and promote: the profile dir is gone,
        // the working copy sits parked under .staging/replaced-<ts>.
        let parked = root.join(".staging").join("replaced-1234567890");
        std::fs::create_dir_all(&parked).expect("mkdir");
        std::fs::write(parked.join("working"), b"old-install").expect("park");
        assert_eq!(assess_installation(&root, &manifest), LocalEmbeddingState::RepairRequired);
    }

    #[test]
    fn not_ready_phrase_is_human_text_for_every_state() {
        // User-facing messages must carry a human phrase, never the
        // Debug-formatted enum name (`RepairRequired`).
        for state in [LocalEmbeddingState::NotInstalled, LocalEmbeddingState::RepairRequired] {
            let phrase = state.not_ready_phrase();
            assert!(!phrase.contains("NotInstalled"), "got: {phrase}");
            assert!(!phrase.contains("RepairRequired"), "got: {phrase}");
            assert!(!phrase.is_empty());
        }
        assert_eq!(
            LocalEmbeddingState::RepairRequired.not_ready_phrase(),
            "some files are missing or incomplete"
        );
        assert_eq!(
            LocalEmbeddingState::NotInstalled.not_ready_phrase(),
            "the components are missing"
        );
    }
}
