//! Post-v007 schema fix (VERSION 8).
//!
//! Restores `idx_audit_entries_article_id`, the index originally created by
//! v001 but silently dropped by every subsequent `audit_entries` CHECK-
//! constraint rebuild (v003→v007). Each rebuild uses the
//! RENAME→CREATE→INSERT→DROP pattern, which removes the old table's non-auto
//! indexes; none of the rebuilds recreated this one.
//!
//! ## Why this matters
//!
//! The index backs two hot paths (`src-tauri/src/db/audit_repo.rs`):
//! 1. The Article Detail audit trail query
//!    (`WHERE article_id = ? ORDER BY timestamp DESC`) - without the index this
//!    is a full table scan of `audit_entries`, which grows unboundedly.
//! 2. The `ON DELETE CASCADE` on `audit_entries.article_id` - a hard article
//!    delete must find all matching audit rows; without the index that is
//!    another full scan.
//!
//! ## Idempotency
//!
//! `CREATE INDEX IF NOT EXISTS` is idempotent, and there is no
//! `ALTER TABLE ADD COLUMN`, so the `heal_partial_migrations` marker-probe
//! pattern (required for ADD COLUMN migrations like v003) is not needed here.
//!
//! ## Re-regression prevention
//!
//! Any future migration that rebuilds `audit_entries` (CHECK-constraint
//! expansion via RENAME→CREATE→INSERT→DROP) MUST recreate this index at the end
//! of its `UP_SQL`. Otherwise this migration will restore it for current DBs,
//! only for the next rebuild to drop it again.

pub const VERSION: i32 = 8;

pub const UP_SQL: &str = "\
-- Restore idx_audit_entries_article_id, the v001 index dropped by every
-- audit_entries CHECK-rebuild (v003→v007 RENAME→CREATE→INSERT→DROP).
-- Backs the Article Detail audit-trail query and the ON DELETE CASCADE.
CREATE INDEX IF NOT EXISTS idx_audit_entries_article_id
    ON audit_entries(article_id);
";
