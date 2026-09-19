//! Pinned component manifest for the Bango AI profile (Ornith-1.5-9B Q4_K_M
//! on a pinned llama.cpp runtime).
//!
//! The manifest is the single source of truth for WHAT gets installed, WHERE
//! it comes from, and HOW it is verified: commit-pinned model URL, pinned
//! GitHub release archives, exact sizes, SHA-256 hashes, and the per-target
//! archive member/alias sets. Runtime archives use the shared-library launcher
//! layout (a small `llama-server` launcher plus impl/common/ggml libraries and
//! the CPU/Metal backends); symlinked soname aliases are materialized as file
//! copies at install time so no symlink privileges are required. Members are
//! archive-relative paths flattened into the version directory at install.
//!
//! The embedded member/alias data was enumerated from the real b10964
//! archives during the T4 spike (sizes from the tar/zip headers).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::llm::local::profile::LOCAL_LLM_PROFILE_ID;
use crate::local_ai::manifest::{
    is_archive_safe_path, is_path_safe_file_name, is_valid_sha256_hex, required_disk_bytes,
    url_uses_allowed_scheme, PinnedFile,
};

pub use crate::local_ai::manifest::{ArchiveAlias as RuntimeAlias, ArchiveMember as RuntimeMember};

/// Known runtime target identifiers (shared with the embedding runtime gate).
pub const RUNTIME_TARGETS: [&str; 3] = ["linux-x64", "osx-arm64", "win-x64"];

/// One pinned runtime archive for a single target triple.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeArchive {
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
    /// Pinned member set (server + required libraries + backends).
    pub members: Vec<RuntimeMember>,
    /// Soname aliases materialized as copies after extraction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<RuntimeAlias>,
}

/// The pinned llama.cpp runtime component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlamaRuntimeManifest {
    /// Component name (`llama.cpp`).
    pub name: String,
    /// Pinned runtime version (release tag).
    pub version: String,
    /// Per-target pinned archives.
    pub archives: Vec<RuntimeArchive>,
}

/// The pinned component manifest for the Bango AI profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BangoAiManifest {
    /// Profile identity this manifest installs. [`local_manifest`] enforces
    /// equality with [`LOCAL_LLM_PROFILE_ID`].
    pub profile: String,
    /// Human-facing model name for the Settings panel.
    pub model: String,
    /// License identifier surfaced in the consent dialog.
    pub license: String,
    /// License terms URL surfaced in the consent dialog.
    pub license_url: String,
    /// The exact source revision (commit SHA) the model URL is pinned to.
    pub source_revision: String,
    /// Pinned model files.
    pub files: Vec<PinnedFile>,
    /// The pinned llama.cpp runtime.
    pub runtime: LlamaRuntimeManifest,
}

