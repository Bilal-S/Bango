//! Integration tests for the local embedding component manager
//! (`embedding::local::download` + `manifest`).
//!
//! All HTTP traffic targets a mockito server with tiny fixtures whose
//! SHA-256 pins are computed in-test (via the `sha2` dependency); no
//! real network or model download happens.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use bango_lib::embedding::local::download::{
    extract_archive_member, install_profile, install_runtime, remove_components, verify_installed,
    verify_runtime,
};
use bango_lib::embedding::local::manifest::{
    ComponentManifest, ManifestFile, RuntimeFile, RuntimeManifest,
};
use bango_lib::embedding::local::profile::LOCAL_PROFILE_DIR;
use bango_lib::embedding::local::state::{probe_installation_state, LocalEmbeddingState};
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn manifest_file(name: &str, url: String, body: &[u8]) -> ManifestFile {
    ManifestFile {
        name: name.to_string(),
        url,
        size: body.len() as u64,
        sha256: Some(sha256_hex(body)),
    }
}

/// A tar.gz archive containing `dir/lib/lib.so` (the pinned member) plus
/// `dir/lib/extra.txt` (must NOT be extracted).
fn tgz_with_lib(dir: &str, lib_body: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(lib_body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, format!("{dir}/lib/lib.so"), lib_body).unwrap();
    let mut extra = tar::Header::new_gnu();
    extra.set_size(2);
    extra.set_mode(0o644);
    extra.set_cksum();
    builder.append_data(&mut extra, format!("{dir}/lib/extra.txt"), &b"no"[..]).unwrap();
    builder.into_inner().unwrap().finish().unwrap()
}

/// A .zip archive with the same two members.
fn zip_with_lib(dir: &str, lib_body: &[u8]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut cursor);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    zip.start_file(format!("{dir}/lib/lib.so"), options).unwrap();
    zip.write_all(lib_body).unwrap();
    zip.start_file(format!("{dir}/lib/extra.txt"), options).unwrap();
    zip.write_all(b"no").unwrap();
    zip.finish().unwrap();
    cursor.into_inner()
}

/// Build the two-file test manifest against `base`.
fn manifest_for(base: &str, a: &[u8], b: &[u8]) -> ComponentManifest {
    ComponentManifest {
        profile: "builtin/embeddinggemma-300m-q4@r1".to_string(),
        model: "Test Model".to_string(),
        license: "Test License".to_string(),
        license_url: "https://example.invalid/terms".to_string(),
        source_revision: "0123456789abcdef".to_string(),
        files: vec![
            manifest_file("model_q4.onnx", format!("{base}/file-a"), a),
            manifest_file("tokenizer.json", format!("{base}/file-b"), b),
        ],
        runtime: None,
    }
}

/// A two-file test manifest backed by mockito endpoints serving `a` + `b`.
async fn mock_manifest(a: &[u8], b: &[u8]) -> (ComponentManifest, mockito::ServerGuard) {
    let mut server = mockito::Server::new_async().await;
    server.mock("GET", "/file-a").with_status(200).with_body(a.to_vec()).create_async().await;
    server.mock("GET", "/file-b").with_status(200).with_body(b.to_vec()).create_async().await;
    let manifest = manifest_for(&server.url(), a, b);
    (manifest, server)
}

#[tokio::test]
async fn component_install_downloads_and_verifies() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);

    let report =
        install_profile(&model_root, &manifest, &|_p| {}, &cancel).await.expect("install succeeds");

    assert_eq!(report.downloaded, 2);
    assert_eq!(report.skipped, 0);
    let final_dir = model_root.join(LOCAL_PROFILE_DIR);
    assert_eq!(std::fs::read(final_dir.join("model_q4.onnx")).unwrap(), b"graph-bytes");
    assert!(final_dir.join("manifest.json").is_file(), "installation manifest written");
    assert_eq!(probe_installation_state(&model_root), LocalEmbeddingState::Ready);
    assert!(verify_installed(&model_root, &manifest).unwrap().is_empty());
}

