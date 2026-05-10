pub const VERSION: i32 = 6;

pub const UP_SQL: &str = r#"
ALTER TABLE llm_config ADD COLUMN skip_temperature INTEGER NOT NULL DEFAULT 0;
"#;
