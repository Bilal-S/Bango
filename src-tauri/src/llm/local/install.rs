//! Bango AI component install/verify/remove.
//!
//! Runtime-first transaction (plan section 5): the small llama.cpp runtime
//! bundle downloads and extracts first, the command layer runs an executable
//! smoke against it, and only then does the ~5.8 GB model download. Both
//! components are SHA-256 pinned and use the shared atomic
//! download/promote/assess primitives; install state stays derived, never
//! persisted.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::AppError;
use crate::llm::local::manifest::{BangoAiManifest, RuntimeArchive};
use crate::llm::local::profile::{
    server_binary_name, LOCAL_LLM_PROFILE_DIR, LOCAL_LLM_RUNTIME_DIR,
};
use crate::local_ai::download::{
    cleanup_ok, download_file, extract_archive_bundle, file_matches_pins, io_err, promote_install,
    sweep_trash, InstallProgress, InstallReport, VerificationFailure, STAGING_DIR_NAME,
};
use crate::local_ai::state::{assess_pinned_files, Assessment};

/// Trash prefix for locked Windows removes of the runtime tree.
pub const RUNTIME_TRASH_PREFIX: &str = "llama.cpp.trash-";

/// `{runtime_root}/llama.cpp/<version>/`.
#[must_use]
pub fn runtime_version_dir(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_root.join(LOCAL_LLM_RUNTIME_DIR).join(version)
}

/// The server executable path inside a version directory.
#[must_use]
pub fn server_path(runtime_root: &Path, version: &str) -> PathBuf {
    runtime_version_dir(runtime_root, version).join(server_binary_name())
}

/// File name of an archive-relative member path.
fn base_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path)
}

/// Expected on-disk (name, size) pairs for an extracted runtime bundle,
/// including alias copies at their target's size.
fn runtime_expected(archive: &RuntimeArchive) -> Vec<(&str, u64)> {
    let mut expected: Vec<(&str, u64)> =
        archive.members.iter().map(|m| (base_name(&m.path), m.size)).collect();
    for alias in &archive.aliases {
        let size = archive
            .members
            .iter()
            .find(|m| base_name(&m.path) == alias.target)
            .map_or(0, |m| m.size);
        expected.push((alias.path.as_str(), size));
    }
    expected
}

/// Whether the version manifest records this runtime's name/version/target.
fn runtime_manifest_ok(
    runtime_root: &Path,
    manifest: &BangoAiManifest,
    archive: &RuntimeArchive,
) -> bool {
    let path = runtime_version_dir(runtime_root, &manifest.runtime.version)
        .join(crate::local_ai::download::INSTALL_MANIFEST_NAME);
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&text).is_ok_and(|value| {
        value.get("name").and_then(|v| v.as_str()) == Some(manifest.runtime.name.as_str())
            && value.get("version").and_then(|v| v.as_str())
                == Some(manifest.runtime.version.as_str())
            && value.get("target").and_then(|v| v.as_str()) == Some(archive.target.as_str())
    })
}

/// Fast derived state of the runtime component.
#[must_use]
pub fn assess_runtime(runtime_root: &Path, manifest: &BangoAiManifest) -> Assessment {
    let Some(archive) = manifest.archive_for_current_target() else {
        return Assessment::NotInstalled;
    };
    let version_dir = runtime_version_dir(runtime_root, &manifest.runtime.version);
    let manifest_ok = runtime_manifest_ok(runtime_root, manifest, archive);
    assess_pinned_files(
        &version_dir,
        &runtime_root.join(STAGING_DIR_NAME),
        &runtime_expected(archive),
        manifest_ok,
    )
}

/// Fast derived state of the model component.
#[must_use]
pub fn assess_model(model_root: &Path, manifest: &BangoAiManifest) -> Assessment {
    let profile_dir = model_root.join(LOCAL_LLM_PROFILE_DIR);
    let manifest_ok = match std::fs::read_to_string(
        profile_dir.join(crate::local_ai::download::INSTALL_MANIFEST_NAME),
    ) {
        Ok(text) => serde_json::from_str::<BangoAiManifest>(&text)
            .is_ok_and(|installed| installed.profile == manifest.profile),
        Err(_) => false,
    };
    let expected: Vec<(&str, u64)> =
        manifest.files.iter().map(|f| (f.name.as_str(), f.size)).collect();
    assess_pinned_files(&profile_dir, &model_root.join(STAGING_DIR_NAME), &expected, manifest_ok)
}

