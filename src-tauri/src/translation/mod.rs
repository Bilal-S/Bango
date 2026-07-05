//! Plan-A permanent-rewrite translation of non-English articles to English.
//!
//! See `translation/AGENTS.md` for the binding contract. Two engine paths
//! exist: `translate_metadata_only` (title + abstract, no full text) and
//! `translate_full_text` (title + abstract + per-chunk full-text translation
//! with re-chunking of the English text).

pub mod engine;
pub mod language;
pub mod wait;
pub mod worker;

// Re-export the bus + waiter so callers don't need to reach into the submodule
// for the two pieces of public API they actually use.
pub use language::should_skip_translation;
pub use wait::{wait_for_article_translation, TranslationDoneBus};
