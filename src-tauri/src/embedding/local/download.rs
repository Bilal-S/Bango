//! Atomic downloader + installer for the Bango Local embedding components.
//!
//! Install transaction (per the plan's component-manager tier): stream every
//! pinned file into `<model_root>/.staging/<profile>/` with the SHA-256
//! computed in-pass, verify (size always, hash when pinned), then swap the
//! staging directory into place with same-filesystem renames and write the
//! installation manifest. A working installation is never partially
//! replaced; an interrupted download leaves only discardable staging files.
//! Re-running the install is idempotent: a healthy installation
//! short-circuits without any network fetch, so install doubles as repair
//! (a corrupt or partial install is repaired by a full re-download + swap).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::embedding::local::manifest::ComponentManifest;
use crate::embedding::local::profile::LOCAL_PROFILE_DIR;
use crate::embedding::local::state::INSTALL_MANIFEST_NAME;
use crate::error::AppError;

/// Staging directory name under the model root (dot-dir: never probed as an
/// installation by `state::probe_installation_state`).
pub const STAGING_DIR_NAME: &str = ".staging";

/// Maximum time a single chunk read may take before the download errors out
/// (a connected-but-stalled body must not hang the install; cancel remains
/// the between-chunk path).
pub const READ_STALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Progress payload for install/verify operations (emitted as
/// `embedding:component` events by the command layer).
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

/// Outcome of a successful [`install_profile`] run.
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
/// macOS x86_64 is NOT supported: Microsoft ships no osx-x86_64 ONNX
/// Runtime builds in any release new enough for ort's API-27 floor
/// (verified across 1.27-1.30), so Intel Macs fall back to the configured
/// provider.
#[must_use]
pub fn target_supported(os: &str, arch: &str) -> bool {
    matches!((os, arch), ("windows", "x86_64") | ("macos", "aarch64") | ("linux", "x86_64"))
}

/// Best-effort free space at `path` (`None` when the volume cannot be probed;
/// callers treat that as "gate skipped").
#[must_use]
pub fn available_bytes(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

fn io_err(context: &str) -> impl Fn(std::io::Error) -> AppError + '_ {
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
fn file_matches_pins(
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
    // workers never churn through ~200 MB of hashing.
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

/// Full install transaction for the pinned manifest (see module docs).
/// Idempotent + repair-capable: files already matching pins are skipped.
pub async fn install_profile(
    model_root: &Path,
    manifest: &ComponentManifest,
    progress: &(dyn Fn(InstallProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<InstallReport, AppError> {
    manifest
        .validate()
        .map_err(|reason| AppError::Validation(format!("invalid manifest: {reason}")))?;

    // Fast path (hashing ~218 MB, so it runs on the blocking pool): an
    // already-healthy installation needs nothing (repair of a corrupt or
    // partial install re-downloads fully into staging, then swaps).
    let fast_path_root = model_root.to_path_buf();
    let fast_path_manifest = manifest.clone();
    let healthy = tokio::task::spawn_blocking(move || {
        verify_installed(&fast_path_root, &fast_path_manifest).map(|f| f.is_empty())
    })
    .await
    .map_err(|e| AppError::Import(format!("fast-path task panicked: {e}")))??;
    if healthy {
        return Ok(InstallReport {
            downloaded: 0,
            skipped: manifest.files.len(),
            bytes_downloaded: 0,
        });
    }

    let staging_dir = model_root.join(STAGING_DIR_NAME).join(LOCAL_PROFILE_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(io_err("staging mkdir"))?;
    let overall_total = manifest.total_download_bytes();
    let mut overall_done: u64 = 0;
    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut bytes_downloaded: u64 = 0;

    // Phase 1: download (or skip) every pinned file into staging.
    for file in &manifest.files {
        let dest = staging_dir.join(&file.name);
        let pinned_hash = file.sha256.as_deref();
        let file_total = file.size;
        let fetched = download_file(
            &file.url,
            &dest,
            file.size,
            pinned_hash,
            &|file_bytes, total| {
                progress(InstallProgress {
                    phase: "downloading".to_string(),
                    file: file.name.clone(),
                    file_bytes,
                    file_total: total,
                    overall_bytes: overall_done + file_bytes.min(file_total),
                    overall_total,
                    message: None,
                });
            },
            cancel,
        )
        .await?;
        if fetched {
            downloaded += 1;
            bytes_downloaded += file.size;
        } else {
            skipped += 1;
        }
        overall_done += file.size;
    }

    // Phase 2: verify every staged file against its pins (defense-in-depth;
    // the download already verified in-pass). Hashing ~218 MB is blocking, so
    // each check runs on the blocking pool - the async workers stay free.
    for file in &manifest.files {
        progress(InstallProgress {
            phase: "verifying".to_string(),
            file: file.name.clone(),
            file_bytes: file.size,
            file_total: file.size,
            overall_bytes: overall_done,
            overall_total,
            message: None,
        });
        let path = staging_dir.join(&file.name);
        let expected_size = file.size;
        let pinned_hash = file.sha256.clone();
        let ok = tokio::task::spawn_blocking(move || {
            file_matches_pins(&path, expected_size, pinned_hash.as_deref())
        })
        .await
        .map_err(|e| AppError::Import(format!("verification task panicked: {e}")))??;
        if !ok {
            return Err(AppError::Import(format!("verification failed for {}", file.name)));
        }
    }

    // Phase 3: swap staging into place (blocking: renames + manifest write).
    progress(InstallProgress {
        phase: "installing".to_string(),
        file: String::new(),
        file_bytes: overall_done,
        file_total: overall_done,
        overall_bytes: overall_done,
        overall_total,
        message: None,
    });
    let final_dir = model_root.join(LOCAL_PROFILE_DIR);
    let promote_root = model_root.to_path_buf();
    let promote_staging = staging_dir;
    let promote_final = final_dir;
    let promote_manifest = manifest.clone();
    tokio::task::spawn_blocking(move || {
        promote_install(&promote_root, &promote_staging, &promote_final, &promote_manifest)
    })
    .await
    .map_err(|e| AppError::Import(format!("promote task panicked: {e}")))??;

    Ok(InstallReport { downloaded, skipped, bytes_downloaded })
}

/// Swap the verified staging directory into place, with rollback.
///
/// Steps: park any existing install as `.staging/replaced-<ts>`, rename
/// staging into place, write the installation manifest (the pinned manifest
/// IS the provenance record), then best-effort-clean staging leftovers. If
/// the promote rename fails, the parked working install is restored before
/// the error surfaces - a failed install never leaves the profile directory
/// missing when a working copy existed. (A process crash between the two
/// renames strands the working copy under `.staging/replaced-*`;
/// `assess_installation` surfaces that as `repair_required` and the retry
/// re-downloads - re-promoting the parked copy directly is a future
/// optimization.)
pub fn promote_install(
    model_root: &Path,
    staging_dir: &Path,
    final_dir: &Path,
    manifest: &ComponentManifest,
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
    let manifest_json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(final_dir.join(INSTALL_MANIFEST_NAME), manifest_json)
        .map_err(io_err("write installation manifest"))?;
    // Best-effort cleanup of replaced versions + leftovers.
    if let Ok(entries) = std::fs::read_dir(model_root.join(STAGING_DIR_NAME)) {
        for entry in entries.filter_map(std::result::Result::ok) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

/// The installed runtime library path for the current target
/// (`{runtime_root}/onnxruntime/<version>/<lib filename>`), when the
/// manifest pins a runtime for this target.
#[must_use]
pub fn runtime_lib_file(runtime_root: &Path, manifest: &ComponentManifest) -> Option<PathBuf> {
    let file = manifest.runtime_file_for_current_target()?;
    let name = Path::new(&file.lib_path).file_name()?;
    Some(
        runtime_root
            .join(&manifest.runtime.as_ref()?.name)
            .join(&manifest.runtime.as_ref()?.version)
            .join(name),
    )
}

/// Whether the installed library exists at its pinned uncompressed size
/// (the archive SHA-256 pins content transitively at download time; this
/// catches truncation/corruption of the extracted file).
#[must_use]
pub fn runtime_library_healthy(lib_file: &Path, lib_size: u64) -> bool {
    std::fs::metadata(lib_file).is_ok_and(|m| m.is_file() && m.len() == lib_size)
}

/// Verify the installed runtime component: the current target's library
/// exists at its pinned uncompressed size. `Err(message)` on failure
/// (folded into the Verify Installation outcome).
pub fn verify_runtime(runtime_root: &Path, manifest: &ComponentManifest) -> Result<(), String> {
    let Some(file) = manifest.runtime_file_for_current_target() else {
        return Ok(()); // no runtime pinned for this target: nothing to verify
    };
    let Some(lib) = runtime_lib_file(runtime_root, manifest) else {
        return Err("runtime library path is malformed".to_string());
    };
    if runtime_library_healthy(&lib, file.lib_size) {
        Ok(())
    } else if lib.is_file() {
        Err(format!(
            "{}: size {} bytes, expected {} (re-install to repair)",
            lib.display(),
            std::fs::metadata(&lib).map_or(0, |m| m.len()),
            file.lib_size
        ))
    } else {
        Err(format!("{} is missing (re-install to repair)", lib.display()))
    }
}

/// Install (or repair) the ONNX Runtime component for the current target:
/// download the pinned archive into `runtime_root/.staging/`, verify its
/// SHA-256, extract ONLY the pinned library member into
/// `runtime_root/onnxruntime/<version>/`, write a version manifest, and
/// remove the archive. Idempotent: an existing library file skips the
/// download entirely.
pub async fn install_runtime(
    runtime_root: &Path,
    manifest: &ComponentManifest,
    progress: &(dyn Fn(InstallProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<InstallReport, AppError> {
    let runtime = manifest
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::Validation("manifest has no runtime component".to_string()))?;
    let file = manifest.runtime_file_for_current_target().ok_or_else(|| {
        AppError::Validation(
            "Bango Local is not available for this system. You can continue using your \
             configured embedding provider."
                .to_string(),
        )
    })?;
    let version_dir = runtime_root.join(&runtime.name).join(&runtime.version);
    let lib_file = runtime_lib_file(runtime_root, manifest).ok_or_else(|| {
        AppError::Validation(format!("runtime libPath has no file name: '{}'", file.lib_path))
    })?;
    let total = file.size;

    // A prior locked remove (Windows, L1 findings-7) may have left a trash
    // tree; retry its deletion now that the process may have restarted.
    sweep_runtime_trash(runtime_root);

    // Idempotent skip with an integrity check: the extracted library exists
    // AND matches its pinned uncompressed size (a truncated/corrupt library
    // falls through to the re-download + re-extract repair path).
    if runtime_library_healthy(&lib_file, file.lib_size) {
        return Ok(InstallReport { downloaded: 0, skipped: 1, bytes_downloaded: 0 });
    }

    // Download the pinned archive into runtime staging.
    let staging_dir = runtime_root.join(STAGING_DIR_NAME);
    std::fs::create_dir_all(&staging_dir).map_err(io_err("runtime staging mkdir"))?;
    let archive = staging_dir.join(&file.name);
    let fetched = download_file(
        &file.url,
        &archive,
        file.size,
        Some(&file.sha256),
        &|file_bytes, file_total| {
            progress(InstallProgress {
                phase: "downloading".to_string(),
                file: file.name.clone(),
                file_bytes,
                file_total,
                overall_bytes: file_bytes,
                overall_total: total,
                message: None,
            });
        },
        cancel,
    )
    .await?;
    let bytes_downloaded = if fetched { file.size } else { 0 };

    // Extract only the pinned library member (blocking).
    progress(InstallProgress {
        phase: "installing".to_string(),
        file: file.name.clone(),
        file_bytes: file.size,
        file_total: file.size,
        overall_bytes: total,
        overall_total: total,
        message: None,
    });
    std::fs::create_dir_all(&version_dir).map_err(io_err("runtime version mkdir"))?;
    let archive_path = archive.clone();
    let member = file.lib_path.clone();
    let lib_path = lib_file.clone();
    let extracted = tokio::task::spawn_blocking(move || {
        extract_archive_member(&archive_path, &member, &lib_path)
    })
    .await
    .map_err(|e| AppError::Import(format!("runtime extract task panicked: {e}")))?;
    if let Err(e) = extracted {
        // Writing over an existing library fails on Windows while the DLL is
        // loaded (ort keeps it resident for the process lifetime): surface a
        // restart hint instead of a raw sharing-violation io error.
        return Err(AppError::Import(format!(
            "could not replace the ONNX Runtime library: {e}. If Bango has used local \
             embeddings this session, restart the app and repair again."
        )));
    }
    // Version record for provenance + the engine's dylib resolution.
    std::fs::write(
        version_dir.join(INSTALL_MANIFEST_NAME),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": runtime.name,
            "version": runtime.version,
            "target": file.target,
            "libPath": file.lib_path,
        }))?,
    )
    .map_err(io_err("write runtime manifest"))?;
    // The archive is no longer needed once extracted; the staging directory
    // itself is cleared too (a cancelled earlier download may have left a
    // `.part` behind).
    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir_all(runtime_root.join(STAGING_DIR_NAME));

    Ok(InstallReport {
        downloaded: usize::from(fetched),
        skipped: usize::from(!fetched),
        bytes_downloaded,
    })
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

/// The extracted library must be dlopen-able: `File::create` yields the
/// umask default (usually rw-r--r--), which fails to load on unix.
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

/// Checks the installation manifest's profile identity against the active
/// manifest first (r1-vs-r2 revision detection), then every file's pins.
pub fn verify_installed(
    model_root: &Path,
    manifest: &ComponentManifest,
) -> Result<Vec<VerificationFailure>, AppError> {
    let final_dir = model_root.join(LOCAL_PROFILE_DIR);
    let mut failures = Vec::new();
    let manifest_path = final_dir.join(INSTALL_MANIFEST_NAME);
    match std::fs::read_to_string(&manifest_path) {
        Ok(text) => match serde_json::from_str::<ComponentManifest>(&text) {
            Ok(installed) if installed.profile != manifest.profile => {
                failures.push(VerificationFailure {
                    name: INSTALL_MANIFEST_NAME.to_string(),
                    reason: format!(
                        "installed profile '{}' does not match the active profile '{}'",
                        installed.profile, manifest.profile
                    ),
                });
            }
            Ok(_) => {}
            Err(e) => failures.push(VerificationFailure {
                name: INSTALL_MANIFEST_NAME.to_string(),
                reason: format!("installation manifest is unparseable: {e}"),
            }),
        },
        Err(_) => failures.push(VerificationFailure {
            name: INSTALL_MANIFEST_NAME.to_string(),
            reason: "installation manifest is missing".to_string(),
        }),
    }
    for file in &manifest.files {
        let path = final_dir.join(&file.name);
        match std::fs::metadata(&path) {
            Err(_) => failures.push(VerificationFailure {
                name: file.name.clone(),
                reason: "file is missing".to_string(),
            }),
            Ok(meta) if meta.len() != file.size => failures.push(VerificationFailure {
                name: file.name.clone(),
                reason: format!("size mismatch: expected {}, found {}", file.size, meta.len()),
            }),
            Ok(_) => {
                if let Some(expected) = &file.sha256 {
                    if !file_matches_pins(&path, file.size, Some(expected))? {
                        failures.push(VerificationFailure {
                            name: file.name.clone(),
                            reason: "sha256 mismatch".to_string(),
                        });
                    }
                }
            }
        }
    }
    Ok(failures)
}

/// Remove all local embedding artifacts (model profile, staging leftovers,
/// ONNX Runtime directory). Idempotent: absent paths are fine. The paper
/// library is untouched.
///
/// `model_roots` accepts every candidate model root (the currently resolved
/// one plus any alternate - e.g. the app-data fallback after a storage-root
/// move), so a removal cannot orphan an older installation.
/// Best-effort sweep of runtime trash directories left by earlier locked
/// removes (L1, findings-7): on Windows a loaded ONNX Runtime DLL cannot be
/// deleted, so `remove_components` renames the tree aside instead of failing.
/// Each later remove/install retries the deletion; a still-locked dir stays
/// until the next restart-then-remove.
fn sweep_runtime_trash(runtime_root: &Path) {
    let Ok(entries) = std::fs::read_dir(runtime_root) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("onnxruntime.trash-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn remove_components(model_roots: &[&Path], runtime_root: &Path) -> Result<(), AppError> {
    sweep_runtime_trash(runtime_root);
    for model_root in model_roots {
        std::fs::remove_dir_all(model_root.join(LOCAL_PROFILE_DIR))
            .or_else(cleanup_ok)
            .map_err(io_err("remove model"))?;
        std::fs::remove_dir_all(model_root.join(STAGING_DIR_NAME))
            .or_else(cleanup_ok)
            .map_err(io_err("remove staging"))?;
        // Remove the model root itself when now empty (e.g. `{storage_root}/model`).
        if let Ok(entries) = std::fs::read_dir(model_root) {
            if entries.filter_map(std::result::Result::ok).count() == 0 {
                let _ = std::fs::remove_dir(model_root);
            }
        }
    }
    // L1 (findings-7): on Windows the loaded ONNX Runtime DLL (kept resident
    // by ort's process-global environment even after `engine.reset()`)
    // cannot be deleted. Try a plain delete first; on failure rename the
    // tree aside so the user-visible Remove always succeeds and a later
    // remove/install (or app restart) sweeps it.
    let runtime_tree = runtime_root.join("onnxruntime");
    if std::fs::remove_dir_all(&runtime_tree).is_err() && runtime_tree.exists() {
        let trash = runtime_root.join(format!(
            "onnxruntime.trash-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        ));
        let _ = std::fs::rename(&runtime_tree, &trash);
    }
    // Runtime staging (a cancelled runtime download leaves `.part` files).
    std::fs::remove_dir_all(runtime_root.join(STAGING_DIR_NAME))
        .or_else(cleanup_ok)
        .map_err(io_err("remove runtime staging"))?;
    Ok(())
}

/// Map NotFound to Ok so removal stays idempotent.
fn cleanup_ok(e: std::io::Error) -> std::result::Result<(), std::io::Error> {
    if e.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_supported_matrix() {
        assert!(target_supported("windows", "x86_64"));
        assert!(target_supported("macos", "aarch64"));
        assert!(target_supported("linux", "x86_64"));
        assert!(!target_supported("windows", "aarch64"));
        assert!(!target_supported("linux", "aarch64"));
        assert!(!target_supported("android", "x86_64"));
        // Intel Macs are unsupported: no osx-x86_64 runtime builds exist for
        // any ONNX Runtime release meeting ort's API floor.
        assert!(!target_supported("macos", "x86_64"));
    }

    #[test]
    fn part_extension_appends_to_file_name() {
        let part = append_part_extension(Path::new("/root/profile/model_q4.onnx"));
        assert_eq!(part, Path::new("/root/profile/model_q4.onnx.part"));
    }
}
