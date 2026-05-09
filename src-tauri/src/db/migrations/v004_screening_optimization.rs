pub const VERSION: i32 = 4;

pub const UP_SQL: &str = r#"
-- Add columns for performance optimization and token tracking
ALTER TABLE articles ADD COLUMN data_length INTEGER;
ALTER TABLE articles ADD COLUMN token_estimate INTEGER;
ALTER TABLE articles ADD COLUMN actual_tokens INTEGER;

-- Index for fast O(log N) maximum length queries
CREATE INDEX IF NOT EXISTS idx_articles_data_length ON articles(data_length);
"#;
