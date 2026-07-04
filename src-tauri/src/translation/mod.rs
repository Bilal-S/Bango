//! Plan-A permanent-rewrite translation of non-English articles to English.
//!
//! See `translation/AGENTS.md` for the binding contract. Two engine paths
//! exist: `translate_metadata_only` (title + abstract, no full text) and
//! `translate_full_text` (title + abstract + per-chunk full-text translation
//! with re-chunking of the English text).

pub mod engine;
pub mod language;
pub mod worker;
