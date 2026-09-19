//! Shared pinned-manifest primitives for local AI components.
//!
//! Moved out of `embedding::local::manifest` in T2: the pinned file shape, URL
//! scheme policy, target mapping, validation helpers, and the disk-space math
//! shared by Bango Local embeddings and Bango AI. Component-specific manifest
//! structs (embedding profiles, the llama.cpp runtime set) stay in their own
//! modules.

use serde::{Deserialize, Serialize};

/// One pinned artifact file: flat destination name, commit-pinned URL, exact
/// size, and an optional SHA-256 (64 lowercase hex chars).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedFile {
    /// Destination file name inside the profile directory (flat, path-safe).
    pub name: String,
    /// Pinned absolute HTTPS download URL.
    pub url: String,
    /// Exact expected size in bytes.
    pub size: u64,
    /// Pinned SHA-256 (64 lowercase hex chars). `None` = verify by size only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// One member inside a pinned runtime archive. Bango AI extracts a pinned
/// member SET (server plus adjacent shared libraries); the embedding runtime
/// extracts a single library and keeps its own `RuntimeFile` shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveMember {
    /// Path inside the archive (never absolute, `..`, or backslash).
    pub path: String,
    /// Uncompressed size in bytes (carried by the zip entry / tar header).
    pub size: u64,
    /// Whether the extracted member needs the executable bit on unix.
    pub executable: bool,
}

/// One soname alias materialized as a copy of another extracted member.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveAlias {
    /// Alias file name inside the install directory.
    pub path: String,
    /// Target file name (a member or another alias) in the same directory.
    pub target: String,
}

/// URL scheme policy: HTTPS everywhere except plain-HTTP loopback hosts,
/// which exist for test mocks and local mirrors (the pinned manifest is
/// embedded in the binary, so the rule guards against accidental plaintext
/// external fetches, not adversarial edits). The loopback prefixes must end
/// at a host boundary (`:` port, `/` path, or end of string) so lookalike
/// domains (`http://localhost.evil.com`) are rejected.
#[must_use]
pub fn url_uses_allowed_scheme(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    ["http://127.0.0.1", "http://localhost"].iter().any(|prefix| match url.strip_prefix(prefix) {
        Some(rest) => rest.is_empty() || rest.starts_with(':') || rest.starts_with('/'),
        None => false,
    })
}

/// Whether `hash` is exactly 64 lowercase hex characters.
#[must_use]
pub fn is_valid_sha256_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Whether a file destination name is flat and path-safe.
#[must_use]
pub fn is_path_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

/// Whether an archive member path is traversal-safe on every platform.
#[must_use]
pub fn is_archive_safe_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains("..") && !path.contains('\\')
}

/// Disk space an install requires for a payload of `total` bytes.
/// `existing_install` doubles the payload: a repair or update stages a complete
/// new copy while the old install still exists, so the transient peak is ~2x.
/// The margin covers staging bookkeeping (`max(64 MiB, 10%)`).
#[must_use]
pub fn required_disk_bytes(total: u64, existing_install: bool) -> u64 {
    let copies = u64::from(existing_install);
    (total * (1 + copies)) + std::cmp::max(64 * 1024 * 1024, total / 10)
}

/// Pure mapping of (os, arch) to the manifest's target identifiers.
/// macOS x86_64 is deliberately absent: no runtime builds meet the pinned
/// toolchain floors, so Intel Macs fall back to the configured provider.
#[must_use]
pub fn target_id(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("win-x64"),
        ("macos", "aarch64") => Some("osx-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

/// The current machine's target identifier, when supported.
#[must_use]
pub fn current_target() -> Option<&'static str> {
    target_id(std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_manifest_validation_rejects_bad_pins() {
        // Hashes: short, uppercase, and 63-char values are rejected.
        assert!(!is_valid_sha256_hex("short"));
        assert!(!is_valid_sha256_hex(&"A".repeat(64)));
        assert!(!is_valid_sha256_hex(&"0".repeat(63)));
        assert!(is_valid_sha256_hex(&"0".repeat(64)));

        // Destination names must be flat and path-safe.
        for bad in ["", ".", "..", ".hidden", "dir/file", r"dir\file", "c:file"] {
            assert!(!is_path_safe_file_name(bad), "name must be rejected: {bad}");
        }
        assert!(is_path_safe_file_name("model_q4.onnx"));

        // Archive members may nest but never escape.
        for bad in ["", "/abs.so", "../escape.so", r"dir\file"] {
            assert!(!is_archive_safe_path(bad), "member must be rejected: {bad}");
        }
        assert!(is_archive_safe_path("llama-b10964-bin-macos-arm64/llama-server"));

        // Scheme policy: HTTPS anywhere, HTTP only at a loopback host boundary.
        assert!(url_uses_allowed_scheme("https://huggingface.co/x"));
        assert!(url_uses_allowed_scheme("http://127.0.0.1:8080/x"));
        assert!(url_uses_allowed_scheme("http://localhost"));
        assert!(!url_uses_allowed_scheme("http://huggingface.co/x"));
        assert!(!url_uses_allowed_scheme("http://localhost.evil.com/x"));
        assert!(!url_uses_allowed_scheme("http://127.0.0.1.evil.com/x"));

        // Target mapping and the shared disk-space math.
        assert_eq!(target_id("windows", "x86_64"), Some("win-x64"));
        assert_eq!(target_id("macos", "aarch64"), Some("osx-arm64"));
        assert_eq!(target_id("linux", "x86_64"), Some("linux-x64"));
        assert_eq!(target_id("macos", "x86_64"), None);
        assert_eq!(target_id("linux", "aarch64"), None);

        let total = 100 * 1024 * 1024_u64;
        let fresh = required_disk_bytes(total, false);
        let repair = required_disk_bytes(total, true);
        assert!(fresh > total, "margin must be added");
        assert_eq!(repair - fresh, total, "repair budgets a second staged copy");
    }
}
