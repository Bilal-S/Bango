//! Plan-A translation of non-English articles to English. Two paths:
//! `translate_metadata_only` (title + abstract) and `translate_full_text`
//! (title + abstract + per-chunk with re-chunking). See `translation/AGENTS.md`.
//!
//! Re-exports the bus + waiter so callers don't reach into submodules.

pub mod engine;
pub mod language;
pub mod wait;
pub mod worker;

pub use language::should_skip_translation;
pub use wait::{wait_for_article_translation, TranslationDoneBus};
