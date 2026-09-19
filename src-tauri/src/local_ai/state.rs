//! Shared derived install-state assessment for local AI components.
//!
//! Moved/generalized from `embedding::local::state` in T2 slice 4: the
//! install lifecycle is DERIVED, not persisted. Callers supply the expected
//! pinned file set and the result of their own installation-manifest identity
//! check; this module owns the decision tree (missing vs repairable vs ready)
//! and the stranded-promote detection. Full SHA-256 verification stays an
//! explicit action owned by each component manager.

use std::path::Path;

/// Lifecycle state of a local AI component install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assessment {
    /// No installation found.
    NotInstalled,
    /// Some installation artifacts exist but the fast health check fails
    /// (missing/short file, foreign manifest, or a crashed promote) -
    /// re-running the install repairs it.
    RepairRequired,
    /// The installation manifest is valid and every expected file matches
    /// its pinned size.
    Ready,
}

impl Assessment {
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

/// Fast startup check (no hashing, no network): `Ready` only when the
/// caller's installation-manifest identity check passed AND every expected
/// file exists at its exact pinned size. Partial presence (manifest OR
/// files), a foreign/unparseable manifest, or a stranded
/// `.staging/replaced-*` park from a crashed promote all report
/// `RepairRequired`; nothing at all reports `NotInstalled`.
#[must_use]
pub fn assess_pinned_files(
    final_dir: &Path,
    staging_root: &Path,
    expected: &[(&str, u64)],
    install_manifest_ok: bool,
) -> Assessment {
    let files_ok = expected.iter().all(|(name, size)| {
        std::fs::metadata(final_dir.join(name)).is_ok_and(|m| m.is_file() && m.len() == *size)
    });
    match (install_manifest_ok, files_ok) {
        (true, true) => Assessment::Ready,
        (false, false) => {
            // A crashed promote parks the working copy under
            // `.staging/replaced-*`: artifacts exist even though the final
            // directory is gone, so this is repairable, not absent.
            if has_stranded_replaced_park(staging_root) {
                Assessment::RepairRequired
            } else {
                Assessment::NotInstalled
            }
        }
        _ => Assessment::RepairRequired,
    }
}

/// Whether a crashed promote left a parked working install under
/// `.staging/replaced-*`.
fn has_stranded_replaced_park(staging_root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(staging_root) else {
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
    fn assessment_ready_when_manifest_and_files_match() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_dir = dir.path().join("profile");
        std::fs::create_dir_all(&final_dir).expect("mkdir");
        std::fs::write(final_dir.join("a.bin"), vec![0u8; 4]).expect("write");
        let expected = [("a.bin", 4u64)];
        assert_eq!(
            assess_pinned_files(&final_dir, &dir.path().join(".staging"), &expected, true),
            Assessment::Ready
        );
    }

    #[test]
    fn assessment_reports_repair_for_stranded_replaced_park() {
        let dir = tempfile::tempdir().expect("tempdir");
        let final_dir = dir.path().join("profile");
        let staging_root = dir.path().join(crate::local_ai::download::STAGING_DIR_NAME);
        // A crashed promote: the final dir is gone, the working copy sits
        // parked under `.staging/replaced-<ts>`.
        let parked = staging_root.join("replaced-1234567890");
        std::fs::create_dir_all(&parked).expect("mkdir");
        std::fs::write(parked.join("working"), b"old-install").expect("park");
        let expected = [("a.bin", 4u64)];
        assert_eq!(
            assess_pinned_files(&final_dir, &staging_root, &expected, false),
            Assessment::RepairRequired
        );
    }

    #[test]
    fn assessment_not_installed_when_nothing_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let expected = [("a.bin", 4u64)];
        assert_eq!(
            assess_pinned_files(
                &dir.path().join("profile"),
                &dir.path().join(crate::local_ai::download::STAGING_DIR_NAME),
                &expected,
                false
            ),
            Assessment::NotInstalled
        );
    }
}
