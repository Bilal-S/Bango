//! Bango Local embeddings: on-device backend internals.
//!
//! Owns the machine-local concerns of the local backend: artifact path
//! resolution (with the OneDrive Documents fallback), the CPU thread budget,
//! the EmbeddingGemma query/document prompt profile, the profile identity
//! constants, the installation state probes, the component manager (pinned
//! manifest + atomic download/install/verify/remove), and the inference
//! engine (a lazily-loaded fastembed session executing on the blocking
//! pool).

pub mod download;
pub mod engine;
pub mod manifest;
pub mod paths;
pub mod profile;
pub mod prompt;
pub mod state;
pub mod thread_budget;

pub use download::{
    available_bytes, extract_archive_member, install_profile, install_runtime, promote_install,
    remove_components, runtime_lib_file, runtime_library_healthy, supported_target,
    target_supported, verify_installed, verify_runtime, InstallProgress, InstallReport,
    VerificationFailure,
};
pub use engine::{EnginePaths, LocalEngine};
pub use manifest::{
    local_manifest, parse_manifest, ComponentManifest, ManifestFile, RuntimeFile, RuntimeManifest,
};

pub use paths::{is_onedrive_path, resolve_ai_paths, resolve_ai_paths_with_base, AiPaths};
pub use profile::{
    LOCAL_EMBEDDING_DIMENSIONS, LOCAL_MAX_INPUT_TOKENS, LOCAL_PROFILE_DIR, LOCAL_PROFILE_ID,
};
pub use prompt::{apply_role_prefix, EmbeddingRole, DOCUMENT_PREFIX, QUERY_PREFIX};
pub use state::{
    assess_installation, probe_installation_state, LocalEmbeddingState, INSTALL_MANIFEST_NAME,
};
pub use thread_budget::embedding_thread_budget;