/// Full verification of the runtime bundle (sizes; the archive SHA-256 pinned
/// content at download time).
pub fn verify_runtime(
    runtime_root: &Path,
    manifest: &BangoAiManifest,
) -> Result<Vec<VerificationFailure>, AppError> {
    let Some(archive) = manifest.archive_for_current_target() else {
        return Ok(vec![VerificationFailure {
            name: manifest.runtime.name.clone(),
            reason: "runtime is not available for this system".to_string(),
        }]);
    };
    let version_dir = runtime_version_dir(runtime_root, &manifest.runtime.version);
    let mut failures = Vec::new();
    if !runtime_manifest_ok(runtime_root, manifest, archive) {
        failures.push(VerificationFailure {
            name: crate::local_ai::download::INSTALL_MANIFEST_NAME.to_string(),
            reason: "runtime version manifest is missing or does not match".to_string(),
        });
    }
    for (name, size) in runtime_expected(archive) {
        if !file_matches_pins(&version_dir.join(name), size, None)? {
            failures.push(VerificationFailure {
                name: name.to_string(),
                reason: format!("missing or wrong size (expected {size} bytes)"),
            });
        }
    }
    Ok(failures)
}

/// Full verification of the installed model profile (sizes + SHA-256).
pub fn verify_model(
    model_root: &Path,
    manifest: &BangoAiManifest,
) -> Result<Vec<VerificationFailure>, AppError> {
    let profile_dir = model_root.join(LOCAL_LLM_PROFILE_DIR);
    let mut failures = Vec::new();
    match std::fs::read_to_string(
        profile_dir.join(crate::local_ai::download::INSTALL_MANIFEST_NAME),
    ) {
        Ok(text) => match serde_json::from_str::<BangoAiManifest>(&text) {
            Ok(installed) if installed.profile != manifest.profile => {
                failures.push(VerificationFailure {
                    name: crate::local_ai::download::INSTALL_MANIFEST_NAME.to_string(),
                    reason: format!(
                        "installed profile '{}' does not match the active profile '{}'",
                        installed.profile, manifest.profile
                    ),
                });
            }
            Ok(_) => {}
            Err(e) => failures.push(VerificationFailure {
                name: crate::local_ai::download::INSTALL_MANIFEST_NAME.to_string(),
                reason: format!("installation manifest is unparseable: {e}"),
            }),
        },
        Err(_) => failures.push(VerificationFailure {
            name: crate::local_ai::download::INSTALL_MANIFEST_NAME.to_string(),
            reason: "installation manifest is missing".to_string(),
        }),
    }
    for file in &manifest.files {
        if !file_matches_pins(&profile_dir.join(&file.name), file.size, file.sha256.as_deref())? {
            failures.push(VerificationFailure {
                name: file.name.clone(),
                reason: format!(
                    "missing, wrong size, or wrong hash (expected {} bytes)",
                    file.size
                ),
            });
        }
    }
    Ok(failures)
}

/// Friendly cancellation error: the install stops at the next phase boundary
/// and `.part` data stays resumable (aifixes1 F8).
fn cancelled() -> AppError {
    AppError::Validation("Bango AI setup was cancelled. You can resume later.".to_string())
}

