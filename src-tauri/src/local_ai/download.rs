//! Shared component downloader primitives.
//!
//! Moved out of `embedding::local::download` in T2: the streamed atomic
//! download with resume + in-pass hashing, pin checks, the archive member
//! extractor, the promote/rollback transaction, trash sweeping, disk probing,
//! and the progress/report shapes shared by Bango Local embeddings and
//! Bango AI. Component-specific install orchestration stays in the owning
//! modules.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::local_ai::manifest::{
    is_archive_safe_path, is_path_safe_file_name, target_id, ArchiveAlias, ArchiveMember,
};

/// Staging directory name under an artifact root (dot-dir: never probed as an
/// installation).
pub const STAGING_DIR_NAME: &str = ".staging";

/// Installation manifest file name written into a promoted install directory.
pub const INSTALL_MANIFEST_NAME: &str = "manifest.json";

/// Maximum time a single chunk read may take before the download errors out
/// (a connected-but-stalled body must not hang the install; cancel remains
/// the between-chunk path).
pub const READ_STALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Progress payload for install/verify operations (emitted as component
/// events by each backend's command layer).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    /// "downloading" | "verifying" | "installing" | "done" | "error".
    pub phase: String,
    pub file: String,
    pub file_bytes: u64,
    pub file_total: u64,
    pub overall_bytes: u64,
    pub overall_total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Outcome of a successful install run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// Files fetched from the network this run.
    pub downloaded: usize,
    /// Files already matching their pins (skipped, no fetch).
    pub skipped: usize,
    /// Bytes transferred this run.
    pub bytes_downloaded: u64,
}

/// One failed verification check (repair diagnostics).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationFailure {
    pub name: String,
    pub reason: String,
}

/// Whether the current OS/arch target is supported for local installs.
#[must_use]
pub fn supported_target() -> bool {
    target_supported(std::env::consts::OS, std::env::consts::ARCH)
}

/// Pure mapping of supported targets: win-x64, osx-arm64, linux-x64.
/// macOS x86_64 is NOT supported (no runtime builds meet the pinned floors),
/// so Intel Macs fall back to the configured provider.
#[must_use]
pub fn target_supported(os: &str, arch: &str) -> bool {
    target_id(os, arch).is_some()
}

/// Best-effort free space at `path` (`None` when the volume cannot be probed;
/// callers treat that as "gate skipped").
#[must_use]
pub fn available_bytes(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

/// AppError-mapping adapter for std::io failures (shared by both backends).
pub(crate) fn io_err(context: &str) -> impl Fn(std::io::Error) -> AppError + '_ {
    move |e| AppError::Io(std::io::Error::other(format!("{context}: {e}")))
}

/// Partial-download suffix (never mistaken for an installed file).
fn append_part_extension(dest: &Path) -> PathBuf {
    let mut name =
        dest.file_name().map_or_else(|| "file".to_string(), |n| n.to_string_lossy().to_string());
    name.push_str(".part");
    dest.with_file_name(name)
}

/// Whether `path` matches the pins (exact size; hash when pinned).
/// Hash-verification streams the file (constant memory).
pub fn file_matches_pins(
    path: &Path,
    expected_size: u64,
    expected_sha256: Option<&str>,
) -> Result<bool, AppError> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != expected_size {
        return Ok(false);
    }
    match expected_sha256 {
        None => Ok(true),
        Some(expected) => {
            let mut file = std::fs::File::open(path).map_err(io_err("verify open"))?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher).map_err(io_err("verify read"))?;
            Ok(format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected))
        }
    }
}

