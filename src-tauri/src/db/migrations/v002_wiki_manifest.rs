//! Wiki index manifest table.
//!
//! Stores per-file content hashes for every `wiki/**/*.md` page so the
//! on-demand `wiki_check_for_updates` command can detect external edits
//! (in-place body changes that leave the FTS5 row count unchanged) and
//! re-index transparently.
//!
//! This is a derived cache table (like `wiki_pages_fts`): it is dropped on
//! `rebuild_schema` / reset and repopulated from disk on the next check.

pub const VERSION: i32 = 2;

pub const UP_SQL: &str = "\
-- Wiki index manifest: per-file content hashes for drift detection.
-- One row per wiki/**/*.md page. The directory-level fingerprint lives in
-- app_settings under key 'wiki_dir_hash' (tier-1 fast path).
CREATE TABLE IF NOT EXISTS wiki_index_manifest (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL
);
";
