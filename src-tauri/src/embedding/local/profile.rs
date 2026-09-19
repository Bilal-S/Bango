//! Bango Local embedding profile identity and request limits.
//!
//! The profile id encodes the model family, quantization, and prompt-strategy
//! revision. It is persisted as `article_embeddings.model_name` (and
//! `app_settings.embedding_model` after the local probe), so the director's
//! model-mismatch staleness treats any profile change - model swap, new
//! quantization, changed prefixes - as rows needing regeneration. No schema
//! change is required for embedding-profile versioning.

/// Profile identity: model family, quantization, and revision.
///
/// Bump `@rN` whenever the artifact revision or prompt strategy changes the
/// produced vectors.
pub const LOCAL_PROFILE_ID: &str = "builtin/embeddinggemma-300m-q4@r1";

/// On-disk profile directory name under the resolved model root
/// (`<model_root>/embeddinggemma-300m-q4/`).
///
/// Deliberately revision-less (path-safe, no `/` or `@`): the manifest inside
/// records the full [`LOCAL_PROFILE_ID`], and the component manager swaps
/// revisions atomically within the same directory.
pub const LOCAL_PROFILE_DIR: &str = "embeddinggemma-300m-q4";

/// Output dimensionality (EmbeddingGemma 300M; MRL truncation unused in v1).
pub const LOCAL_EMBEDDING_DIMENSIONS: usize = 768;

/// Model context window in tokens (EmbeddingGemma 300M). Bango's 512-word
/// chunks (~680 tokens) stay far below this cap, so the corpus inputs never
/// need splitting for the local backend.
pub const LOCAL_MAX_INPUT_TOKENS: usize = 2048;
