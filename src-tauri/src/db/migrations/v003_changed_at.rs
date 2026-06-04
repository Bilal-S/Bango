pub const VERSION: i32 = 3;

pub const UP_SQL: &str = r#"
-- Add changed_at column, initialised to imported_at for existing rows.
ALTER TABLE articles ADD COLUMN changed_at TEXT NOT NULL DEFAULT '';

-- Backfill: use imported_at as the initial "changed" value.
UPDATE articles SET changed_at = COALESCE(imported_at, datetime('now')) WHERE changed_at = '';

CREATE INDEX IF NOT EXISTS idx_articles_changed_at ON articles(changed_at);
"#;