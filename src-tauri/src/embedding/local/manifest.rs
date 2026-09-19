//! Pinned component manifest for the Bango Local embedding profile.
//!
//! The manifest is the single source of truth for WHAT gets installed, WHERE
//! it comes from, and HOW it is verified (per the plan's component-manager
//! tier): pinned URLs, exact sizes, and SHA-256 hashes for every file - the
//! three LFS artifacts use the published `lfs.oid` values; the three small
//! non-LFS files were hashed at the pinned revision at manifest-build time.
//! Bango never resolves "latest": updates ship as new pinned
//! manifests.
//!
//! Hash provenance: the `lfs.oid` values from the Hugging Face tree API for
//! the pinned repository revision (verified against
//! `onnx-community/embeddinggemma-300m-ONNX`).

use serde::{Deserialize, Serialize};

use crate::embedding::local::profile::LOCAL_PROFILE_ID;

/// One pinned artifact file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    /// Destination file name inside the profile directory (flat, path-safe).
    pub name: String,
    /// Pinned absolute HTTPS download URL.
    pub url: String,
    /// Exact expected size in bytes.
    pub size: u64,
    /// Pinned SHA-256 (64 lowercase hex chars). `None` = verify by size
    /// only (unused by the pinned manifest, which pins all six files; the
    /// variant remains for future artifacts without published hashes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// One pinned runtime archive for a single target triple.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFile {
    /// Target identifier: "linux-x64" | "osx-arm64" | "win-x64".
    pub target: String,
    /// Archive file name (staging name for the download).
    pub name: String,
    /// Pinned absolute HTTPS download URL.
    pub url: String,
    /// Exact expected archive size in bytes.
    pub size: u64,
    /// Pinned archive SHA-256 (official GitHub release digest).
    pub sha256: String,
    /// Path of the runtime library INSIDE the archive (only this member is
    /// extracted), e.g. `onnxruntime-linux-x64-1.30.0/lib/libonnxruntime.so`.
    pub lib_path: String,
    /// Uncompressed size of the extracted library in bytes (carried by the
    /// zip entry / tar header). The integrity check for the install skip,
    /// verification, and the engine's dylib resolution - the archive SHA-256
    /// pins content transitively at download time.
    pub lib_size: u64,
}

/// The pinned ONNX Runtime component (dynamic library, downloaded + extracted
/// on demand - never bundled).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManifest {
    /// Component name (`onnxruntime`).
    pub name: String,
    /// Pinned runtime version.
    pub version: String,
    /// Per-target pinned archives.
    pub files: Vec<RuntimeFile>,
}

/// The pinned component manifest for one embedding profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentManifest {
    /// Profile identity this manifest installs. [`local_manifest`] enforces
    /// equality with [`LOCAL_PROFILE_ID`] so the `model_name` identity and
    /// the on-disk manifest can never drift apart silently.
    pub profile: String,
    /// Human-facing model name for the Settings component details panel.
    pub model: String,
    /// License identifier surfaced in the consent dialog.
    pub license: String,
    /// License terms URL surfaced in the consent dialog.
    pub license_url: String,
    /// The exact source revision (commit SHA) every URL is pinned to.
    /// Recorded for provenance and the Component Details panel.
    pub source_revision: String,
    /// Pinned artifact files.
    pub files: Vec<ManifestFile>,
    /// The pinned ONNX Runtime component (present in shipped manifests;
    /// optional so test/reduced manifests can omit it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeManifest>,
}

