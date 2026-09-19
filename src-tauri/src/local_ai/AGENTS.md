# local_ai/

## Purpose

Shared local-AI artifact lifecycle used by Bango Local embeddings
(`embedding::local`) and Bango AI (`llm::local`): artifact path resolution
with the OneDrive fallback, pinned-manifest primitives, streamed
download/verify/promote, archive extraction, removal, and derived install
state assessment.

## Ownership

- `paths.rs` - implemented (moved from `embedding::local::paths` in T2 slice 1;
  behavior byte-identical, one shared implementation): `AiPaths`,
  `is_onedrive_path`, `resolve_ai_paths`, `resolve_ai_paths_with_base`,
  `MODEL_DIR_NAME`, `AI_DIR_NAME`. Debug builds honor the test-only
  `BANGO_TEST_DATA_LOCAL_DIR` knob so integration tests can redirect runtime
  artifacts; release builds ignore it.
- `manifest.rs` - implemented (T2 slice 2): `PinnedFile`, `ArchiveMember`,
  `url_uses_allowed_scheme`, `is_valid_sha256_hex`, `is_path_safe_file_name`,
  `is_archive_safe_path`, `required_disk_bytes`, `target_id`,
  `current_target`. Embedding re-exports `PinnedFile as ManifestFile` and
  `target_id`, so its public API is unchanged; its `ComponentManifest` /
  `RuntimeFile` shapes and profile enforcement stay in
  `embedding::local::manifest`.
- `download.rs` - implemented (T2 slice 3): `STAGING_DIR_NAME`,
  `INSTALL_MANIFEST_NAME`, `READ_STALL_TIMEOUT`, `InstallProgress`,
  `InstallReport`, `VerificationFailure`, `supported_target`,
  `target_supported`, `available_bytes`, `download_file` (staging + resume +
  in-pass SHA-256), `file_matches_pins`, the generic `promote_install`
  (install-manifest JSON), `extract_archive_member` (traversal-safe,
  `./`-normalizing, regular-file-only), `sweep_trash`, `cleanup_ok`, plus the
  moved `target_supported_matrix` + `part_extension_appends_to_file_name`
  tests. Embedding re-exports the shared names and keeps thin wrappers where
  its signatures differ (`promote_install` serializes `ComponentManifest`).
- `state.rs` - implemented (T2 slice 4): `Assessment` (`NotInstalled` |
  `RepairRequired` | `Ready`, with `as_str` / `not_ready_phrase`) and
  `assess_pinned_files(final_dir, staging_root, expected, manifest_ok)` plus
  the stranded `.staging/replaced-*` park detection. Embedding re-exports
  `Assessment` as `LocalEmbeddingState` and keeps `probe_installation_state`
  plus the profile-identity check in `embedding/local/state.rs`.
- Consumers today: `embedding::local::engine`, `commands/local_embeddings.rs`,
  `citation_finder::readiness.rs`. Bango AI (`llm::local`,
  `commands/bango_ai.rs`) joins at T5.

## Local Contracts

### OneDrive fallback (moved from embedding)

- Model payloads live under `{storage_root}/model` unless a whole path segment
  is `OneDrive` or `OneDrive - <tenant>` (case-insensitive) and an app-data
  base exists; then the root falls back to `{data_local}/Bango/ai/models` with
  `used_fallback = true`.
- Runtime libraries always derive from `{data_local}/Bango/ai/runtimes` when
  an app-data base exists; a missing base degrades to
  `{storage_root}/ai/runtimes` without claiming a fallback (never a panic).
- Paths re-resolve from the current storage root on every load; they are never
  persisted.

### Pinned-pin policy (shared)

- URLs are HTTPS except loopback HTTP at a host boundary (`127.0.0.1`,
  `localhost`), which exist for test mocks and local mirrors.
- SHA-256 pins are exactly 64 lowercase hex characters.
- Destination file names are flat and path-safe; archive member paths are
  traversal-safe (no absolute paths, `..`, or backslashes).
- Disk math: `total x (1|2) + max(64 MiB, 10%)`, where the second copy covers
  repair/update staging.
- Targets: `win-x64`, `osx-arm64`, `linux-x64`; macOS x86_64 is absent by
  design (no runtime builds meet the pinned floors).

### Download transaction (shared)

- `download_file` stages `<dest>.part`, resumes with `Range` (prefix hashed
  first), restarts cleanly on a 200 response, verifies size + SHA-256 in-pass,
  renames only on success, and bounds every chunk read by `READ_STALL_TIMEOUT`.
- `promote_install` parks an existing install under `.staging/replaced-*`,
  renames staging into place, writes the caller's install-manifest JSON, and
  restores the parked copy when the promote rename fails.
- `extract_archive_member` accepts zip/tar.gz, rejects traversal and
  non-regular members, normalizes `./` prefixes, and chmods 0755 on unix.
- `sweep_trash(root, prefix)` retries deletion of trash trees renamed aside by
  locked Windows removal paths.
- `extract_archive_bundle(archive, dest, members, aliases)` extracts a pinned
  member SET (flattening to file names, verifying pinned sizes, rejecting
  traversal/duplicates/non-regular entries) and materializes alias copies with
  chain resolution and cycle rejection.

### Derived assessment (shared)

- `Ready` requires the caller's installation-manifest identity check AND every
  expected file at its exact pinned size; partial presence reports
  `RepairRequired`; nothing at all reports `NotInstalled`.
- A stranded `.staging/replaced-*` park from a crashed promote reports
  `RepairRequired` even when the final directory is gone.

## Work Guidance

- Keep `paths.rs` pure (no filesystem access); filesystem work belongs to the
  download/state tiers.
- Extract pure helpers so unit tests stay platform-independent (the app-data
  base is a parameter).

## Verification

- `cargo test --lib local_ai` - paths inline tests (8: OneDrive detection,
  resolution, fallback, degradation), shared manifest primitives (1:
  hash/name/archive/scheme/target/disk-math validation), shared download
  primitives (2: target matrix + `.part` suffix), shared assessment (3:
  ready, stranded park, not installed).
- Embedding parity after the moves: `cargo test --test embedding` (141 passed),
  embedding manifest inline tests (9), plus `npm run check:all`.

## Child DOX Index

No child `AGENTS.md` files.