#[tokio::test]
async fn component_install_rejects_bad_hash() {
    let (mut manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    // Corrupt the pinned hash of the first file.
    manifest.files[0].sha256 = Some("0".repeat(64));
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);

    let err = install_profile(&model_root, &manifest, &|_p| {}, &cancel)
        .await
        .expect_err("hash mismatch must abort the install");

    assert!(err.to_string().contains("hash mismatch"), "got: {err}");
    assert_eq!(probe_installation_state(&model_root), LocalEmbeddingState::NotInstalled);
    assert!(!model_root.join(LOCAL_PROFILE_DIR).exists(), "no partial install");
}

#[tokio::test]
async fn component_install_is_idempotent_without_refetch() {
    // expect(1) mocks make refetching fail the test: the second install must
    // skip both pinned files without another HTTP hit.
    let mut server = mockito::Server::new_async().await;
    let mock_a = server
        .mock("GET", "/file-a")
        .with_status(200)
        .with_body(b"graph-bytes".to_vec())
        .expect(1)
        .create_async()
        .await;
    let mock_b = server
        .mock("GET", "/file-b")
        .with_status(200)
        .with_body(b"tokenizer-bytes".to_vec())
        .expect(1)
        .create_async()
        .await;
    let manifest = manifest_for(&server.url(), b"graph-bytes", b"tokenizer-bytes");
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);

    let first =
        install_profile(&model_root, &manifest, &|_p| {}, &cancel).await.expect("first install");
    let second = install_profile(&model_root, &manifest, &|_p| {}, &cancel)
        .await
        .expect("second install (repair)");

    assert_eq!((first.downloaded, first.skipped), (2, 0));
    assert_eq!((second.downloaded, second.skipped), (0, 2), "repair skips pinned files");
    mock_a.assert();
    mock_b.assert();
}

#[tokio::test]
async fn component_install_cancel_aborts() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);

    let err = install_profile(
        &model_root,
        &manifest,
        &|progress| {
            // Cancel as soon as the first progress event fires; the post-loop
            // check aborts even for single-chunk bodies.
            if progress.phase == "downloading" {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .await
    .expect_err("cancelled install must abort");

    assert!(err.to_string().contains("Cancelled"), "got: {err}");
    assert_eq!(probe_installation_state(&model_root), LocalEmbeddingState::NotInstalled);
}

#[tokio::test]
async fn component_verify_detects_corruption() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);
    install_profile(&model_root, &manifest, &|_p| {}, &cancel).await.expect("install");

    // Tamper with one installed file (same length, different bytes).
    std::fs::write(model_root.join(LOCAL_PROFILE_DIR).join("model_q4.onnx"), b"tampered!").unwrap();

    let failures = verify_installed(&model_root, &manifest).unwrap();
    assert!(
        failures.iter().any(|f| f.name == "model_q4.onnx"),
        "corruption must be reported: {failures:?}"
    );
}

#[tokio::test]
async fn component_verify_flags_profile_mismatch() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let cancel = AtomicBool::new(false);
    install_profile(&model_root, &manifest, &|_p| {}, &cancel).await.expect("install");

    // Rewrite the installed manifest with a different profile revision
    // (simulates an r1 install inspected against a future r2 pin).
    let mut stale = manifest.clone();
    stale.profile = "builtin/embeddinggemma-300m-q4@r2".to_string();
    std::fs::write(
        model_root.join(LOCAL_PROFILE_DIR).join("manifest.json"),
        serde_json::to_string(&stale).unwrap(),
    )
    .unwrap();

    let failures = verify_installed(&model_root, &manifest).unwrap();
    assert!(
        failures
            .iter()
            .any(|f| f.name == "manifest.json" && f.reason.contains("does not match the active")),
        "profile mismatch must be reported: {failures:?}"
    );
}