/// The pinned v1 manifest: EmbeddingGemma 300M Q4 from the ungated
/// `onnx-community` ONNX mirror (~218 MB total download).
///
/// Every URL is commit-pinned (`/resolve/<sha>/`) - `main` is mutable and a
/// force push would break hash verification, contradicting the "never
/// resolves latest" contract. All six files carry SHA-256 pins: the three
/// LFS artifacts use the published `lfs.oid` values; the three small
/// non-LFS files were hashed at the pinned revision at manifest-build time.
pub const LOCAL_COMPONENT_MANIFEST_JSON: &str = r#"{
  "profile": "builtin/embeddinggemma-300m-q4@r1",
  "model": "EmbeddingGemma 300M Q4",
  "license": "Gemma Terms of Use",
  "licenseUrl": "https://ai.google.dev/gemma/terms",
  "sourceRevision": "5090578d9565bb06545b4552f76e6bc2c93e4a66",
  "files": [
    {
      "name": "model_q4.onnx",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/onnx/model_q4.onnx",
      "size": 519322,
      "sha256": "ad1dfee81a70f7944b9b9d1cc6e48075b832881cf33fab2f2b248be78f3f0043"
    },
    {
      "name": "model_q4.onnx_data",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/onnx/model_q4.onnx_data",
      "size": 196725760,
      "sha256": "599962c3143b040de2dd05e5975be3e9091dd067cacc6a8f7186e3203bab9e02"
    },
    {
      "name": "tokenizer.json",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/tokenizer.json",
      "size": 20323312,
      "sha256": "4dda02faaf32bc91031dc8c88457ac272b00c1016cc679757d1c441b248b9c47"
    },
    {
      "name": "tokenizer_config.json",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/tokenizer_config.json",
      "size": 1156830,
      "sha256": "3ca953eea6c3c9fcda9cf3df22949ff18b216f7c74bd6459230f3f1013953f3a"
    },
    {
      "name": "config.json",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/config.json",
      "size": 1765,
      "sha256": "6e1f06404b7163e0325ed2ea3e6781cde50f4a50b31780a95ad0d30e8404d77b"
    },
    {
      "name": "special_tokens_map.json",
      "url": "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/5090578d9565bb06545b4552f76e6bc2c93e4a66/special_tokens_map.json",
      "size": 662,
      "sha256": "2f7b0adf4fb469770bb1490e3e35df87b1dc578246c5e7e6fc76ecf33213a397"
    }
  ],
  "runtime": {
    "name": "onnxruntime",
    "version": "1.30.0",
    "files": [
      {
        "target": "linux-x64",
        "name": "onnxruntime-linux-x64-1.30.0.tgz",
        "url": "https://github.com/microsoft/onnxruntime/releases/download/v1.30.0/onnxruntime-linux-x64-1.30.0.tgz",
        "size": 11306877,
        "sha256": "a5ed5a3cac51fbb2e90da632ae43d19212faaa20e76484e62bcb7c23ddb3b3fd",
        "libPath": "onnxruntime-linux-x64-1.30.0/lib/libonnxruntime.so.1.30.0",
        "libSize": 28985152
      },
      {
        "target": "osx-arm64",
        "name": "onnxruntime-osx-arm64-1.30.0.tgz",
        "url": "https://github.com/microsoft/onnxruntime/releases/download/v1.30.0/onnxruntime-osx-arm64-1.30.0.tgz",
        "size": 42373116,
        "sha256": "6ebb5062a934537c352937821f9fe9718e7de1a2db1122a93dd363ffd53a7012",
        "libPath": "onnxruntime-osx-arm64-1.30.0/lib/libonnxruntime.1.30.0.dylib",
        "libSize": 43879424
      },
      {
        "target": "win-x64",
        "name": "onnxruntime-win-x64-1.30.0.zip",
        "url": "https://github.com/microsoft/onnxruntime/releases/download/v1.30.0/onnxruntime-win-x64-1.30.0.zip",
        "size": 82645522,
        "sha256": "c6ba983baf5681af108599675d2a89c2d145512d02de28aed0bff177cd0ba949",
        "libPath": "onnxruntime-win-x64-1.30.0/lib/onnxruntime.dll",
        "libSize": 16462648
      }
    ]
  }
}"#;

/// URL scheme policy: HTTPS everywhere except plain-HTTP loopback hosts,
/// which exist for test mocks and local mirrors (the pinned manifest is
/// embedded in the binary, so the rule guards against accidental plaintext
/// external fetches, not adversarial edits). The loopback prefixes must end
/// at a host boundary (`:` port, `/` path, or end of string) so lookalike
/// domains (`http://localhost.evil.com`) are rejected.
fn url_uses_allowed_scheme(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    ["http://127.0.0.1", "http://localhost"].iter().any(|prefix| match url.strip_prefix(prefix) {
        Some(rest) => rest.is_empty() || rest.starts_with(':') || rest.starts_with('/'),
        None => false,
    })
}

