pub const VERSION: i32 = 2;

pub const UP_SQL: &str = r#"
ALTER TABLE tags ADD COLUMN color TEXT;
ALTER TABLE labels ADD COLUMN color TEXT;
"#;