/// Stream `url` into `dest` atomically: chunks land in `<dest>.part` with
/// the hash computed in-pass; pins are verified; only then is the part
/// renamed into place. Returns `Ok(false)` when `dest` already matches the
/// pins (idempotent skip).
///
/// Resume: a partial `<dest>.part` from an interrupted transfer continues
/// with a `Range: bytes=<n>-` request (the local prefix is hashed first so
/// verification still covers the whole file); a server that ignores the
/// Range (HTTP 200 instead of 206) restarts the transfer cleanly. An
/// oversized or complete-but-unrenamed part also restarts.
///
/// `cancel` is checked between chunks AND before the final rename, so a
/// single-chunk body is still cancellable. Each chunk read is bounded by
/// [`READ_STALL_TIMEOUT`] so a stalled body errors out instead of hanging.
pub async fn download_file(
    url: &str,
    dest: &Path,
    expected_size: u64,
    expected_sha256: Option<&str>,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<bool, AppError> {
    // Blocking prepare (may hash a large partial prefix on resume, or a full
    // staged file on the skip check): runs on the blocking pool so async
    // workers never churn through large hashing.
    enum Prepared {
        Skip,
        Fresh { file: std::fs::File },
        Resume { file: std::fs::File, hasher: Sha256, offset: u64 },
    }
    let part = append_part_extension(dest);
    let prepare_dest = dest.to_path_buf();
    let prepare_part = part.clone();
    let pinned_hash = expected_sha256.map(str::to_string);
    let prepared = tokio::task::spawn_blocking(move || -> Result<Prepared, AppError> {
        if let Some(parent) = prepare_part.parent() {
            std::fs::create_dir_all(parent).map_err(io_err("staging mkdir"))?;
        }
        if file_matches_pins(&prepare_dest, expected_size, pinned_hash.as_deref())? {
            return Ok(Prepared::Skip);
        }
        // Resume decision: a partial prefix strictly smaller than the pin
        // continues; anything else (absent, oversized) restarts.
        let existing = std::fs::metadata(&prepare_part).map_or(0, |m| m.len());
        if existing > 0 && existing < expected_size {
            let mut hasher = Sha256::new();
            let mut prefix = std::fs::File::open(&prepare_part).map_err(io_err("resume open"))?;
            std::io::copy(&mut prefix, &mut hasher).map_err(io_err("resume hash"))?;
            let file = std::fs::OpenOptions::new()
                .append(true)
                .open(&prepare_part)
                .map_err(io_err("resume append"))?;
            Ok(Prepared::Resume { file, hasher, offset: existing })
        } else {
            Ok(Prepared::Fresh {
                file: std::fs::File::create(&prepare_part).map_err(io_err("staging create"))?,
            })
        }
    })
    .await
    .map_err(|e| AppError::Import(format!("download prepare task panicked: {e}")))??;
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Import(format!("HTTP client init failed: {e}")))?;
    let (mut file, mut hasher, mut done, resume_from) = match prepared {
        Prepared::Skip => return Ok(false),
        Prepared::Fresh { file } => (file, Sha256::new(), 0u64, None),
        Prepared::Resume { file, hasher, offset } => (file, hasher, offset, Some(offset)),
    };

    let mut request = client.get(url);
    if let Some(offset) = resume_from {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| AppError::Import(format!("download failed for {}: {e}", dest.display())))?;
    if resume_from.is_some() && response.status() == reqwest::StatusCode::OK {
        // Server ignored the Range: restart cleanly from byte 0.
        file = std::fs::File::create(&part).map_err(io_err("staging create"))?;
        hasher = Sha256::new();
        done = 0;
    } else if !response.status().is_success() {
        return Err(AppError::Import(format!(
            "download failed for {}: HTTP {}",
            dest.display(),
            response.status()
        )));
    }
    progress(done, expected_size);

    while let Some(chunk) = tokio::time::timeout(READ_STALL_TIMEOUT, response.chunk())
        .await
        .map_err(|_| {
            AppError::Import(format!(
                "download stalled for {}: no data for {}s",
                dest.display(),
                READ_STALL_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|e| {
            AppError::Import(format!("download stream failed for {}: {e}", dest.display()))
        })?
    {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Import("Cancelled".to_string()));
        }
        file.write_all(&chunk).map_err(io_err("staging write"))?;
        hasher.update(&chunk);
        done += chunk.len() as u64;
        if done > expected_size {
            let _ = std::fs::remove_file(&part);
            return Err(AppError::Import(format!(
                "download for {} exceeded the pinned size",
                dest.display()
            )));
        }
        progress(done, expected_size);
    }
    file.flush().map_err(io_err("staging flush"))?;
    // Post-loop cancel check covers single-chunk bodies. Keep the partial
    // file so a retry resumes instead of restarting.
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::Import("Cancelled".to_string()));
    }
    if done != expected_size {
        let _ = std::fs::remove_file(&part);
        return Err(AppError::Import(format!(
            "download for {} is incomplete: {done} of {expected_size} bytes",
            dest.display()
        )));
    }
    if let Some(expected) = expected_sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(AppError::Import(format!(
                "hash mismatch for {}: expected {expected}, got {actual}",
                dest.display()
            )));
        }
    }
    std::fs::rename(&part, dest).map_err(io_err("staging rename"))?;
    Ok(true)
}