/// Parse + validate a pinned manifest JSON string: structural validation,
/// well-formed source revision, and profile identity matching
/// [`LOCAL_PROFILE_ID`]. Fails only if the input is malformed or drifted
/// (a build-time invariant for the embedded constant; covered by tests).
pub fn parse_manifest(json: &str) -> Result<ComponentManifest, crate::error::AppError> {
    let manifest: ComponentManifest = serde_json::from_str(json)?;
    manifest.validate().map_err(|reason| {
        crate::error::AppError::Validation(format!("component manifest is invalid: {reason}"))
    })?;
    if manifest.profile != LOCAL_PROFILE_ID {
        return Err(crate::error::AppError::Validation(format!(
            "component manifest profile '{}' does not match the active profile '{LOCAL_PROFILE_ID}'",
            manifest.profile
        )));
    }
    Ok(manifest)
}

/// Parse + validate the embedded pinned manifest.
pub fn local_manifest() -> Result<ComponentManifest, crate::error::AppError> {
    parse_manifest(LOCAL_COMPONENT_MANIFEST_JSON)
}

impl ComponentManifest {
    /// Structural validation: non-empty profile/files, unique path-safe
    /// names, HTTPS URLs, positive sizes, well-formed pinned hashes.
    pub fn validate(&self) -> Result<(), String> {
        if self.profile.trim().is_empty() {
            return Err("profile must be non-empty".to_string());
        }
        if self.files.is_empty() {
            return Err("files must be non-empty".to_string());
        }
        let revision = self.source_revision.trim();
        let revision_ok =
            (7..=40).contains(&revision.len()) && revision.chars().all(|c| c.is_ascii_hexdigit());
        if !revision_ok {
            return Err(format!("sourceRevision must be a 7-40 char commit sha: '{revision}'"));
        }
        let mut names = std::collections::HashSet::new();
        for file in &self.files {
            let name = file.name.trim();
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.starts_with('.')
                || name.contains('/')
                || name.contains('\\')
                || name.contains(':')
            {
                return Err(format!("file name is not path-safe: '{}'", file.name));
            }
            if !names.insert(file.name.clone()) {
                return Err(format!("duplicate file name: '{}'", file.name));
            }
            if !url_uses_allowed_scheme(&file.url) {
                return Err(format!("url must be HTTPS: '{}'", file.url));
            }
            if file.size == 0 {
                return Err(format!("size must be positive: '{}'", file.name));
            }
            if let Some(hash) = &file.sha256 {
                let well_formed = hash.len() == 64
                    && hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                if !well_formed {
                    return Err(format!(
                        "sha256 must be 64 lowercase hex chars for '{}'",
                        file.name
                    ));
                }
            }
        }
        if let Some(runtime) = &self.runtime {
            if runtime.name.trim().is_empty() || runtime.version.trim().is_empty() {
                return Err("runtime name/version must be non-empty".to_string());
            }
            let mut targets = std::collections::HashSet::new();
            for file in &runtime.files {
                if !matches!(file.target.as_str(), "linux-x64" | "osx-arm64" | "win-x64") {
                    return Err(format!("unknown runtime target: '{}'", file.target));
                }
                if !targets.insert(file.target.clone()) {
                    return Err(format!("duplicate runtime target: '{}'", file.target));
                }
                let well_formed = file.sha256.len() == 64
                    && file.sha256.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                if !well_formed {
                    return Err(format!(
                        "runtime sha256 must be 64 lowercase hex chars for '{}'",
                        file.name
                    ));
                }
                if file.size == 0 {
                    return Err(format!("runtime size must be positive: '{}'", file.name));
                }
                if file.lib_size == 0 {
                    return Err(format!("runtime libSize must be positive: '{}'", file.name));
                }
                let lib = file.lib_path.trim();
                if lib.is_empty()
                    || lib.starts_with('/')
                    || lib.contains("..")
                    || lib.contains('\\')
                {
                    return Err(format!("runtime libPath is not archive-safe: '{lib}'"));
                }
            }
        }
        Ok(())
    }