impl BangoAiManifest {
    /// Structural validation: profile/files/archives non-empty, path-safe
    /// names, allowed URL schemes, positive sizes, well-formed hashes, unique
    /// targets/members/aliases, and resolvable alias targets (chains
    /// allowed, cycles rejected).
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
        for file in &self.files {
            let name = file.name.trim();
            if !is_path_safe_file_name(name) {
                return Err(format!("file name is not path-safe: '{}'", file.name));
            }
            if !url_uses_allowed_scheme(&file.url) {
                return Err(format!("url must be HTTPS: '{}'", file.url));
            }
            if file.size == 0 {
                return Err(format!("size must be positive: '{}'", file.name));
            }
            if let Some(hash) = &file.sha256 {
                if !is_valid_sha256_hex(hash) {
                    return Err(format!(
                        "sha256 must be 64 lowercase hex chars for '{}'",
                        file.name
                    ));
                }
            }
        }
        if self.runtime.name.trim().is_empty() || self.runtime.version.trim().is_empty() {
            return Err("runtime name/version must be non-empty".to_string());
        }
        if self.runtime.archives.is_empty() {
            return Err("runtime archives must be non-empty".to_string());
        }
        let mut targets = HashSet::new();
        for archive in &self.runtime.archives {
            if !RUNTIME_TARGETS.contains(&archive.target.as_str()) {
                return Err(format!("unknown runtime target: '{}'", archive.target));
            }
            if !targets.insert(archive.target.clone()) {
                return Err(format!("duplicate runtime target: '{}'", archive.target));
            }
            if !url_uses_allowed_scheme(&archive.url) {
                return Err(format!("runtime url must be HTTPS: '{}'", archive.url));
            }
            if archive.size == 0 {
                return Err(format!("runtime size must be positive: '{}'", archive.name));
            }
            if !is_valid_sha256_hex(&archive.sha256) {
                return Err(format!(
                    "runtime sha256 must be 64 lowercase hex chars for '{}'",
                    archive.name
                ));
            }
            if archive.members.is_empty() {
                return Err(format!("runtime members must be non-empty: '{}'", archive.name));
            }
            let mut member_names = HashSet::new();
            for member in &archive.members {
                if !is_archive_safe_path(&member.path) {
                    return Err(format!("member path is not archive-safe: '{}'", member.path));
                }
                let base = std::path::Path::new(&member.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| format!("member path has no file name: '{}'", member.path))?;
                if !is_path_safe_file_name(base) {
                    return Err(format!("member file name is not path-safe: '{base}'"));
                }
                if !member_names.insert(base.to_string()) {
                    return Err(format!("duplicate member file name: '{base}'"));
                }
                if member.size == 0 {
                    return Err(format!("member size must be positive: '{}'", member.path));
                }
            }
            let mut alias_names = HashSet::new();
            for alias in &archive.aliases {
                if !is_path_safe_file_name(&alias.path) || !is_path_safe_file_name(&alias.target) {
                    return Err(format!("alias names must be path-safe: '{:?}'", alias));
                }
                if member_names.contains(&alias.path) {
                    return Err(format!("alias collides with a member: '{}'", alias.path));
                }
                if !alias_names.insert(alias.path.clone()) {
                    return Err(format!("duplicate alias: '{}'", alias.path));
                }
            }
            for alias in &archive.aliases {
                let mut current = alias.target.clone();
                let mut hops = 0usize;
                loop {
                    if member_names.contains(&current) {
                        break;
                    }
                    let Some(next) = archive.aliases.iter().find(|a| a.path == current) else {
                        return Err(format!(
                            "alias '{}' targets unknown file '{current}'",
                            alias.path
                        ));
                    };
                    current = next.target.clone();
                    hops += 1;
                    if hops > archive.aliases.len() {
                        return Err(format!("alias cycle detected at '{}'", alias.path));
                    }
                }
            }
        }
        Ok(())
    }

    /// Total download size in bytes: model files plus the current target's
    /// runtime archive.
    #[must_use]
    pub fn total_install_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum::<u64>()
            + self.archive_for_current_target().map_or(0, |a| a.size)
    }

    /// Disk space the install requires (shared repair-aware math).
    #[must_use]
    pub fn required_disk_bytes(&self, existing_install: bool) -> u64 {
        required_disk_bytes(self.total_install_bytes(), existing_install)
    }

    /// The pinned runtime archive for the current target, when present.
    #[must_use]
    pub fn archive_for_current_target(&self) -> Option<&RuntimeArchive> {
        let target = crate::local_ai::manifest::current_target()?;
        self.runtime.archives.iter().find(|a| a.target == target)
    }
}

/// Parse + validate a pinned manifest JSON string, including the profile
/// identity check against [`LOCAL_LLM_PROFILE_ID`].
pub fn parse_manifest(json: &str) -> Result<BangoAiManifest, AppError> {
    let manifest: BangoAiManifest = serde_json::from_str(json)?;
    manifest.validate().map_err(|reason| {
        AppError::Validation(format!("bango ai manifest is invalid: {reason}"))
    })?;
    if manifest.profile != LOCAL_LLM_PROFILE_ID {
        return Err(AppError::Validation(format!(
            "bango ai manifest profile '{}' does not match the active profile '{LOCAL_LLM_PROFILE_ID}'",
            manifest.profile
        )));
    }
    Ok(manifest)
}

