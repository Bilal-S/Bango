//! LLM Wiki module.
//!
//! Generates and maintains a local-first Markdown knowledge base from the
//! project's `included` article corpus.
//! See `wiki-root/AGENTS.md` for the agent contract.
//!

pub mod agents_contract;
pub mod chat;
pub mod engine;
pub mod frontmatter;
pub mod fts;
pub mod ingest;
pub mod raw_export;
pub mod storage;
pub mod templates;