/// Swap a verified staging directory into place, with rollback.
///
/// Steps: park any existing install as `.staging/replaced-<ts>`, rename
/// staging into place, write `install_manifest_json` as the installation
/// manifest (the pinned manifest IS the provenance record), then best-effort
/// clean staging leftovers. If the promote rename fails, the parked working
/// install is restored before the error surfaces - a failed install never
/// leaves the final directory missing when a working copy existed. (A process
/// crash between the two renames strands the working copy under
/// `.staging/replaced-*`; state assessment surfaces that as repair-required.)
pub fn promote_install(
    model_root: &Path,
    staging_dir: &Path,
    final_dir: &Path,
    install_manifest_json: &str,
) -> Result<(), AppError> {
    let mut parked: Option<PathBuf> = None;
    if final_dir.exists() {
        let replaced = model_root
            .join(STAGING_DIR_NAME)
            .join(format!("replaced-{}", chrono::Utc::now().timestamp()));
        std::fs::rename(final_dir, &replaced).map_err(io_err("replace old install"))?;
        parked = Some(replaced);
    }
    if let Err(e) = std::fs::rename(staging_dir, final_dir) {
        // Rollback: restore the parked working install before surfacing.
        if let Some(replaced) = parked {
            if let Err(restore_err) = std::fs::rename(&replaced, final_dir) {
                return Err(AppError::Import(format!(
                    "install promote failed ({e}) and rollback failed too ({restore_err}); \
                     the previous installation remains parked at {}",
                    replaced.display()
                )));
            }
        }
        return Err(AppError::Import(format!("install promote failed: {e}")));
    }
    std::fs::write(final_dir.join(INSTALL_MANIFEST_NAME), install_manifest_json)
        .map_err(io_err("write installation manifest"))?;
    // Best-effort cleanup of replaced versions + leftovers.
    if let Ok(entries) = std::fs::read_dir(model_root.join(STAGING_DIR_NAME)) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

/// Extract a single `member` from a `.zip` or tar.gz (`.tgz`) archive into
/// `dest`. The member path comes from the pinned manifest; defense-in-depth
/// rejects path traversal.
pub fn extract_archive_member(archive: &Path, member: &str, dest: &Path) -> Result<(), AppError> {
    let member_path = std::path::Path::new(member);
    if member.is_empty() || member.contains("..") || member.starts_with('/') {
        return Err(AppError::Validation(format!("unsafe archive member: '{member}'")));
    }
    let file_name = member_path.file_name().ok_or_else(|| {
        AppError::Validation(format!("archive member has no file name: '{member}'"))
    })?;
    // Compare member paths component-wise, ignoring `.` components: the
    // macOS archives store members with a leading `./` prefix.
    let member_components: Vec<_> =
        member_path.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect();
    let same_path = |entry: &std::path::Path| -> bool {
        entry.file_name().is_some_and(|n| n == file_name)
            && entry
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir))
                .eq(member_components.iter().copied())
    };
    if archive.extension().is_some_and(|e| e == "zip") {
        let reader = std::fs::File::open(archive).map_err(io_err("archive open"))?;
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| {
            AppError::Import(format!("zip open failed for {}: {e}", archive.display()))
        })?;
        // Parity with the tar path: entries may carry a leading `./` prefix.
        // Resolve the stored name via the immutable `file_names()` view first
        // (a mutable `by_name` double-borrow is not allowed), then open it.
        let prefixed = format!("./{member}");
        let stored_name = if zip.file_names().any(|n| n == member) {
            member.to_string()
        } else if zip.file_names().any(|n| n == prefixed) {
            prefixed
        } else {
            return Err(AppError::Import(format!("archive member '{member}' not found")));
        };
        let mut entry = zip
            .by_name(&stored_name)
            .map_err(|_| AppError::Import(format!("archive member '{member}' not found")))?;
        // Same member-type discipline as the tar path: directories and
        // symlink-style entries must not be extracted as file contents.
        if entry.is_dir() || entry.is_symlink() {
            return Err(AppError::Import(format!(
                "archive member '{member}' is not a regular file (symlink?)"
            )));
        }
        let mut out = std::fs::File::create(dest).map_err(io_err("extract create"))?;
        std::io::copy(&mut entry, &mut out).map_err(io_err("extract copy"))?;
        mark_executable(dest)?;
        return Ok(());
    }
    // Default: tar.gz (.tgz / .tar.gz).
    let reader = std::fs::File::open(archive).map_err(io_err("archive open"))?;
    let decoder = flate2::read::GzDecoder::new(reader);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar
        .entries()
        .map_err(|e| AppError::Import(format!("tar read failed for {}: {e}", archive.display())))?
    {
        let mut entry = entry.map_err(|e| AppError::Import(format!("tar entry failed: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| AppError::Import(format!("tar path failed: {e}")))?
            .to_path_buf();
        if same_path(&path) {
            if !matches!(entry.header().entry_type(), tar::EntryType::Regular) {
                // The pinned member must be the real file, not a symlink
                // (Microsoft ships the unversioned names as links into the
                // versioned file; copying a link entry yields zero bytes).
                return Err(AppError::Import(format!(
                    "archive member '{member}' is not a regular file (symlink?)"
                )));
            }
            let mut out = std::fs::File::create(dest).map_err(io_err("extract create"))?;
            std::io::copy(&mut entry, &mut out).map_err(io_err("extract copy"))?;
            mark_executable(dest)?;
            return Ok(());
        }
    }
    Err(AppError::Import(format!("archive member '{member}' not found")))
}

/// Extract a pinned member SET from a `.zip` or tar.gz (`.tgz`) archive into
/// `dest_dir`, flattening every member to its file name, then materialize
/// soname aliases as copies (portable: no symlink privileges required).
/// Every pinned member must be found at its pinned size; traversal, duplicate
/// destinations, unknown members, and non-regular entries are rejected.
pub fn extract_archive_bundle(
    archive: &Path,
    dest_dir: &Path,
    members: &[ArchiveMember],
    aliases: &[ArchiveAlias],
) -> Result<(), AppError> {
    let mut expected: HashMap<String, (&ArchiveMember, String)> = HashMap::new();
    for member in members {
        if !is_archive_safe_path(&member.path) {
            return Err(AppError::Validation(format!("unsafe archive member: '{}'", member.path)));
        }
        let base =
            Path::new(&member.path).file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                AppError::Validation(format!("archive member has no file name: '{}'", member.path))
            })?;
        if !is_path_safe_file_name(base) {
            return Err(AppError::Validation(format!("unsafe archive destination: '{base}'")));
        }
        let normalized = normalize_archive_path(&member.path);
        if expected.insert(normalized, (member, base.to_string())).is_some() {
            return Err(AppError::Validation(format!(
                "duplicate archive member: '{}'",
                member.path
            )));
        }
    }
    std::fs::create_dir_all(dest_dir).map_err(io_err("bundle dir create"))?;
    let mut found: HashSet<String> = HashSet::new();

    if archive.extension().is_some_and(|e| e == "zip") {
        let reader = std::fs::File::open(archive).map_err(io_err("archive open"))?;
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| {
            AppError::Import(format!("zip open failed for {}: {e}", archive.display()))
        })?;
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| AppError::Import(format!("zip entry failed: {e}")))?;
            let name = normalize_archive_path(entry.name());
            let Some((member, base)) = expected.get(&name) else {
                continue;
            };
            if entry.is_dir() || entry.is_symlink() {
                return Err(AppError::Import(format!(
                    "archive member '{}' is not a regular file (symlink?)",
                    member.path
                )));
            }
            if entry.size() != member.size {
                return Err(AppError::Import(format!(
                    "archive member '{}' size mismatch: expected {}, found {}",
                    member.path,
                    member.size,
                    entry.size()
                )));
            }
            let dest = dest_dir.join(base.as_str());
            let mut out = std::fs::File::create(&dest).map_err(io_err("extract create"))?;
            std::io::copy(&mut entry, &mut out).map_err(io_err("extract copy"))?;
            if member.executable {
                mark_executable(&dest)?;
            }
            found.insert(name);
        }
    } else {
        let reader = std::fs::File::open(archive).map_err(io_err("archive open"))?;
        let decoder = flate2::read::GzDecoder::new(reader);
        let mut tar = tar::Archive::new(decoder);
        for entry in tar.entries().map_err(|e| {
            AppError::Import(format!("tar read failed for {}: {e}", archive.display()))
        })? {
            let mut entry =
                entry.map_err(|e| AppError::Import(format!("tar entry failed: {e}")))?;
            let path =
                entry.path().map_err(|e| AppError::Import(format!("tar path failed: {e}")))?;
            let name = normalize_archive_path(&path.to_string_lossy());
            let Some((member, base)) = expected.get(&name) else {
                continue;
            };
            if !matches!(entry.header().entry_type(), tar::EntryType::Regular) {
                return Err(AppError::Import(format!(
                    "archive member '{}' is not a regular file (symlink?)",
                    member.path
                )));
            }
            if entry.size() != member.size {
                return Err(AppError::Import(format!(
                    "archive member '{}' size mismatch: expected {}, found {}",
                    member.path,
                    member.size,
                    entry.size()
                )));
            }
            let dest = dest_dir.join(base.as_str());
            let mut out = std::fs::File::create(&dest).map_err(io_err("extract create"))?;
            std::io::copy(&mut entry, &mut out).map_err(io_err("extract copy"))?;
            if member.executable {
                mark_executable(&dest)?;
            }
            found.insert(name);
        }
    }

    for (path, (member, _)) in &expected {
        if !found.contains(path) {
            return Err(AppError::Import(format!("archive member '{}' not found", member.path)));
        }
    }
    write_archive_aliases(dest_dir, aliases, &expected)
}