/// Parse + validate the embedded pinned manifest.
pub fn local_manifest() -> Result<BangoAiManifest, AppError> {
    parse_manifest(BANGO_AI_MANIFEST_JSON)
}

/// The pinned v1 manifest: Ornith-1.5-9B Q4_K_M (MIT) plus the pinned
/// llama.cpp `b10964` runtime archives. Member/alias data enumerated from the
/// real archives during the T4 spike.
pub const BANGO_AI_MANIFEST_JSON: &str = r#"{
  "profile": "builtin/ornith-1.5-9b-q4km@r1",
  "model": "Ornith 1.5 9B",
  "license": "MIT",
  "licenseUrl": "https://huggingface.co/ornith-ai/Ornith-1.5-9B-GGUF",
  "sourceRevision": "abdd624b12ebf020b767fff532ff44fe552b28c3",
  "files": [
    {
      "name": "Ornith-1.5-9B-Q4_K_M.gguf",
      "url": "https://huggingface.co/ornith-ai/Ornith-1.5-9B-GGUF/resolve/abdd624b12ebf020b767fff532ff44fe552b28c3/Ornith-1.5-9B-Q4_K_M.gguf",
      "size": 5780090816,
      "sha256": "70c112196e0b7023803c9762752e46d29e612a92c83f995bc3ba1ceb07e8fab6"
    }
  ],
  "runtime": {
    "name": "llama.cpp",
    "version": "b10964",
    "archives": [
      {
        "target": "linux-x64",
        "name": "llama-b10964-bin-ubuntu-x64.tar.gz",
        "url": "https://github.com/ggml-org/llama.cpp/releases/download/b10964/llama-b10964-bin-ubuntu-x64.tar.gz",
        "size": 16825086,
        "sha256": "9abf88aea48a55d0f80edb1ee20220b186848cca0b4e919d71518cfd7ca67443",
        "members": [
          {
            "path": "llama-b10964/libggml-base.so.0.24.0",
            "size": 920352,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-alderlake.so",
            "size": 1195328,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-cannonlake.so",
            "size": 1336224,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-cascadelake.so",
            "size": 1332128,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-cooperlake.so",
            "size": 1332192,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-haswell.so",
            "size": 1199424,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-icelake.so",
            "size": 1332128,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-ivybridge.so",
            "size": 1152928,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-piledriver.so",
            "size": 1148832,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-sandybridge.so",
            "size": 1143616,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-sapphirerapids.so",
            "size": 1598560,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-skylakex.so",
            "size": 1336224,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-sse42.so",
            "size": 940328,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-x64.so",
            "size": 940496,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu-zen4.so",
            "size": 1332192,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml.so.0.24.0",
            "size": 55184,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama-common.so.0.4.1",
            "size": 6239736,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama-server-impl.so",
            "size": 7199800,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama.so.0.4.1",
            "size": 4593312,
            "executable": false
          },
          {
            "path": "llama-b10964/libmtmd.so.0.4.1",
            "size": 1878008,
            "executable": false
          },
          {
            "path": "llama-b10964/llama-server",
            "size": 17864,
            "executable": true
          }
        ],
        "aliases": [
          {
            "path": "libggml-base.so.0",
            "target": "libggml-base.so.0.24.0"
          },
          {
            "path": "libggml.so.0",
            "target": "libggml.so.0.24.0"
          },
          {
            "path": "libllama-common.so.0",
            "target": "libllama-common.so.0.4.1"
          },
          {
            "path": "libllama.so.0",
            "target": "libllama.so.0.4.1"
          },
          {
            "path": "libmtmd.so.0",
            "target": "libmtmd.so.0.4.1"
          }
        ]
      },
      {
        "target": "osx-arm64",
        "name": "llama-b10964-bin-macos-arm64.tar.gz",
        "url": "https://github.com/ggml-org/llama.cpp/releases/download/b10964/llama-b10964-bin-macos-arm64.tar.gz",
        "size": 11149739,
        "sha256": "033c845c1df9bf945ff37bb193238b40910b2244be3e1e637b2ceb5878f1a6f5",
        "members": [
          {
            "path": "llama-b10964/libggml-base.0.24.0.dylib",
            "size": 732936,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-blas.0.24.0.dylib",
            "size": 58776,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-cpu.0.24.0.dylib",
            "size": 968688,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml-metal.0.24.0.dylib",
            "size": 2148136,
            "executable": false
          },
          {
            "path": "llama-b10964/libggml.0.24.0.dylib",
            "size": 60128,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama-common.0.4.1.dylib",
            "size": 7618952,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama-server-impl.dylib",
            "size": 8980232,
            "executable": false
          },
          {
            "path": "llama-b10964/libllama.0.4.1.dylib",
            "size": 3113072,
            "executable": false
          },
          {
            "path": "llama-b10964/libmtmd.0.4.1.dylib",
            "size": 1378704,
            "executable": false
          },
          {
            "path": "llama-b10964/llama-server",
            "size": 49984,
            "executable": true
          }
        ],
        "aliases": [
          {
            "path": "libggml-base.0.dylib",
            "target": "libggml-base.0.24.0.dylib"
          },
          {
            "path": "libggml-blas.0.dylib",
            "target": "libggml-blas.0.24.0.dylib"
          },
          {
            "path": "libggml-cpu.0.dylib",
            "target": "libggml-cpu.0.24.0.dylib"
          },
          {
            "path": "libggml-metal.0.dylib",
            "target": "libggml-metal.0.24.0.dylib"
          },
          {
            "path": "libggml.0.dylib",
            "target": "libggml.0.24.0.dylib"
          },
          {
            "path": "libllama-common.0.dylib",
            "target": "libllama-common.0.4.1.dylib"
          },
          {
            "path": "libllama.0.dylib",
            "target": "libllama.0.4.1.dylib"
          },
          {
            "path": "libmtmd.0.dylib",
            "target": "libmtmd.0.4.1.dylib"
          }
        ]
      },
      {
        "target": "win-x64",
        "name": "llama-b10964-bin-win-cpu-x64.zip",
        "url": "https://github.com/ggml-org/llama.cpp/releases/download/b10964/llama-b10964-bin-win-cpu-x64.zip",
        "size": 18427629,
        "sha256": "917f39c076402c421224824607397af20f53625a60defc20e8dd22446bf4c5d7",
        "members": [
          {
            "path": "ggml-base.dll",
            "size": 796160,
            "executable": false
          },
          {
            "path": "ggml-cpu-alderlake.dll",
            "size": 1230336,
            "executable": false
          },
          {
            "path": "ggml-cpu-cannonlake.dll",
            "size": 1448448,
            "executable": false
          },
          {
            "path": "ggml-cpu-cascadelake.dll",
            "size": 1434112,
            "executable": false
          },
          {
            "path": "ggml-cpu-cooperlake.dll",
            "size": 1434624,
            "executable": false
          },
          {
            "path": "ggml-cpu-haswell.dll",
            "size": 1235968,
            "executable": false
          },
          {
            "path": "ggml-cpu-icelake.dll",
            "size": 1440768,
            "executable": false
          },
          {
            "path": "ggml-cpu-ivybridge.dll",
            "size": 1127424,
            "executable": false
          },
          {
            "path": "ggml-cpu-piledriver.dll",
            "size": 1131520,
            "executable": false
          },
          {
            "path": "ggml-cpu-sandybridge.dll",
            "size": 1107968,
            "executable": false
          },
          {
            "path": "ggml-cpu-sapphirerapids.dll",
            "size": 1711616,
            "executable": false
          },
          {
            "path": "ggml-cpu-skylakex.dll",
            "size": 1441792,
            "executable": false
          },
          {
            "path": "ggml-cpu-sse42.dll",
            "size": 932352,
            "executable": false
          },
          {
            "path": "ggml-cpu-x64.dll",
            "size": 924160,
            "executable": false
          },
          {
            "path": "ggml-cpu-zen4.dll",
            "size": 1441280,
            "executable": false
          },
          {
            "path": "ggml.dll",
            "size": 79872,
            "executable": false
          },
          {
            "path": "libomp.dll",
            "size": 768000,
            "executable": false
          },
          {
            "path": "llama-common.dll",
            "size": 7772672,
            "executable": false
          },
          {
            "path": "llama-server-impl.dll",
            "size": 8904192,
            "executable": false
          },
          {
            "path": "llama-server.exe",
            "size": 9216,
            "executable": true
          },
          {
            "path": "llama.dll",
            "size": 3154944,
            "executable": false
          },
          {
            "path": "mtmd.dll",
            "size": 1772032,
            "executable": false
          }
        ],
        "aliases": []
      }
    ]
  }
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bango_ai_manifest_pins_runtime_and_model_files() {
        let manifest = local_manifest().expect("embedded manifest parses + validates");
        assert_eq!(manifest.profile, LOCAL_LLM_PROFILE_ID);
        assert_eq!(manifest.license, "MIT");
        // Model: one pinned GGUF at the pinned commit.
        assert_eq!(manifest.files.len(), 1);
        let model = &manifest.files[0];
        assert_eq!(model.name, "Ornith-1.5-9B-Q4_K_M.gguf");
        assert_eq!(model.size, 5_780_090_816);
        assert!(model.sha256.is_some(), "model hash must be pinned");
        assert!(model.url.contains(&format!("/resolve/{}/", manifest.source_revision)));
        // Runtime: all three targets, release-pinned, with member sets and the
        // server marked executable.
        assert_eq!(manifest.runtime.archives.len(), 3);
        for archive in &manifest.runtime.archives {
            assert!(RUNTIME_TARGETS.contains(&archive.target.as_str()));
            assert!(archive.url.contains("/releases/download/b10964/"));
            assert!(!archive.members.is_empty());
            assert!(archive.members.iter().any(|m| m.executable));
        }
        // Aliases resolve to a member (directly or through a chain).
        for archive in &manifest.runtime.archives {
            let member_names: std::collections::HashSet<&str> = archive
                .members
                .iter()
                .filter_map(|m| std::path::Path::new(&m.path).file_name()?.to_str())
                .collect();
            for alias in &archive.aliases {
                let mut target = alias.target.as_str();
                let mut hops = 0;
                while !member_names.contains(target) {
                    target = archive
                        .aliases
                        .iter()
                        .find(|a| a.path == target)
                        .map(|a| a.target.as_str())
                        .expect("alias target resolves to a member");
                    hops += 1;
                    assert!(hops <= archive.aliases.len(), "no alias cycles");
                }
            }
        }
    }

    #[test]
    fn manifest_rejects_profile_drift_and_bad_pins() {
        let mut manifest = local_manifest().expect("embedded manifest");
        manifest.profile = "builtin/other@r9".to_string();
        let json = serde_json::to_string(&manifest).expect("serialize");
        let err = parse_manifest(&json).expect_err("profile drift must be rejected");
        assert!(err.to_string().contains("does not match the active profile"), "got: {err}");

        let mut bad = local_manifest().expect("embedded manifest");
        bad.files[0].sha256 = Some("short".to_string());
        assert!(bad.validate().is_err(), "bad model hash rejected");

        let mut bad_alias = local_manifest().expect("embedded manifest");
        bad_alias.runtime.archives[0].aliases.push(RuntimeAlias {
            path: "libghost.so".to_string(),
            target: "missing.so".to_string(),
        });
        assert!(bad_alias.validate().is_err(), "unresolvable alias rejected");

        let mut cycle = local_manifest().expect("embedded manifest");
        cycle.runtime.archives[0].aliases = vec![
            RuntimeAlias { path: "a.so".to_string(), target: "b.so".to_string() },
            RuntimeAlias { path: "b.so".to_string(), target: "a.so".to_string() },
        ];
        assert!(cycle.validate().is_err(), "alias cycle rejected");
    }
}
