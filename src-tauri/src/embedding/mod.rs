//! Embedding pipeline for semantic article search.
//!
//! 1. Provider client (`llm::embedding`) - HTTP shapes, model resolution, capability probe.
//! 2. Runner + director (`embedding::director`, `embedding::runner`) - work-list computation
//!    and execution with correct lock discipline + orchestrator-bounded parallelism.
//! 3. Storage + recall (`db::embedding_repo`, `embedding::recall`) - CRUD + bounded cosine recall.
//! 4. Service + local backend (`embedding::service`, `embedding::local`) - the backend
//!    router (Configured Provider vs Bango Local) and the local backend's machine-local
//!    internals: artifact paths, thread budget, prompt profile, state probes, and the
//!    component manager (pinned manifest + atomic download/install/verify/remove).
//!    The inference engine itself (fastembed + ort session) lands with the T5 tier.

pub mod backend;
pub mod batching;
pub mod director;
pub mod local;
pub mod recall;
pub mod runner;
pub mod service;
pub mod text;

// Re-export the pure text helpers used flat by the integration tests
// (`tests/embedding/embedding_text_test.rs`); every other consumer uses the
// deep module paths.
pub use text::{
    cosine_similarity, expected_rows, format_embedding_text, hash_text, ChunkInput,
    TITLE_ABSTRACT_CHUNK_INDEX,
};