#[tokio::test]
async fn component_install_resumes_partial_download() {
    let body = b"0123456789abcdef"; // 16 bytes
    let mut server = mockito::Server::new_async().await;
    // Only the tail is served, matched on the exact Range header the resume
    // path must send for an 8-byte partial prefix.
    let tail = server
        .mock("GET", "/file-a")
        .match_header("Range", "bytes=8-")
        .with_status(206)
        .with_header("content-range", "bytes 8-15/16")
        .with_body(&body[8..])
        .expect(1)
        .create_async()
        .await;
    let head = server
        .mock("GET", "/file-b")
        .with_status(200)
        .with_body(b"tokenizer-bytes")
        .create_async()
        .await;
    let manifest = manifest_for(&server.url(), body, b"tokenizer-bytes");
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    // Seed the interrupted partial: 8 of 16 bytes of the model file.
    let staging = model_root.join(".staging").join(LOCAL_PROFILE_DIR);
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("model_q4.onnx.part"), &body[..8]).unwrap();
    let cancel = AtomicBool::new(false);

    install_profile(&model_root, &manifest, &|_p| {}, &cancel)
        .await
        .expect("resumed install succeeds");

    tail.assert();
    drop(head);
    assert_eq!(
        std::fs::read(model_root.join(LOCAL_PROFILE_DIR).join("model_q4.onnx")).unwrap(),
        body,
        "the prefix and the 206 tail combine into the pinned file"
    );
    assert!(verify_installed(&model_root, &manifest).unwrap().is_empty());
}

#[tokio::test]
async fn component_promote_restores_parked_install_on_failure() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let final_dir = model_root.join(LOCAL_PROFILE_DIR);
    let staging = model_root.join(".staging").join(LOCAL_PROFILE_DIR);

    // A working install exists; a fresh staging dir was prepared, then lost
    // (e.g. external cleanup) before the promote rename.
    std::fs::create_dir_all(&final_dir).unwrap();
    std::fs::write(final_dir.join("working"), b"old-install").unwrap();
    std::fs::create_dir_all(model_root.join(".staging")).unwrap();

    let err = bango_lib::embedding::local::download::promote_install(
        &model_root,
        &staging,
        &final_dir,
        &manifest,
    )
    .expect_err("promote must fail when staging is missing");

    assert!(err.to_string().contains("promote failed"), "got: {err}");
    assert!(final_dir.join("working").is_file(), "rollback restores the parked working install");
}

#[tokio::test]
async fn component_remove_deletes_artifacts() {
    let (manifest, _server) = mock_manifest(b"graph-bytes", b"tokenizer-bytes").await;
    let dir = tempfile::tempdir().unwrap();
    let model_root = dir.path().join("model");
    let runtime_root = dir.path().join("runtimes");
    let cancel = AtomicBool::new(false);
    install_profile(&model_root, &manifest, &|_p| {}, &cancel).await.expect("install");

    remove_components(&[&model_root], &runtime_root).expect("remove");

    assert_eq!(probe_installation_state(&model_root), LocalEmbeddingState::NotInstalled);
    assert!(!model_root.join(LOCAL_PROFILE_DIR).exists());
    // Idempotent: removing again succeeds.
    remove_components(&[&model_root], &runtime_root).expect("remove is idempotent");
    // Multi-root sweep: an orphaned install under an alternate root is gone too.
    let alternate = dir.path().join("appdata-models");
    std::fs::create_dir_all(alternate.join(LOCAL_PROFILE_DIR)).unwrap();
    std::fs::write(alternate.join(LOCAL_PROFILE_DIR).join("orphan"), b"x").unwrap();
    remove_components(&[&model_root, &alternate], &runtime_root).expect("multi-root remove");
    assert!(!alternate.join(LOCAL_PROFILE_DIR).exists());
}

// --- Runtime component (archive extraction + install) ----------------------

fn current_target_runtime_file(
    url: String,
    archive_body: &[u8],
    lib_path: &str,
    lib_size: u64,
) -> RuntimeFile {
    RuntimeFile {
        target: ComponentManifest::current_target()
            .expect("test machine target supported")
            .to_string(),
        name: "onnxruntime-test.tgz".to_string(),
        url,
        size: archive_body.len() as u64,
        sha256: sha256_hex(archive_body),
        lib_path: lib_path.to_string(),
        lib_size,
    }
}

/// A manifest whose runtime entry for the current target extracts
/// `onnxruntime-1.30.0/lib/lib.so` (content `lib_body`) from `archive_body`.
fn manifest_with_runtime(archive_body: &[u8], lib_body_len: u64, url: String) -> ComponentManifest {
    ComponentManifest {
        runtime: Some(RuntimeManifest {
            name: "onnxruntime".to_string(),
            version: "1.30.0".to_string(),
            files: vec![current_target_runtime_file(
                url,
                archive_body,
                "onnxruntime-1.30.0/lib/lib.so",
                lib_body_len,
            )],
        }),
        ..manifest_for("https://invalid.invalid", b"a", b"b")
    }
}

