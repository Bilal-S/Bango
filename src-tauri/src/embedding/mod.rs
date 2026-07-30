//! Embedding pipeline for semantic article search.
//!
//! Three layers (see `.worktrees/embed-plan.md`):
//!
//! 1. **Provider client** (`llm::embedding`) - per-provider HTTP shapes,
//!    model resolution, capability probe.
//! 2. **Runner + director** (`embedding::director`, `embedding::runner`) - the
//!    callable flow that computes the work list and executes it under correct
//!    lock discipline with orchestrator-bounded parallelism.
//! 3. **Storage + recall** (`db::embedding_repo`, `embedding::recall`) - CRUD
//!    for `article_embeddings` and the bounded cosine recall.

pub mod batching;
pub mod director;
pub mod recall;
pub mod runner;
pub mod text;

// Re-export the most-used helpers for ergonomic access from outside the crate.
pub use batching::group_into_embedding_batches;
pub use runner::{
    resolve_effective_dim, EmbeddingBatchSender, EmbeddingRunReport, HttpEmbeddingBatchSender,
};
pub use text::{
    cosine_similarity, expected_rows, format_embedding_text, hash_text, ChunkInput,
    TITLE_ABSTRACT_CHUNK_INDEX,
};
