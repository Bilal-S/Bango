//! Bango AI profile constants: identity, model file, runtime directory, and
//! the platform server binary name.

/// Profile identity (the on-disk installation manifest and readiness probes
/// compare against this; mirrors the embedding profile-id convention).
/* Previous pinned model (Ornith-1.5-9B Q4_K_M), kept for revert:
`builtin/ornith-1.5-9b-q4km@r1`, dir `bango-ai-ornith-1.5-9b-q4km`,
file `Ornith-1.5-9B-Q4_K_M.gguf`. */
pub const LOCAL_LLM_PROFILE_ID: &str = "builtin/qwen3.5-2b-ud-q4kxl@r1";

/// On-disk model profile directory under `{storage_root}/model/`.
pub const LOCAL_LLM_PROFILE_DIR: &str = "bango-ai-qwen3.5-2b";

/// Pinned model file name inside the profile directory.
pub const LOCAL_LLM_MODEL_FILE: &str = "Qwen3.5-2B-UD-Q4_K_XL.gguf";

/// Runtime directory name under `{data_local}/Bango/ai/runtimes/`.
pub const LOCAL_LLM_RUNTIME_DIR: &str = "llama.cpp";

/// Human-facing engine label for Component Details and diagnostics.
pub const LOCAL_LLM_ENGINE_LABEL: &str = "llama.cpp";

/// The server executable name for this platform.
#[must_use]
pub fn server_binary_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}
