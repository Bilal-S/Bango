pub const VERSION: i32 = 4;

pub const UP_SQL: &str = r#"
-- Full-text extraction and AI summary of article content.
-- Both nullable: populated on demand (not during import).
ALTER TABLE articles ADD COLUMN full_text TEXT;
ALTER TABLE articles ADD COLUMN full_text_ai_summary TEXT;
"#;