#[test]
fn runtime_extract_tgz_pulls_only_pinned_member() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("rt.tgz");
    let lib_body = b"dylib-bytes";
    std::fs::write(&archive, tgz_with_lib("onnxruntime-1.30.0", lib_body)).unwrap();
    let dest = dir.path().join("lib.so");

    extract_archive_member(&archive, "onnxruntime-1.30.0/lib/lib.so", &dest)
        .expect("tgz member extracted");

    assert_eq!(std::fs::read(&dest).unwrap(), lib_body);
    assert!(!dir.path().join("extra.txt").exists(), "non-pinned members stay unextracted");
}

#[test]
fn runtime_extract_zip_pulls_only_pinned_member() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("rt.zip");
    let lib_body = b"dll-bytes";
    std::fs::write(&archive, zip_with_lib("onnxruntime-1.30.0", lib_body)).unwrap();
    let dest = dir.path().join("onnxruntime.dll");

    extract_archive_member(&archive, "onnxruntime-1.30.0/lib/lib.so", &dest)
        .expect("zip member extracted");

    assert_eq!(std::fs::read(&dest).unwrap(), lib_body);
    assert!(!dir.path().join("extra.txt").exists(), "non-pinned members stay unextracted");
}

#[test]
fn runtime_extract_rejects_unsafe_member_paths() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("rt.tgz");
    std::fs::write(&archive, tgz_with_lib("onnxruntime-1.30.0", b"x")).unwrap();

    let err = extract_archive_member(&archive, "../escape.so", &dir.path().join("out"))
        .expect_err("traversal rejected");
    assert!(err.to_string().contains("unsafe"), "got: {err}");

    let out2 = dir.path().join("out2");
    let err = extract_archive_member(&archive, "onnxruntime-1.30.0/lib/missing.so", &out2)
        .expect_err("missing member rejected");
    assert!(err.to_string().contains("not found"), "got: {err}");
}

