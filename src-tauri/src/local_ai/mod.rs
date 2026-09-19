//! Shared local-AI component manager (plan section 4).
//!
//! Generic artifact lifecycle shared by Bango Local embeddings and Bango AI:
//! path resolution with the OneDrive fallback, pinned-manifest primitives,
//! streamed download/verify/promote, archive extraction, and derived install
//! state. Embedding-specific and LLM-specific code stays in their own modules.

pub mod download;
pub mod manifest;
pub mod paths;
pub mod state;