/// Normalize an archive path: strip a single leading `./` (macOS archives).
fn normalize_archive_path(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

/// Materialize alias copies after extraction; alias targets may chain, cycles
/// are rejected.
fn write_archive_aliases(
    dest_dir: &Path,
    aliases: &[ArchiveAlias],
    expected: &HashMap<String, (&ArchiveMember, String)>,
) -> Result<(), AppError> {
    let member_bases: HashSet<&str> = expected.values().map(|(_, base)| base.as_str()).collect();
    for alias in aliases {
        if !is_path_safe_file_name(&alias.path) || !is_path_safe_file_name(&alias.target) {
            return Err(AppError::Validation(format!("unsafe archive alias: '{}'", alias.path)));
        }
        let mut current = alias.target.as_str();
        let mut hops = 0usize;
        loop {
            if member_bases.contains(current) {
                break;
            }
            let Some(next) = aliases.iter().find(|a| a.path == current) else {
                return Err(AppError::Import(format!(
                    "alias '{}' targets unknown file '{current}'",
                    alias.path
                )));
            };
            current = next.target.as_str();
            hops += 1;
            if hops > aliases.len() {
                return Err(AppError::Import(format!("alias cycle at '{}'", alias.path)));
            }
        }
        std::fs::copy(dest_dir.join(current), dest_dir.join(&alias.path))
            .map_err(io_err("alias copy"))?;
    }
    Ok(())
}

/// An extracted executable must be loadable: `File::create` yields the
/// umask default (usually rw-r--r--), which fails to dlopen/exec on unix.
fn mark_executable(dest: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dest).map_err(io_err("extract stat"))?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dest, perms).map_err(io_err("extract chmod"))?;
    }
    #[cfg(not(unix))]
    let _ = dest;
    Ok(())
}