    /// Total download size in bytes (all files).
    #[must_use]
    pub fn total_download_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Disk space the install requires. `existing_install` doubles the
    /// payload: a repair or update stages a complete new copy while the old
    /// install still exists, so the transient peak is ~2x. The margin covers
    /// staging bookkeeping (`max(64 MiB, 10%)`). Counts the model files plus
    /// the current target's runtime archive.
    #[must_use]
    pub fn required_disk_bytes(&self, existing_install: bool) -> u64 {
        let total = self.total_install_bytes();
        let copies = u64::from(existing_install);
        (total * (1 + copies)) + std::cmp::max(64 * 1024 * 1024, total / 10)
    }

    /// Look up a file by destination name.
    #[must_use]
    pub fn file(&self, name: &str) -> Option<&ManifestFile> {
        self.files.iter().find(|f| f.name == name)
    }

    /// The current machine's target identifier, when supported.
    #[must_use]
    pub fn current_target() -> Option<&'static str> {
        target_id(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// The pinned runtime archive for the current target, when the manifest
    /// carries a runtime component for it.
    #[must_use]
    pub fn runtime_file_for_current_target(&self) -> Option<&RuntimeFile> {
        let target = Self::current_target()?;
        self.runtime.as_ref()?.files.iter().find(|f| f.target == target)
    }

    /// Model files PLUS the current target's runtime archive (when pinned
    /// and not yet extracted - the caller decides). Used for disk gating and
    /// progress totals: the runtime library is extracted from the archive,
    /// so the archive size is the transfer cost.
    #[must_use]
    pub fn total_install_bytes(&self) -> u64 {
        self.total_download_bytes() + self.runtime_file_for_current_target().map_or(0, |f| f.size)
    }
}

/// Pure mapping of (os, arch) to the manifest's target identifiers.
#[must_use]
pub fn target_id(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("win-x64"),
        ("macos", "aarch64") => Some("osx-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_manifest_parses_and_validates() {
        let manifest = local_manifest().expect("embedded manifest parses + validates");
        assert_eq!(manifest.profile, LOCAL_PROFILE_ID);
        assert_eq!(manifest.model, "EmbeddingGemma 300M Q4");
        assert!(manifest.files.len() >= 3, "model graph + tokenizer files");
        // Every file carries a pinned hash and a commit-pinned URL.
        assert!(manifest.files.iter().all(|f| f.sha256.is_some()));
        assert!(manifest
            .files
            .iter()
            .all(|f| f.url.contains(&format!("/resolve/{}/", manifest.source_revision))));
        assert!(!manifest.license_url.is_empty(), "license terms must be surfaced");
        // The runtime component is pinned for all three supported targets.
        let runtime = manifest.runtime.as_ref().expect("runtime component pinned");
        assert_eq!(runtime.version, "1.30.0");
        assert!(runtime.files.iter().any(|f| f.target == "linux-x64"));
        assert!(runtime.files.iter().any(|f| f.target == "osx-arm64"));
        assert!(runtime.files.iter().any(|f| f.target == "win-x64"));
    }

    #[test]
    fn manifest_target_id_matrix() {
        assert_eq!(target_id("windows", "x86_64"), Some("win-x64"));
        assert_eq!(target_id("macos", "aarch64"), Some("osx-arm64"));
        assert_eq!(target_id("linux", "x86_64"), Some("linux-x64"));
        assert_eq!(target_id("macos", "x86_64"), None);
        assert_eq!(target_id("linux", "aarch64"), None);
    }

    #[test]
    fn manifest_runtime_validation_rejects_bad_entries() {
        let mut manifest = local_manifest().expect("embedded manifest");
        let mut runtime = manifest.runtime.clone().expect("runtime");
        runtime.files[0].sha256 = "short".to_string();
        manifest.runtime = Some(runtime.clone());
        assert!(manifest.validate().is_err(), "bad runtime hash rejected");
        runtime.files[0].sha256 = "0".repeat(64);
        runtime.files.push(runtime.files[0].clone());
        manifest.runtime = Some(runtime.clone());
        assert!(manifest.validate().is_err(), "duplicate target rejected");
        runtime.files.pop();
        runtime.files[0].lib_path = "../escape.so".to_string();
        manifest.runtime = Some(runtime);
        assert!(manifest.validate().is_err(), "unsafe libPath rejected");
    }

    #[test]
    fn parse_manifest_rejects_profile_drift() {
        let mut drifted = local_manifest().expect("embedded manifest");
        drifted.profile = "builtin/some-other-profile@r9".to_string();
        let json = serde_json::to_string(&drifted).expect("serialize");
        let err = parse_manifest(&json).expect_err("profile drift must be rejected");
        assert!(err.to_string().contains("does not match the active profile"), "got: {err}");
    }

    #[test]
    fn manifest_rejects_malformed_sha256() {
        let mut manifest = local_manifest().expect("embedded manifest");
        manifest.files[0].sha256 = Some("not-a-hash".to_string());
        assert!(manifest.validate().is_err(), "short non-hex hash must be rejected");
        manifest.files[0].sha256 = Some("ABCD".repeat(16));
        assert!(manifest.validate().is_err(), "uppercase hex must be rejected");
        manifest.files[0].sha256 = Some("0".repeat(63));
        assert!(manifest.validate().is_err(), "63 chars must be rejected");
    }

    #[test]
    fn manifest_rejects_malformed_source_revision() {
        let mut manifest = local_manifest().expect("embedded manifest");
        manifest.source_revision = "not-a-sha!".to_string();
        assert!(manifest.validate().is_err(), "non-hex revision must be rejected");
        manifest.source_revision = String::new();
        assert!(manifest.validate().is_err(), "empty revision must be rejected");
    }

    #[test]
    fn manifest_rejects_non_https_external_url() {
        let mut manifest = local_manifest().expect("embedded manifest");
        manifest.files[0].url = "http://huggingface.co/some/file".to_string();
        assert!(manifest.validate().is_err(), "plain-HTTP external URLs must be rejected");
        // Loopback plain-HTTP stays allowed (test mocks, local mirrors), but
        // only at a host boundary - lookalike domains are rejected.
        for allowed in ["http://127.0.0.1:1234/file-a", "http://localhost/file", "http://localhost"]
        {
            manifest.files[0].url = allowed.to_string();
            assert!(manifest.validate().is_ok(), "allowed loopback URL: {allowed}");
        }
        manifest.files[0].url = "http://localhost.evil.com/file".to_string();
        assert!(manifest.validate().is_err(), "lookalike domain must be rejected");
        manifest.files[0].url = "http://127.0.0.1.evil.com/file".to_string();
        assert!(manifest.validate().is_err(), "lookalike IP domain must be rejected");
    }

    #[test]
    fn manifest_required_bytes_cover_download_plus_margin() {
        let manifest = local_manifest().expect("embedded manifest");
        // The install total is model files PLUS the current target's runtime
        // archive (the disk gate and the progress totals both use it).
        let total = manifest.total_install_bytes();
        let expected: u64 = manifest.files.iter().map(|f| f.size).sum::<u64>()
            + manifest.runtime_file_for_current_target().map_or(0, |f| f.size);
        assert_eq!(total, expected);
        let required = manifest.required_disk_bytes(false);
        assert!(required > total, "margin must be added");
        assert!(required <= total + 64 * 1024 * 1024 + total, "margin must be bounded");
    }

    #[test]
    fn manifest_required_bytes_double_for_repair() {
        let manifest = local_manifest().expect("embedded manifest");
        let fresh = manifest.required_disk_bytes(false);
        let repair = manifest.required_disk_bytes(true);
        assert!(repair > fresh, "repair must budget for the staged copy");
        assert_eq!(repair - fresh, manifest.total_install_bytes());
    }
}