/// Whether the caller requested cancellation.
#[must_use]
pub fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// Download + extract the pinned runtime bundle for the current target.
/// Idempotent: a healthy extracted bundle skips the download entirely.
pub async fn install_runtime_bundle(
    runtime_root: &Path,
    manifest: &BangoAiManifest,
    progress: &(dyn Fn(InstallProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<InstallReport, AppError> {
    let archive = manifest.archive_for_current_target().ok_or_else(|| {
        AppError::Validation(
            "Bango AI is not available for this system. You can continue using your \
             configured provider."
                .to_string(),
        )
    })?;
    let version_dir = runtime_version_dir(runtime_root, &manifest.runtime.version);
    sweep_trash(runtime_root, RUNTIME_TRASH_PREFIX);

    if assess_runtime(runtime_root, manifest) == Assessment::Ready {
        return Ok(InstallReport { downloaded: 0, skipped: 1, bytes_downloaded: 0 });
    }

    let staging_dir = runtime_root.join(STAGING_DIR_NAME);
    std::fs::create_dir_all(&staging_dir).map_err(io_err("runtime staging mkdir"))?;
    let archive_path = staging_dir.join(&archive.name);
    let total = archive.size;
    let fetched = download_file(
        &archive.url,
        &archive_path,
        archive.size,
        Some(&archive.sha256),
        &|file_bytes, file_total| {
            progress(InstallProgress {
                phase: "installing".to_string(),
                file: archive.name.clone(),
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
    let bytes_downloaded = if fetched { archive.size } else { 0 };

    progress(InstallProgress {
        phase: "installing".to_string(),
        file: archive.name.clone(),
        file_bytes: total,
        file_total: total,
        overall_bytes: total,
        overall_total: total,
        message: None,
    });
    std::fs::create_dir_all(&version_dir).map_err(io_err("runtime version mkdir"))?;
    let extract_dir = version_dir.clone();
    let members = archive.members.clone();
    let aliases = archive.aliases.clone();
    let archive_clone = archive_path.clone();
    // Cancellation before extraction (F8): the downloaded archive stays in
    // staging for a later resume.
    if is_cancelled(cancel) {
        return Err(cancelled());
    }
    tokio::task::spawn_blocking(move || {
        extract_archive_bundle(&archive_clone, &extract_dir, &members, &aliases)
    })
    .await
    .map_err(|e| AppError::Import(format!("runtime extract task panicked: {e}")))??;

    std::fs::write(
        version_dir.join(crate::local_ai::download::INSTALL_MANIFEST_NAME),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": manifest.runtime.name,
            "version": manifest.runtime.version,
            "target": archive.target,
        }))?,
    )
    .map_err(io_err("write runtime manifest"))?;
    let _ = std::fs::remove_file(&archive_path);
    let _ = std::fs::remove_dir_all(runtime_root.join(STAGING_DIR_NAME));

    Ok(InstallReport {
        downloaded: usize::from(fetched),
        skipped: usize::from(!fetched),
        bytes_downloaded,
    })
}

/// Download the pinned model profile (single GGUF). Idempotent: a healthy
/// install skips the network entirely.
pub async fn install_model_profile(
    model_root: &Path,
    manifest: &BangoAiManifest,
    progress: &(dyn Fn(InstallProgress) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<InstallReport, AppError> {
    if assess_model(model_root, manifest) == Assessment::Ready {
        return Ok(InstallReport {
            downloaded: 0,
            skipped: manifest.files.len(),
            bytes_downloaded: 0,
        });
    }
    let staging_dir = model_root.join(STAGING_DIR_NAME).join(LOCAL_LLM_PROFILE_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(io_err("model staging mkdir"))?;
    let total = manifest.files.iter().map(|f| f.size).sum::<u64>();
    let mut overall_done = 0u64;
    let mut downloaded = 0usize;
    let mut skipped = 0usize;
    let mut bytes_downloaded = 0u64;
    let mut verified_in_pass: Vec<String> = Vec::new();

    for file in &manifest.files {
        let dest = staging_dir.join(&file.name);
        let file_name = file.name.clone();
        let fetched = download_file(
            &file.url,
            &dest,
            file.size,
            file.sha256.as_deref(),
            &|file_bytes, file_total| {
                progress(InstallProgress {
                    phase: "downloading".to_string(),
                    file: file_name.clone(),
                    file_bytes,
                    file_total,
                    overall_bytes: overall_done + file_bytes.min(file_total),
                    overall_total: total,
                    message: None,
                });
            },
            cancel,
        )
        .await?;
        if fetched {
            downloaded += 1;
            bytes_downloaded += file.size;
            verified_in_pass.push(file.name.clone());
        } else {
            skipped += 1;
        }
        overall_done += file.size;
    }

    for file in &manifest.files {
        // Cancellation before each verify (F8): a cancelled install must never
        // report success.
        if is_cancelled(cancel) {
            return Err(cancelled());
        }
        // The downloader already SHA-256-verified files fetched this run
        // (in-pass hashing); only re-verify files that pre-existed (repair).
        if verified_in_pass.iter().any(|name| name == &file.name) {
            continue;
        }
        let path = staging_dir.join(&file.name);
        let expected_size = file.size;
        let pinned_hash = file.sha256.clone();
        progress(InstallProgress {
            phase: "verifying".to_string(),
            file: file.name.clone(),
            file_bytes: expected_size,
            file_total: expected_size,
            overall_bytes: overall_done,
            overall_total: total,
            message: None,
        });
        let ok = tokio::task::spawn_blocking(move || {
            file_matches_pins(&path, expected_size, pinned_hash.as_deref())
        })
        .await
        .map_err(|e| AppError::Import(format!("verification task panicked: {e}")))??;
        if !ok {
            return Err(AppError::Import(format!("verification failed for {}", file.name)));
        }
    }

    let final_dir = model_root.join(LOCAL_LLM_PROFILE_DIR);
    let promote_root = model_root.to_path_buf();
    let promote_staging = staging_dir;
    let promote_final = final_dir;
    let manifest_json = serde_json::to_string_pretty(manifest)?;
    // Cancellation before the atomic promote (F8): staging stays resumable.
    if is_cancelled(cancel) {
        return Err(cancelled());
    }
    tokio::task::spawn_blocking(move || {
        promote_install(&promote_root, &promote_staging, &promote_final, &manifest_json)
    })
    .await
    .map_err(|e| AppError::Import(format!("promote task panicked: {e}")))??;

    Ok(InstallReport { downloaded, skipped, bytes_downloaded })
}

/// Remove the model profile and runtime version directory. Idempotent; a
/// locked Windows runtime tree is renamed aside and swept on the next
/// install/remove.
pub fn remove_components(
    model_roots: &[&Path],
    runtime_root: &Path,
    version: &str,
) -> Result<(), AppError> {
    sweep_trash(runtime_root, RUNTIME_TRASH_PREFIX);
    for model_root in model_roots {
        std::fs::remove_dir_all(model_root.join(LOCAL_LLM_PROFILE_DIR))
            .or_else(cleanup_ok)
            .map_err(io_err("remove model"))?;
        std::fs::remove_dir_all(model_root.join(STAGING_DIR_NAME))
            .or_else(cleanup_ok)
            .map_err(io_err("remove model staging"))?;
        if let Ok(entries) = std::fs::read_dir(model_root) {
            if entries.filter_map(std::result::Result::ok).count() == 0 {
                let _ = std::fs::remove_dir(model_root);
            }
        }
    }
    let runtime_tree = runtime_root.join(LOCAL_LLM_RUNTIME_DIR);
    let version_dir = runtime_tree.join(version);
    if std::fs::remove_dir_all(&version_dir).is_err() && version_dir.exists() {
        let trash = runtime_root.join(format!(
            "{RUNTIME_TRASH_PREFIX}{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        ));
        let _ = std::fs::rename(&version_dir, &trash);
    }
    if std::fs::remove_dir(&runtime_tree).is_err() {
        // Non-empty (another version or trash) or absent: keep the tree.
    }
    std::fs::remove_dir_all(runtime_root.join(STAGING_DIR_NAME))
        .or_else(cleanup_ok)
        .map_err(io_err("remove runtime staging"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_manifest() -> BangoAiManifest {
        serde_json::from_str(crate::llm::local::manifest::BANGO_AI_MANIFEST_JSON)
            .expect("embedded manifest parses")
    }

    fn current_archive(manifest: &BangoAiManifest) -> RuntimeArchive {
        manifest.archive_for_current_target().expect("current target archive").clone()
    }

    #[test]
    fn runtime_assessment_reports_ready_after_bundle_layout() {
        let manifest = tiny_manifest();
        let archive = current_archive(&manifest);
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime_root = dir.path().join("runtimes");
        let version_dir = runtime_version_dir(&runtime_root, &manifest.runtime.version);
        std::fs::create_dir_all(&version_dir).expect("mkdir");
        std::fs::write(
            version_dir.join(crate::local_ai::download::INSTALL_MANIFEST_NAME),
            serde_json::json!({
                "name": manifest.runtime.name,
                "version": manifest.runtime.version,
                "target": archive.target,
            })
            .to_string(),
        )
        .expect("write version manifest");
        for member in &archive.members {
            std::fs::write(version_dir.join(base_name(&member.path)), b"x").expect("write member");
        }
        // Version manifest present but member sizes wrong: repair required.
        assert_eq!(assess_runtime(&runtime_root, &manifest), Assessment::RepairRequired);

        for member in &archive.members {
            std::fs::write(
                version_dir.join(base_name(&member.path)),
                vec![0u8; member.size as usize],
            )
            .expect("write sized member");
        }
        for alias in &archive.aliases {
            let size = archive
                .members
                .iter()
                .find(|m| base_name(&m.path) == alias.target)
                .map_or(0, |m| m.size);
            std::fs::write(version_dir.join(&alias.path), vec![0u8; size as usize])
                .expect("write alias");
        }
        assert_eq!(assess_runtime(&runtime_root, &manifest), Assessment::Ready);
        assert!(verify_runtime(&runtime_root, &manifest).expect("verify").is_empty());
    }

    #[test]
    fn model_assessment_requires_manifest_identity_and_exact_size() {
        let manifest = tiny_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let model_root = dir.path().join("model");
        assert_eq!(assess_model(&model_root, &manifest), Assessment::NotInstalled);

        let profile_dir = model_root.join(LOCAL_LLM_PROFILE_DIR);
        std::fs::create_dir_all(&profile_dir).expect("mkdir");
        let file = &manifest.files[0];
        std::fs::write(profile_dir.join(&file.name), b"short").expect("write model");
        std::fs::write(
            profile_dir.join(crate::local_ai::download::INSTALL_MANIFEST_NAME),
            serde_json::to_string(&manifest).expect("manifest json"),
        )
        .expect("write manifest");
        assert_eq!(assess_model(&model_root, &manifest), Assessment::RepairRequired);

        std::fs::write(profile_dir.join(&file.name), vec![0u8; file.size as usize])
            .expect("write sized model");
        assert_eq!(assess_model(&model_root, &manifest), Assessment::Ready);
    }
}
