pub const VERSION: i32 = 5;

pub const UP_SQL: &str = "
-- Add sequence_id column for stable, user-facing article numbering
ALTER TABLE articles ADD COLUMN sequence_id INTEGER NOT NULL DEFAULT 0;

-- Backfill existing rows with sequential numbers based on import order
UPDATE articles SET sequence_id = (
    SELECT COUNT(*) FROM articles a2
    WHERE a2.rowid <= articles.rowid
);

-- Index for efficient ORDER BY and pagination
CREATE INDEX IF NOT EXISTS idx_articles_sequence_id ON articles(sequence_id);
";
