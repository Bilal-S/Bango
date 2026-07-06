//! Post-v002 schema additions: chunk-aware FTS5, article chunk storage,
//! audit_entries expansion, and the translation persistence layer.
//!
//! This migration carries two bundles so the migration sequence stays
//! gap-free (`v001` -> `v002` -> `v003`):
//!
//! 1. The reverted v002 content (because v002 shipped with only
//!    `wiki_index_manifest`):
//!    - `DROP TABLE IF EXISTS wiki_pages_fts;` - the FTS5 virtual table is a
//!      derived cache rebuilt from disk by `rebuild_index_with_manifest`.
//!      Dropping it is safe and lets `fts::ensure_table` recreate it with the
//!      chunk-aware column set on the next read. FTS5 virtual tables cannot be
//!      `ALTER`ed, so the explicit `DROP` is the supported way to change the
//!      column set. Self-heals via `ensure_index_populated` /
//!      `rebuild_index_with_manifest`.
//!    - `CREATE TABLE article_chunks` - per-article chunk storage populated at
//!      attach time by `extract_sections` + `chunk_sections`. Consumed by
//!      screening and reusable by any per-article retrieval.
//!    - Rebuild `audit_entries` to add both `figure_descriptions` and
//!      `ai_screen_enhanced` to the `action` CHECK constraint.
//! 2. The translation additions:
//!    - Four `articles` columns (`is_translated`, `translation_status`,
//!      `translation_error`, `translated_at`) that record the in-memory
//!      translation-queue progress.
//!    - `article_original_content` + `article_original_chunks` tables that
//!      preserve the original-language text/chunks before Plan-A translation
//!      rewrites the working `articles`/`article_chunks` rows.
//!    - A second `audit_entries` rebuild extending the action CHECK with
//!      `'translation'` and `'translation_error'`.
//!
//! `articles.language` remains the sole original-language source. It is set at
//! import time and never overwritten by translation. `is_translated = 1` with
//! `language = 'French'` means "originally French, now translated to English;
//! originals in `article_original_content`". No `original_language` or
//! `detected_language` columns are added.
//!
//! ## Crash-recovery contract
//!
//! The migration's `ALTER TABLE articles ADD COLUMN is_translated` (and the
//! three sibling columns) have no `IF NOT EXISTS` guard - SQLite does not
//! support that syntax for `ADD COLUMN`. The migration runner
//! (`db::migration::run_migrations`) wraps this migration in a transaction so
//! a crash between the DDL and the `user_version` bump rolls back cleanly.
//! Additionally, `db::migration::heal_partial_migrations` detects databases
//! corrupted by older non-transactional builds (where the DDL committed but
//! `user_version` stayed at 2) by probing for `articles.is_translated` and,
//! if present, advances `user_version` to 3 without re-running these ALTERs.
//! If you add another `ALTER TABLE ... ADD COLUMN` migration in the future,
//! extend `heal_partial_migrations` with a marker-column check for it.

pub const VERSION: i32 = 3;

pub const UP_SQL: &str = "\
-- 1. Drop the lazily-created FTS5 virtual table so fts::ensure_table recreates it
--    with the chunk-aware column set (chunk_index, section, parent_slug) on the
--    next read. The table is a derived cache rebuilt from disk; dropping it is
--    safe and self-heals via ensure_index_populated / rebuild_index_with_manifest.
DROP TABLE IF EXISTS wiki_pages_fts;

-- 2. Article-level chunk storage. Populated at attach time by
--    extract_sections + chunk_sections. Consumed by screening and reusable by
--    any per-article retrieval.
CREATE TABLE IF NOT EXISTS article_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    section TEXT,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    UNIQUE(article_id, chunk_index)
);
CREATE INDEX IF NOT EXISTS idx_article_chunks_article ON article_chunks(article_id);

-- 3. Rebuild audit_entries to add 'figure_descriptions' and 'ai_screen_enhanced'
--    to the action CHECK constraint. SQLite CHECK constraints cannot be
--    ALTERed; use the rename-create-copy-drop pattern.
ALTER TABLE audit_entries RENAME TO audit_entries_v003a_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'ai_screen_enhanced', 'manual_override',
        'ai_summary', 'error', 'dedup_auto', 'reference_import',
        'reference_match', 'figure_descriptions'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT
);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v003a_old;

DROP TABLE audit_entries_v003a_old;

-- 4. Translation status columns on articles. The existing `articles.language`
--    column records the original language and is immutable; `is_translated`
--    records whether the working text has been rewritten to English.
ALTER TABLE articles ADD COLUMN is_translated INTEGER NOT NULL DEFAULT 0;
ALTER TABLE articles ADD COLUMN translation_status TEXT NOT NULL DEFAULT 'none'
    CHECK(translation_status IN ('none','queued','running','succeeded','failed'));
ALTER TABLE articles ADD COLUMN translation_error TEXT;
ALTER TABLE articles ADD COLUMN translated_at TEXT;

-- 5. Original-language content archive. Populated once at translation time
--    (before the working articles row is rewritten). `source_language` captures
--    the `articles.language` value at translation time.
CREATE TABLE IF NOT EXISTS article_original_content (
    article_id TEXT PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    original_title TEXT,
    original_abstract_text TEXT,
    original_full_text TEXT,
    source_language TEXT,
    stored_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 6. Original-language chunk archive. Holds the pre-translation chunk
--    coordinate space; after translation the re-chunked English content lives
--    in `article_chunks` with its own indices. The two spaces must not be
--    compared or joined directly.
CREATE TABLE IF NOT EXISTS article_original_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    article_id TEXT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    section TEXT,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    UNIQUE(article_id, chunk_index)
);
CREATE INDEX IF NOT EXISTS idx_article_original_chunks_article
    ON article_original_chunks(article_id);

-- 7. Second audit_entries rebuild: extend the action CHECK with 'translation'
--    and 'translation_error'. Same rename-create-copy-drop pattern as step 3.
ALTER TABLE audit_entries RENAME TO audit_entries_v003b_old;

CREATE TABLE audit_entries (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'ai_screen_enhanced', 'manual_override',
        'ai_summary', 'error', 'dedup_auto', 'reference_import',
        'reference_match', 'figure_descriptions',
        'translation', 'translation_error'
    )),
    article_id TEXT,
    details TEXT,
    from_status TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    to_status TEXT,
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

INSERT INTO audit_entries (id, action, article_id, details, from_status, source, timestamp, to_status)
SELECT id, action, article_id, details, from_status, source, timestamp, to_status
FROM audit_entries_v003b_old;

DROP TABLE audit_entries_v003b_old;
";