#[tokio::test]
async fn runtime_install_skips_when_library_exists() {
    // No mock server: an existing healthy library must short-circuit before
    // any HTTP (existence + pinned-size integrity check).
    let dir = tempfile::tempdir().unwrap();
    let runtime_root = dir.path().join("runtimes");
    let version_dir = runtime_root.join("onnxruntime").join("1.30.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    let existing = b"existing"; // exactly lib_size bytes below
    std::fs::write(version_dir.join("lib.so"), existing).unwrap();
    let manifest = manifest_with_runtime(
        b"never-downloaded",
        existing.len() as u64,
        "https://invalid.invalid/never-hit.tgz".to_string(),
    );
    let cancel = AtomicBool::new(false);

    let report = install_runtime(&runtime_root, &manifest, &|_p| {}, &cancel)
        .await
        .expect("idempotent skip");

    assert_eq!(report.downloaded, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(std::fs::read(version_dir.join("lib.so")).unwrap(), existing);
}

#[tokio::test]
async fn runtime_install_repairs_size_mismatch() {
    // A truncated/corrupt extracted library (wrong size) must NOT be skipped:
    // install re-downloads + re-extracts from the pinned archive.
    let archive_body = tgz_with_lib("onnxruntime-1.30.0", b"healthy-lib-bytes");
    let mut server = mockito::Server::new_async().await;
    let mock = server.mock("GET", "/rt.tgz").with_body(archive_body.clone()).create_async().await;
    let dir = tempfile::tempdir().unwrap();
    let runtime_root = dir.path().join("runtimes");
    let version_dir = runtime_root.join("onnxruntime").join("1.30.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    // Truncated library: right file, wrong size.
    std::fs::write(version_dir.join("lib.so"), b"trunc").unwrap();
    let manifest = manifest_with_runtime(
        &archive_body,
        b"healthy-lib-bytes".len() as u64,
        server.url().replace("/rt.tgz", "") + "/rt.tgz",
    );
    let cancel = AtomicBool::new(false);

    let report = install_runtime(&runtime_root, &manifest, &|_p| {}, &cancel)
        .await
        .expect("repair re-installs");

    assert_eq!(report.downloaded, 1, "the corrupt library is re-fetched");
    assert_eq!(std::fs::read(version_dir.join("lib.so")).unwrap(), b"healthy-lib-bytes");
    // Staging is cleaned after a successful install.
    assert!(!runtime_root.join(".staging").exists());
    mock.assert_async().await;
}

#[test]
fn runtime_extract_matches_dot_prefixed_members() {
    // The macOS archives store members with a leading `./`; the comparison
    // must normalize CurDir components or extraction fails on macOS.
    let dir = tempfile::tempdir().unwrap();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(4);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, "./onnxruntime-1.30.0/lib/lib.so", &b"lib!"[..]).unwrap();
    let archive = dir.path().join("rt.tgz");
    std::fs::write(&archive, builder.into_inner().unwrap().finish().unwrap()).unwrap();
    let dest = dir.path().join("lib.so");

    extract_archive_member(&archive, "onnxruntime-1.30.0/lib/lib.so", &dest)
        .expect("dot-prefixed member matches the pinned path");

    assert_eq!(std::fs::read(&dest).unwrap(), b"lib!");
}

#[test]
fn runtime_extract_zip_matches_dot_prefixed_members() {
    // Zip parity: entries stored with a leading `./` must resolve the same
    // way as the tar path (resolved via the immutable `file_names()` view).
    let dir = tempfile::tempdir().unwrap();
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut cursor);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    zip.start_file("./onnxruntime-1.30.0/lib/lib.so", options).unwrap();
    zip.write_all(b"lib!").unwrap();
    zip.finish().unwrap();
    let archive = dir.path().join("rt.zip");
    std::fs::write(&archive, cursor.into_inner()).unwrap();
    let dest = dir.path().join("lib.so");

    extract_archive_member(&archive, "onnxruntime-1.30.0/lib/lib.so", &dest)
        .expect("dot-prefixed zip member matches the pinned path");

    assert_eq!(std::fs::read(&dest).unwrap(), b"lib!");
}

#[test]
fn runtime_verify_reports_missing_and_corrupt_library() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_root = dir.path().join("runtimes");
    let archive_body = tgz_with_lib("onnxruntime-1.30.0", b"healthy-lib-bytes");
    let manifest = manifest_with_runtime(
        &archive_body,
        b"healthy-lib-bytes".len() as u64,
        "https://invalid.invalid/rt.tgz".to_string(),
    );

    let err = verify_runtime(&runtime_root, &manifest).expect_err("missing library reported");
    assert!(err.contains("missing"), "got: {err}");

    let version_dir = runtime_root.join("onnxruntime").join("1.30.0");
    std::fs::create_dir_all(&version_dir).unwrap();
    std::fs::write(version_dir.join("lib.so"), b"short").unwrap();
    let err = verify_runtime(&runtime_root, &manifest).expect_err("wrong size reported");
    assert!(err.contains("expected"), "got: {err}");

    std::fs::write(version_dir.join("lib.so"), b"healthy-lib-bytes").unwrap();
    verify_runtime(&runtime_root, &manifest).expect("healthy library verifies");
}

#[tokio::test]
async fn runtime_install_missing_target_archive_errors() {
    // A runtime pinned only for the CURRENT target is required; a manifest
    // carrying only foreign targets must produce the actionable error.
    let dir = tempfile::tempdir().unwrap();
    let manifest = ComponentManifest {
        runtime: Some(RuntimeManifest {
            name: "onnxruntime".to_string(),
            version: "1.30.0".to_string(),
            files: vec![RuntimeFile {
                // A target that is never the test machine's own (validated set).
                target: if ComponentManifest::current_target() == Some("win-x64") {
                    "linux-x64"
                } else {
                    "win-x64"
                }
                .to_string(),
                name: "foreign-archive.zip".to_string(),
                url: "https://invalid.invalid/foreign.zip".to_string(),
                size: 8,
                sha256: "0".repeat(64),
                lib_path: "onnxruntime/lib/onnxruntime.dll".to_string(),
                lib_size: 8,
            }],
        }),
        ..manifest_for("https://invalid.invalid", b"a", b"b")
    };
    let cancel = AtomicBool::new(false);

    let err = install_runtime(&dir.path().join("runtimes"), &manifest, &|_p| {}, &cancel)
        .await
        .expect_err("no archive for this target");

    assert!(err.to_string().contains("not available"), "got: {err}");
}
