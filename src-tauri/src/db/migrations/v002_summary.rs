pub const VERSION: i32 = 2;

pub const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS summary (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    summary_text TEXT NOT NULL,
    citation_style TEXT NOT NULL DEFAULT 'APA',
    generated_at TEXT NOT NULL
);
"#;
