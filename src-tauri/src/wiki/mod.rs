//! LLM Wiki module.
//!
//! Generates and maintains a local-first Markdown knowledge base from the
//! project's `included` article corpus. See `.worktrees/llmwiki-plan.md` for
//! the authoritative design and `wiki-root/AGENTS.md` for the agent contract.
//!
//! Phase 1 (this module so far): storage resolution + scaffolding + the
//! `AGENTS.md` and template seed content. Ingest, lint, FTS5, and chat are
//! added in later phases.

pub mod agents_contract;
pub mod chat;
pub mod engine;
pub mod frontmatter;
pub mod fts;
pub mod ingest;
pub mod raw_export;
pub mod storage;
pub mod templates;