/// Best-effort sweep of trash directories left by earlier locked removes
/// (Windows): a loaded runtime library cannot be deleted, so removal renames
/// the tree aside; each later remove/install retries the deletion.
pub fn sweep_trash(root: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Map NotFound to Ok so removal stays idempotent.
pub fn cleanup_ok(e: std::io::Error) -> std::result::Result<(), std::io::Error> {
    if e.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, body) in entries {
            zip.start_file(name.to_string(), options).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
        cursor.into_inner()
    }

    #[test]
    fn bundle_extracts_members_materializes_aliases_and_rejects_mismatches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let archive = dir.path().join("bundle.zip");
        std::fs::write(
            &archive,
            bundle_zip(&[("root/server", b"server-bytes"), ("root/lib.so", b"lib-bytes!!")]),
        )
        .expect("write archive");
        let dest = dir.path().join("out");
        let members = vec![
            ArchiveMember { path: "root/server".into(), size: 12, executable: true },
            ArchiveMember { path: "root/lib.so".into(), size: 11, executable: false },
        ];
        let aliases = vec![ArchiveAlias { path: "lib.so.0".into(), target: "lib.so".into() }];
        extract_archive_bundle(&archive, &dest, &members, &aliases).expect("bundle extracts");
        assert_eq!(std::fs::read(dest.join("server")).unwrap(), b"server-bytes");
        assert_eq!(std::fs::read(dest.join("lib.so.0")).unwrap(), b"lib-bytes!!");

        // A pinned size that does not match the entry is rejected.
        let bad = vec![ArchiveMember { path: "root/lib.so".into(), size: 1, executable: false }];
        assert!(extract_archive_bundle(&archive, &dir.path().join("out2"), &bad, &[]).is_err());
        // Traversal and unknown aliases are rejected.
        let evil = vec![ArchiveMember { path: "../escape".into(), size: 1, executable: false }];
        assert!(extract_archive_bundle(&archive, &dir.path().join("out3"), &evil, &[]).is_err());
        let ghost = vec![ArchiveAlias { path: "ghost.so".into(), target: "missing.so".into() }];
        assert!(
            extract_archive_bundle(&archive, &dir.path().join("out4"), &members, &ghost).is_err()
        );
    }

    #[test]
    fn bundle_extracts_from_tar_gz() {
        let dir = tempfile::tempdir().expect("tempdir");
        let archive = dir.path().join("bundle.tgz");
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(6);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "root/server", &b"server"[..]).unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        std::fs::write(&archive, bytes).expect("write archive");
        let dest = dir.path().join("out");
        let members = vec![ArchiveMember { path: "root/server".into(), size: 6, executable: true }];
        extract_archive_bundle(&archive, &dest, &members, &[]).expect("tar bundle extracts");
        assert_eq!(std::fs::read(dest.join("server")).unwrap(), b"server");
    }

    #[test]
    fn target_supported_matrix() {
        assert!(target_supported("windows", "x86_64"));
        assert!(target_supported("macos", "aarch64"));
        assert!(target_supported("linux", "x86_64"));
        assert!(!target_supported("windows", "aarch64"));
        assert!(!target_supported("linux", "aarch64"));
        assert!(!target_supported("android", "x86_64"));
        // Intel Macs are unsupported: no osx-x86_64 runtime builds exist for
        // any runtime release meeting the pinned floors.
        assert!(!target_supported("macos", "x86_64"));
    }

    #[test]
    fn part_extension_appends_to_file_name() {
        let part = append_part_extension(Path::new("/root/profile/model_q4.onnx"));
        assert_eq!(part, Path::new("/root/profile/model_q4.onnx.part"));
    }
}
