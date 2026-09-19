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
//!
//! Generic primitives (streamed download + resume, pin checks, the
//! promote/rollback transaction, archive member extraction, trash sweeping,
//! progress/report shapes) live in `local_ai::download` and are re-exported
//! here so the embedding module's public API is unchanged.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::embedding::local::manifest::ComponentManifest;
use crate::embedding::local::profile::LOCAL_PROFILE_DIR;
use crate::embedding::local::state::INSTALL_MANIFEST_NAME;
use crate::error::AppError;

pub use crate::local_ai::download::{
    available_bytes, extract_archive_member, supported_target, target_supported, InstallProgress,
    InstallReport, VerificationFailure,
};
use crate::local_ai::download::{
    cleanup_ok, download_file, file_matches_pins, io_err,
    promote_install as shared_promote_install, sweep_trash, STAGING_DIR_NAME,
};

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
    let promote_manifest_json = serde_json::to_string_pretty(manifest)?;
    tokio::task::spawn_blocking(move || {
        shared_promote_install(
            &promote_root,
            &promote_staging,
            &promote_final,
            &promote_manifest_json,
        )
    })
    .await
    .map_err(|e| AppError::Import(format!("promote task panicked: {e}")))??;

    Ok(InstallReport { downloaded, skipped, bytes_downloaded })
}

/// Swap the verified staging directory into place, with rollback. Thin
/// wrapper over the shared transaction that serializes the pinned manifest as
/// the installation record (signature preserved for existing callers/tests).
pub fn promote_install(
    model_root: &Path,
    staging_dir: &Path,
    final_dir: &Path,
    manifest: &ComponentManifest,
) -> Result<(), AppError> {
    let json = serde_json::to_string_pretty(manifest)?;
    shared_promote_install(model_root, staging_dir, final_dir, &json)
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
    sweep_trash(runtime_root, "onnxruntime.trash-");

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
pub fn remove_components(model_roots: &[&Path], runtime_root: &Path) -> Result<(), AppError> {
    sweep_trash(runtime_root, "onnxruntime.trash-");
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
