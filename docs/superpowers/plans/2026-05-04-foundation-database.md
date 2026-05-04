# Foundation & Database Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up SQLite database with full schema, Tauri command infrastructure, Vue Router, Pinia stores, and the application shell layout so all subsequent plans have a working foundation to build on.

**Architecture:** Tauri 2.x app with SQLite via `rusqlite`. Rust backend exposes typed Tauri commands. Vue 3 frontend uses Vue Router for navigation and Pinia for state management. The app shell provides a fixed sidebar + fluid main content area.

**Tech Stack:** Tauri 2.x, Rust (rusqlite, serde, uuid), Vue 3, Vue Router, Pinia, TypeScript, Vite

**Depends on:** Nothing (this is the first plan)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/
├── Cargo.toml                    (modify: add dependencies)
├── src/
│   ├── main.rs                   (no change)
│   ├── lib.rs                    (modify: register commands, init DB)
│   ├── db/
│   │   ├── mod.rs                (new: module declarations)
│   │   ├── connection.rs         (new: SQLite connection management)
│   │   ├── migration.rs          (new: migration runner)
│   │   └── migrations/
│   │       ├── mod.rs            (new: migration registry)
│   │       └── v001_initial.rs   (new: initial schema)
│   ├── models/
│   │   ├── mod.rs                (new: module declarations)
│   │   ├── article.rs            (new: Article struct + status enum)
│   │   ├── criterion.rs          (new: Criterion + ResearchAim structs)
│   │   ├── tag.rs                (new: Tag struct)
│   │   ├── label.rs              (new: Label struct)
│   │   ├── audit.rs              (new: AuditEntry struct)
│   │   └── llm_config.rs         (new: LLMConfig struct)
│   ├── commands/
│   │   └── mod.rs                (new: Tauri command stubs)
│   └── error.rs                  (new: app error types)
├── tests/
│   └── db_test.rs                (new: database integration tests)
```

### TypeScript/Vue (src/)

```
src/
├── main.ts                       (modify: add router + pinia)
├── App.vue                       (modify: app shell layout)
├── router/
│   └── index.ts                  (new: route definitions)
├── stores/
│   ├── articles.ts               (new: article store)
│   ├── criteria.ts               (new: criteria store)
│   ├── tags.ts                   (new: tags store)
│   └── labels.ts                 (new: labels store)
├── types/
│   └── index.ts                  (new: TypeScript interfaces mirroring Rust models)
├── views/
│   ├── dashboard.vue             (new: project dashboard)
│   └── placeholder.vue           (new: placeholder for unbuilt views)
├── components/
│   ├── app-shell.vue             (new: sidebar + main layout)
│   └── nav-sidebar.vue           (new: navigation sidebar)
├── styles/
│   ├── tokens.css                (new: CSS custom properties from DESIGN.md)
│   └── base.css                  (new: reset + base styles)
├── composables/
│   └── use-tauri-command.ts      (new: typed Tauri invoke wrapper)
└── utils/
    └── formatters.ts             (new: display formatters)
```

---

## Task 1: Rust Dependencies & Error Types

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/error.rs`

- [ ] **Step 1: Add dependencies to Cargo.toml**

In `src-tauri/Cargo.toml`, add these to `[dependencies]`:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
tauri-plugin-sql = { version = "2", features = ["sqlite"] }
```

- [ ] **Step 2: Run `cargo check` to verify dependencies resolve**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors (warnings OK)

- [ ] **Step 3: Create `src-tauri/src/error.rs`**

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Import error: {0}")]
    Import(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<AppError> for tauri::ipc::InvokeError {
    fn from(error: AppError) -> Self {
        tauri::ipc::InvokeError::from(error.to_string())
    }
}
```

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/error.rs
git commit -m "feat(core): add dependencies and error types"
```

---

## Task 2: Database Connection & Migration Infrastructure

**Files:**
- Create: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/db/connection.rs`
- Create: `src-tauri/src/db/migration.rs`
- Create: `src-tauri/src/db/migrations/mod.rs`
- Create: `src-tauri/src/db/migrations/v001_initial.rs`
- Test: `src-tauri/tests/db_test.rs`

- [ ] **Step 1: Write failing database test**

Create `src-tauri/tests/db_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;

#[test]
fn test_database_initializes_with_all_tables() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("Failed to run migrations");

    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("Failed to prepare query")
        .query_map([], |row| row.get(0))
        .expect("Failed to query")
        .filter_map(|r| r.ok())
        .collect();

    assert!(tables.contains(&"articles".to_string()), "Missing articles table");
    assert!(tables.contains(&"criteria".to_string()), "Missing criteria table");
    assert!(tables.contains(&"research_aims".to_string()), "Missing research_aims table");
    assert!(tables.contains(&"tags".to_string()), "Missing tags table");
    assert!(tables.contains(&"labels".to_string()), "Missing labels table");
    assert!(tables.contains(&"audit_entries".to_string()), "Missing audit_entries table");
    assert!(tables.contains(&"llm_config".to_string()), "Missing llm_config table");
    assert!(tables.contains(&"article_tags".to_string()), "Missing article_tags table");
    assert!(tables.contains(&"article_labels".to_string()), "Missing article_labels table");
}

#[test]
fn test_migrations_are_idempotent() {
    let conn = create_connection().expect("Failed to create connection");
    run_migrations(&conn).expect("First migration run failed");
    run_migrations(&conn).expect("Second migration run should succeed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test db_test --test db_test`
Expected: FAIL — modules don't exist yet

- [ ] **Step 3: Create `src-tauri/src/db/mod.rs`**

```rust
pub mod connection;
pub mod migration;
pub mod migrations;
```

- [ ] **Step 4: Create `src-tauri/src/db/connection.rs`**

```rust
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

use crate::error::AppError;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

pub fn create_connection() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

pub fn create_connection_at(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}
```

- [ ] **Step 5: Create `src-tauri/src/db/migration.rs`**

```rust
use rusqlite::Connection;

use crate::error::AppError;
use super::migrations;

pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    let current_version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0);

    let migrations = migrations::get_migrations();

    for migration in migrations {
        if migration.version > current_version {
            conn.execute_batch(&migration.up_sql)?;
            conn.pragma_update(None, "user_version", migration.version)?;
        }
    }

    Ok(())
}
```

- [ ] **Step 6: Create `src-tauri/src/db/migrations/v001_initial.rs`**

```rust
pub const VERSION: i32 = 1;

pub const UP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS research_aims (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS criteria (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK(type IN ('inclusion', 'exclusion')),
    text TEXT NOT NULL,
    priority TEXT NOT NULL CHECK(priority IN ('critical', 'high', 'standard', 'low', 'optional')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_suggested', 'user_created', 'ris_keyword'))
);

CREATE TABLE IF NOT EXISTS labels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK(source IN ('ai_generated', 'user_created'))
);

CREATE TABLE IF NOT EXISTS articles (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'imported' CHECK(status IN ('imported', 'working', 'included', 'rejected')),
    screening_error INTEGER NOT NULL DEFAULT 0,
    title TEXT NOT NULL,
    abstract_text TEXT NOT NULL,
    authors TEXT NOT NULL,
    publication_year INTEGER,
    doi TEXT,
    journal TEXT,
    volume TEXT,
    issue TEXT,
    start_page TEXT,
    end_page TEXT,
    keywords TEXT,
    url TEXT,
    language TEXT,
    publisher TEXT,
    publisher_city TEXT,
    publisher_address TEXT,
    issn TEXT,
    reference_type TEXT,
    date TEXT,
    author_address TEXT,
    accession_number TEXT,
    custom_field3 TEXT,
    journal_abbreviation TEXT,
    journal_iso_abbreviation TEXT,
    notes TEXT,
    web_of_science_db TEXT,
    user_notes TEXT,
    ris_extras TEXT,
    duplicate_of TEXT,
    ai_decision TEXT CHECK(ai_decision IS NULL OR ai_decision IN ('include', 'exclude')),
    ai_reasoning TEXT,
    ai_confidence REAL,
    matched_inclusion_criteria TEXT,
    matched_exclusion_criteria TEXT,
    manual_override INTEGER NOT NULL DEFAULT 0,
    import_source TEXT,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    screened_at TEXT,
    FOREIGN KEY (duplicate_of) REFERENCES articles(id)
);

CREATE TABLE IF NOT EXISTS article_tags (
    article_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (article_id, tag_id),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS article_labels (
    article_id TEXT NOT NULL,
    label_id TEXT NOT NULL,
    PRIMARY KEY (article_id, label_id),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE,
    FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS audit_entries (
    id TEXT PRIMARY KEY,
    article_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    action TEXT NOT NULL CHECK(action IN (
        'import', 'dedup_merge', 'dedup_flag', 'status_change',
        'tag_add', 'tag_remove', 'label_add', 'label_remove',
        'criteria_match', 'ai_screen', 'manual_override', 'ai_summary'
    )),
    from_status TEXT,
    to_status TEXT,
    details TEXT,
    source TEXT NOT NULL CHECK(source IN ('ai', 'user', 'system')),
    FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS llm_config (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    provider TEXT NOT NULL CHECK(provider IN ('openai', 'google', 'z_ai', 'llama_cpp', 'ollama', 'lm_studio', 'custom')),
    endpoint_url TEXT NOT NULL,
    api_key_encrypted TEXT,
    model_name TEXT NOT NULL,
    temperature REAL NOT NULL DEFAULT 0.2,
    max_concurrent_requests INTEGER NOT NULL DEFAULT 3,
    request_delay_ms INTEGER NOT NULL DEFAULT 500,
    context_window_tokens INTEGER NOT NULL DEFAULT 50000
);

CREATE INDEX IF NOT EXISTS idx_articles_status ON articles(status);
CREATE INDEX IF NOT EXISTS idx_articles_duplicate_of ON articles(duplicate_of);
CREATE INDEX IF NOT EXISTS idx_articles_screened_at ON articles(screened_at);
CREATE INDEX IF NOT EXISTS idx_audit_entries_article_id ON audit_entries(article_id);
CREATE INDEX IF NOT EXISTS idx_criteria_type ON criteria(type);
"#;
```

- [ ] **Step 7: Create `src-tauri/src/db/migrations/mod.rs`**

```rust
pub mod v001_initial;

pub struct Migration {
    pub version: i32,
    pub up_sql: &'static str,
}

pub fn get_migrations() -> Vec<Migration> {
    vec![Migration {
        version: v001_initial::VERSION,
        up_sql: v001_initial::UP_SQL,
    }]
}
```

- [ ] **Step 8: Update `src-tauri/src/lib.rs` to register DB module**

Replace `src-tauri/src/lib.rs` with:

```rust
pub mod db;
pub mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 9: Run tests**

Run: `cd src-tauri && cargo test db_test --test db_test`
Expected: PASS — both tests pass

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/db/ src-tauri/src/lib.rs src-tauri/tests/db_test.rs
git commit -m "feat(db): add SQLite schema and migration infrastructure"
```

---

## Task 3: Rust Model Types

**Files:**
- Create: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/models/article.rs`
- Create: `src-tauri/src/models/criterion.rs`
- Create: `src-tauri/src/models/tag.rs`
- Create: `src-tauri/src/models/label.rs`
- Create: `src-tauri/src/models/audit.rs`
- Create: `src-tauri/src/models/llm_config.rs`

- [ ] **Step 1: Create `src-tauri/src/models/mod.rs`**

```rust
pub mod article;
pub mod audit;
pub mod criterion;
pub mod label;
pub mod llm_config;
pub mod tag;
```

- [ ] **Step 2: Create `src-tauri/src/models/article.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub id: String,
    pub status: ArticleStatus,
    pub screening_error: bool,
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub start_page: Option<String>,
    pub end_page: Option<String>,
    pub keywords: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub publisher_city: Option<String>,
    pub publisher_address: Option<String>,
    pub issn: Option<String>,
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub user_notes: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub duplicate_of: Option<String>,
    pub ai_decision: Option<AiDecision>,
    pub ai_reasoning: Option<String>,
    pub ai_confidence: Option<f64>,
    pub matched_inclusion_criteria: Vec<String>,
    pub matched_exclusion_criteria: Vec<String>,
    pub tags: Vec<String>,
    pub labels: Vec<String>,
    pub manual_override: bool,
    pub import_source: Option<String>,
    pub imported_at: String,
    pub screened_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArticleStatus {
    Imported,
    Working,
    Included,
    Rejected,
}

impl ArticleStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Working => "working",
            Self::Included => "included",
            Self::Rejected => "rejected",
        }
    }
}

impl std::fmt::Display for ArticleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiDecision {
    Include,
    Exclude,
}

impl AiDecision {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Include => "include",
            Self::Exclude => "exclude",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewArticle {
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub start_page: Option<String>,
    pub end_page: Option<String>,
    pub keywords: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub publisher_city: Option<String>,
    pub publisher_address: Option<String>,
    pub issn: Option<String>,
    pub reference_type: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub ris_extras: Option<serde_json::Value>,
    pub import_source: Option<String>,
}
```

- [ ] **Step 3: Create `src-tauri/src/models/criterion.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAim {
    pub id: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Criterion {
    pub id: String,
    pub criterion_type: CriterionType,
    pub text: String,
    pub priority: Priority,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CriterionType {
    Inclusion,
    Exclusion,
}

impl CriterionType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inclusion => "inclusion",
            Self::Exclusion => "exclusion",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Critical = 5,
    High = 4,
    Standard = 3,
    Low = 2,
    Optional = 1,
}

impl Priority {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Standard => "standard",
            Self::Low => "low",
            Self::Optional => "optional",
        }
    }
}
```

- [ ] **Step 4: Create `src-tauri/src/models/tag.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub source: TagSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagSource {
    AiSuggested,
    UserCreated,
    RisKeyword,
}

impl TagSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AiSuggested => "ai_suggested",
            Self::UserCreated => "user_created",
            Self::RisKeyword => "ris_keyword",
        }
    }
}
```

- [ ] **Step 5: Create `src-tauri/src/models/label.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    pub source: LabelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelSource {
    AiGenerated,
    UserCreated,
}

impl LabelSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AiGenerated => "ai_generated",
            Self::UserCreated => "user_created",
        }
    }
}
```

- [ ] **Step 6: Create `src-tauri/src/models/audit.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub article_id: String,
    pub timestamp: String,
    pub action: AuditAction,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub details: Option<String>,
    pub source: AuditSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditAction {
    Import,
    DedupMerge,
    DedupFlag,
    StatusChange,
    TagAdd,
    TagRemove,
    LabelAdd,
    LabelRemove,
    CriteriaMatch,
    AiScreen,
    ManualOverride,
    AiSummary,
}

impl AuditAction {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::DedupMerge => "dedup_merge",
            Self::DedupFlag => "dedup_flag",
            Self::StatusChange => "status_change",
            Self::TagAdd => "tag_add",
            Self::TagRemove => "tag_remove",
            Self::LabelAdd => "label_add",
            Self::LabelRemove => "label_remove",
            Self::CriteriaMatch => "criteria_match",
            Self::AiScreen => "ai_screen",
            Self::ManualOverride => "manual_override",
            Self::AiSummary => "ai_summary",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditSource {
    Ai,
    User,
    System,
}

impl AuditSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::User => "user",
            Self::System => "system",
        }
    }
}
```

- [ ] **Step 7: Create `src-tauri/src/models/llm_config.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub endpoint_url: String,
    pub api_key_encrypted: Option<String>,
    pub model_name: String,
    pub temperature: f64,
    pub max_concurrent_requests: i32,
    pub request_delay_ms: i32,
    pub context_window_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LlmProvider {
    Openai,
    Google,
    ZAi,
    LlamaCpp,
    Ollama,
    LmStudio,
    Custom,
}

impl LlmProvider {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Google => "google",
            Self::ZAi => "z_ai",
            Self::LlamaCpp => "llama_cpp",
            Self::Ollama => "ollama",
            Self::LmStudio => "lm_studio",
            Self::Custom => "custom",
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Openai,
            endpoint_url: String::new(),
            api_key_encrypted: None,
            model_name: String::new(),
            temperature: 0.2,
            max_concurrent_requests: 3,
            request_delay_ms: 500,
            context_window_tokens: 50_000,
        }
    }
}
```

- [ ] **Step 8: Register models in `lib.rs`**

Update `src-tauri/src/lib.rs`:

```rust
pub mod db;
pub mod error;
pub mod models;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 9: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/models/ src-tauri/src/lib.rs
git commit -m "feat(models): add Rust model types for all domain entities"
```

---

## Task 4: Tauri Command Stubs & DB State Setup

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/db_test.rs` (add tests)

- [ ] **Step 1: Create `src-tauri/src/commands/mod.rs` with health-check command**

```rust
use crate::db::connection::DbState;
use crate::error::AppError;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub status: String,
    pub article_count: usize,
}

#[tauri::command]
pub fn health_check(db_state: tauri::State<'_, DbState>) -> Result<HealthCheck, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let count: usize = conn
        .query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))
        .unwrap_or(0);
    Ok(HealthCheck {
        status: "ok".to_string(),
        article_count: count,
    })
}
```

- [ ] **Step 2: Update `src-tauri/src/lib.rs` to initialize DB and register commands**

```rust
pub mod commands;
pub mod db;
pub mod error;
pub mod models;

use db::connection::{create_connection, DbState};
use db::migration::run_migrations;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let conn = create_connection().expect("Failed to create database connection");
    run_migrations(&conn).expect("Failed to run database migrations");

    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DbState {
            conn: std::sync::Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![commands::health_check])
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 3: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/lib.rs
git commit -m "feat(commands): add Tauri command infrastructure with health check"
```

---

## Task 5: TypeScript Types & Tauri Command Wrapper

**Files:**
- Create: `src/types/index.ts`
- Create: `src/composables/use-tauri-command.ts`

- [ ] **Step 1: Create `src/types/index.ts`**

```typescript
export interface Article {
  id: string;
  status: ArticleStatus;
  screeningError: boolean;
  title: string;
  abstractText: string;
  authors: string[];
  publicationYear: number | null;
  doi: string | null;
  journal: string | null;
  volume: string | null;
  issue: string | null;
  startPage: string | null;
  endPage: string | null;
  keywords: string[];
  url: string | null;
  language: string | null;
  publisher: string | null;
  publisherCity: string | null;
  publisherAddress: string | null;
  issn: string | null;
  referenceType: string | null;
  date: string | null;
  authorAddress: string | null;
  accessionNumber: string | null;
  customField3: string | null;
  journalAbbreviation: string | null;
  journalIsoAbbreviation: string | null;
  notes: string | null;
  webOfScienceDb: string | null;
  userNotes: string | null;
  risExtras: Record<string, unknown> | null;
  duplicateOf: string | null;
  aiDecision: AiDecision | null;
  aiReasoning: string | null;
  aiConfidence: number | null;
  matchedInclusionCriteria: string[];
  matchedExclusionCriteria: string[];
  tags: string[];
  labels: string[];
  manualOverride: boolean;
  importSource: string | null;
  importedAt: string;
  screenedAt: string | null;
}

export type ArticleStatus = 'imported' | 'working' | 'included' | 'rejected';
export type AiDecision = 'include' | 'exclude';

export interface ResearchAim {
  id: string;
  text: string;
  createdAt: string;
}

export interface Criterion {
  id: string;
  criterionType: CriterionType;
  text: string;
  priority: Priority;
  createdAt: string;
}

export type CriterionType = 'inclusion' | 'exclusion';
export type Priority = 'critical' | 'high' | 'standard' | 'low' | 'optional';

export interface Tag {
  id: string;
  name: string;
  source: TagSource;
}

export type TagSource = 'ai_suggested' | 'user_created' | 'ris_keyword';

export interface Label {
  id: string;
  name: string;
  source: LabelSource;
}

export type LabelSource = 'ai_generated' | 'user_created';

export interface AuditEntry {
  id: string;
  articleId: string;
  timestamp: string;
  action: AuditAction;
  fromStatus: string | null;
  toStatus: string | null;
  details: string | null;
  source: AuditSource;
}

export type AuditAction =
  | 'import'
  | 'dedup_merge'
  | 'dedup_flag'
  | 'status_change'
  | 'tag_add'
  | 'tag_remove'
  | 'label_add'
  | 'label_remove'
  | 'criteria_match'
  | 'ai_screen'
  | 'manual_override'
  | 'ai_summary';

export type AuditSource = 'ai' | 'user' | 'system';

export interface LlmConfig {
  provider: LlmProvider;
  endpointUrl: string;
  apiKeyEncrypted: string | null;
  modelName: string;
  temperature: number;
  maxConcurrentRequests: number;
  requestDelayMs: number;
  contextWindowTokens: number;
}

export type LlmProvider = 'openai' | 'google' | 'z_ai' | 'llama_cpp' | 'ollama' | 'lm_studio' | 'custom';

export interface HealthCheck {
  status: string;
  articleCount: number;
}
```

- [ ] **Step 2: Create `src/composables/use-tauri-command.ts`**

```typescript
import { invoke } from '@tauri-apps/api/core';

export async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}
```

- [ ] **Step 3: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/types/ src/composables/
git commit -m "feat(frontend): add TypeScript types and Tauri command wrapper"
```

---

## Task 6: CSS Design Tokens & Base Styles

**Files:**
- Create: `src/styles/tokens.css`
- Create: `src/styles/base.css`

- [ ] **Step 1: Create `src/styles/tokens.css`**

```css
:root {
  /* Colors — Scholarly Precision */
  --color-primary: #4f46e5;
  --color-primary-container: #4f46e5;
  --color-on-primary: #ffffff;
  --color-surface: #fcf8ff;
  --color-surface-dim: #dcd8e5;
  --color-surface-container: #f0ecf9;
  --color-surface-container-low: #f5f2ff;
  --color-surface-container-high: #eae6f4;
  --color-surface-container-highest: #e4e1ee;
  --color-surface-bright: #fcf8ff;
  --color-on-surface: #1b1b24;
  --color-on-surface-variant: #464555;
  --color-outline: #777587;
  --color-outline-variant: #c7c4d8;
  --color-sidebar: #1e293b;
  --color-sidebar-text: #e2e8f0;
  --color-sidebar-hover: #334155;
  --color-error: #ba1a1a;
  --color-error-container: #ffdad6;
  --color-on-error: #ffffff;
  --color-border: #e5e7eb;
  --color-hover: #f3f4f6;

  /* Priority Colors */
  --color-priority-critical: #ef4444;
  --color-priority-high: #f97316;
  --color-priority-standard: #3b82f6;
  --color-priority-low: #6b7280;
  --color-priority-optional: #9ca3af;

  /* Typography */
  --font-family: Inter, system-ui, -apple-system, sans-serif;
  --font-family-mono: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, monospace;

  --font-size-display: 24px;
  --font-size-h1: 20px;
  --font-size-h2: 16px;
  --font-size-body: 14px;
  --font-size-caption: 13px;
  --font-size-label: 11px;
  --font-size-mono: 13px;

  --font-weight-semibold: 600;
  --font-weight-regular: 400;

  --line-height-display: 32px;
  --line-height-h1: 28px;
  --line-height-h2: 24px;
  --line-height-body: 20px;
  --line-height-caption: 18px;
  --line-height-label: 16px;

  --letter-spacing-display: -0.02em;
  --letter-spacing-h1: -0.01em;
  --letter-spacing-label: 0.05em;

  /* Spacing */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-10: 40px;

  /* Layout */
  --sidebar-width: 260px;
  --container-padding: 24px;
  --gutter: 16px;

  /* Border Radius */
  --radius-sm: 4px;
  --radius-default: 8px;
  --radius-md: 12px;
  --radius-lg: 16px;
  --radius-pill: 9999px;

  /* Shadows */
  --shadow-sm: 0 4px 12px rgba(0, 0, 0, 0.05);
}
```

- [ ] **Step 2: Create `src/styles/base.css`**

```css
@import './tokens.css';

*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html {
  font-size: 16px;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  font-family: var(--font-family);
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-regular);
  line-height: var(--line-height-body);
  color: var(--color-on-surface);
  background-color: var(--color-surface);
}

h1 {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  line-height: var(--line-height-h1);
  letter-spacing: var(--letter-spacing-h1);
}

h2 {
  font-size: var(--font-size-h2);
  font-weight: var(--font-weight-semibold);
  line-height: var(--line-height-h2);
}

code,
pre {
  font-family: var(--font-family-mono);
  font-size: var(--font-size-mono);
}

button {
  cursor: pointer;
  font-family: inherit;
  font-size: inherit;
  border: none;
  background: none;
}

input,
textarea,
select {
  font-family: inherit;
  font-size: inherit;
}

a {
  color: var(--color-primary);
  text-decoration: none;
}

::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: var(--color-outline-variant);
  border-radius: var(--radius-pill);
}
```

- [ ] **Step 3: Commit**

```bash
git add src/styles/
git commit -m "feat(styles): add design tokens and base CSS from DESIGN.md"
```

---

## Task 7: Vue Router & Placeholder Views

**Files:**
- Create: `src/router/index.ts`
- Create: `src/views/dashboard.vue`
- Create: `src/views/placeholder.vue`

- [ ] **Step 1: Install vue-router**

Run: `npm install vue-router@4`

- [ ] **Step 2: Create `src/router/index.ts`**

```typescript
import { createRouter, createWebHashHistory } from 'vue-router';

const Dashboard = () => import('@/views/dashboard.vue');
const Placeholder = () => import('@/views/placeholder.vue');

const routes = [
  { path: '/', name: 'dashboard', component: Dashboard },
  { path: '/articles', name: 'articles', component: Placeholder, props: { title: 'Articles' } },
  { path: '/import', name: 'import', component: Placeholder, props: { title: 'RIS Import' } },
  { path: '/dedup', name: 'dedup', component: Placeholder, props: { title: 'Deduplication' } },
  { path: '/criteria', name: 'criteria', component: Placeholder, props: { title: 'Criteria Editor' } },
  { path: '/screening', name: 'screening', component: Placeholder, props: { title: 'AI Screening' } },
  { path: '/tags', name: 'tags', component: Placeholder, props: { title: 'Tags & Labels' } },
  { path: '/prisma', name: 'prisma', component: Placeholder, props: { title: 'PRISMA Flow Diagram' } },
  { path: '/settings', name: 'settings', component: Placeholder, props: { title: 'LLM Configuration' } },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
```

- [ ] **Step 3: Create `src/views/dashboard.vue`**

```vue
<script setup lang="ts">
</script>

<template>
  <div class="dashboard">
    <h1>Project Dashboard</h1>
    <p class="dashboard__subtitle">AI-assisted systematic literature review</p>
  </div>
</template>

<style scoped>
.dashboard {
  padding: var(--space-6);
}

.dashboard h1 {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
}

.dashboard__subtitle {
  color: var(--color-on-surface-variant);
  margin-top: var(--space-2);
}
</style>
```

- [ ] **Step 4: Create `src/views/placeholder.vue`**

```vue
<script setup lang="ts">
defineProps<{ title: string }>();
</script>

<template>
  <div class="placeholder">
    <h1>{{ title }}</h1>
    <p>Coming soon</p>
  </div>
</template>

<style scoped>
.placeholder {
  padding: var(--space-6);
  color: var(--color-on-surface-variant);
}

.placeholder h1 {
  color: var(--color-on-surface);
}
</style>
```

- [ ] **Step 5: Update `src/main.ts` to use router**

```typescript
import { createApp } from 'vue';
import { createPinia } from 'pinia';
import router from './router';
import App from './App.vue';
import './styles/base.css';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#app');
```

- [ ] **Step 6: Install pinia**

Run: `npm install pinia`

- [ ] **Step 7: Run `npm run lint:check && npm run format:check`**

Run: `npm run lint:check && npm run format:check`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/router/ src/views/ src/main.ts package.json package-lock.json
git commit -m "feat(router): add Vue Router, Pinia, and placeholder views"
```

---

## Task 8: App Shell & Navigation Sidebar

**Files:**
- Create: `src/components/nav-sidebar.vue`
- Create: `src/components/app-shell.vue`
- Modify: `src/App.vue`

- [ ] **Step 1: Create `src/components/nav-sidebar.vue`**

```vue
<script setup lang="ts">
import { useRoute } from 'vue-router';

const route = useRoute();

interface NavItem {
  label: string;
  icon: string;
  route: string;
}

const navItems: NavItem[] = [
  { label: 'Dashboard', icon: '▣', route: '/' },
  { label: 'Articles', icon: '☷', route: '/articles' },
  { label: 'Import RIS', icon: '↑', route: '/import' },
  { label: 'Deduplicate', icon: '⊞', route: '/dedup' },
  { label: 'Criteria', icon: '✓', route: '/criteria' },
  { label: 'Screening', icon: '◎', route: '/screening' },
  { label: 'Tags & Labels', icon: '◉', route: '/tags' },
  { label: 'PRISMA', icon: ' ◦', route: '/prisma' },
  { label: 'Settings', icon: '⚙', route: '/settings' },
];
</script>

<template>
  <nav class="sidebar">
    <div class="sidebar__header">
      <span class="sidebar__logo">B</span>
      <span class="sidebar__title">Bango</span>
    </div>
    <ul class="sidebar__nav">
      <li v-for="item in navItems" :key="item.route">
        <router-link
          :to="item.route"
          class="sidebar__link"
          :class="{ 'sidebar__link--active': route.path === item.route }"
        >
          <span class="sidebar__icon">{{ item.icon }}</span>
          <span class="sidebar__label">{{ item.label }}</span>
        </router-link>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  height: 100vh;
  background-color: var(--color-sidebar);
  color: var(--color-sidebar-text);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  overflow-y: auto;
}

.sidebar__header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-4);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.sidebar__logo {
  width: 32px;
  height: 32px;
  background-color: var(--color-primary);
  border-radius: var(--radius-default);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-h1);
  color: var(--color-on-primary);
}

.sidebar__title {
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-h2);
}

.sidebar__nav {
  list-style: none;
  padding: var(--space-2) 0;
}

.sidebar__link {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  color: var(--color-sidebar-text);
  text-decoration: none;
  font-size: var(--font-size-caption);
  transition: background-color 0.15s;
}

.sidebar__link:hover {
  background-color: var(--color-sidebar-hover);
}

.sidebar__link--active {
  background-color: var(--color-sidebar-hover);
  color: #ffffff;
}

.sidebar__icon {
  width: 20px;
  text-align: center;
  font-size: 14px;
  opacity: 0.7;
}

.sidebar__link--active .sidebar__icon {
  opacity: 1;
}
</style>
```

- [ ] **Step 2: Create `src/components/app-shell.vue`**

```vue
<script setup lang="ts">
import NavSidebar from './nav-sidebar.vue';
</script>

<template>
  <div class="app-shell">
    <NavSidebar />
    <main class="app-shell__main">
      <router-view />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.app-shell__main {
  flex: 1;
  overflow-y: auto;
  background-color: var(--color-surface);
}
</style>
```

- [ ] **Step 3: Replace `src/App.vue`**

```vue
<script setup lang="ts">
import AppShell from './components/app-shell.vue';
</script>

<template>
  <AppShell />
</template>
```

- [ ] **Step 4: Run `npm run lint:check && npm run format:check`**

Run: `npm run lint:check && npm run format:check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/ src/App.vue
git commit -m "feat(ui): add app shell with navigation sidebar"
```

---

## Task 9: Pinia Store Stubs

**Files:**
- Create: `src/stores/articles.ts`
- Create: `src/stores/criteria.ts`
- Create: `src/stores/tags.ts`
- Create: `src/stores/labels.ts`
- Create: `src/utils/formatters.ts`

- [ ] **Step 1: Create `src/utils/formatters.ts`**

```typescript
export function formatDate(isoString: string): string {
  return new Date(isoString).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function formatConfidence(confidence: number | null): string {
  if (confidence === null) return '—';
  return `${Math.round(confidence * 100)}%`;
}

export function formatPriority(priority: string): string {
  return priority.charAt(0).toUpperCase() + priority.slice(1);
}
```

- [ ] **Step 2: Create `src/stores/articles.ts`**

```typescript
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Article, ArticleStatus } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useArticlesStore = defineStore('articles', () => {
  const articles = ref<Article[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const byStatus = computed(() => {
    const counts: Record<ArticleStatus, number> = {
      imported: 0,
      working: 0,
      included: 0,
      rejected: 0,
    };
    for (const article of articles.value) {
      counts[article.status]++;
    }
    return counts;
  });

  const totalImported = computed(() => articles.value.length);

  async function fetchArticles(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('get_articles');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { articles, loading, error, byStatus, totalImported, fetchArticles };
});
```

- [ ] **Step 3: Create `src/stores/criteria.ts`**

```typescript
import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { ResearchAim, Criterion } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useCriteriaStore = defineStore('criteria', () => {
  const aims = ref<ResearchAim[]>([]);
  const criteria = ref<Criterion[]>([]);
  const loading = ref(false);

  const inclusionCriteria = ref<Criterion[]>([]);
  const exclusionCriteria = ref<Criterion[]>([]);

  async function fetchAll(): Promise<void> {
    loading.value = true;
    try {
      const [aimsResult, criteriaResult] = await Promise.all([
        tauriCommand<ResearchAim[]>('get_research_aims'),
        tauriCommand<Criterion[]>('get_criteria'),
      ]);
      aims.value = aimsResult;
      criteria.value = criteriaResult;
      inclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'inclusion');
      exclusionCriteria.value = criteriaResult.filter((c) => c.criterionType === 'exclusion');
    } finally {
      loading.value = false;
    }
  }

  return { aims, criteria, inclusionCriteria, exclusionCriteria, loading, fetchAll };
});
```

- [ ] **Step 4: Create `src/stores/tags.ts`**

```typescript
import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Tag } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useTagsStore = defineStore('tags', () => {
  const tags = ref<Tag[]>([]);
  const loading = ref(false);

  async function fetchTags(): Promise<void> {
    loading.value = true;
    try {
      tags.value = await tauriCommand<Tag[]>('get_tags');
    } finally {
      loading.value = false;
    }
  }

  return { tags, loading, fetchTags };
});
```

- [ ] **Step 5: Create `src/stores/labels.ts`**

```typescript
import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { Label } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useLabelsStore = defineStore('labels', () => {
  const labels = ref<Label[]>([]);
  const loading = ref(false);

  async function fetchLabels(): Promise<void> {
    loading.value = true;
    try {
      labels.value = await tauriCommand<Label[]>('get_labels');
    } finally {
      loading.value = false;
    }
  }

  return { labels, loading, fetchLabels };
});
```

- [ ] **Step 6: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/stores/ src/utils/
git commit -m "feat(stores): add Pinia store stubs and utility formatters"
```

---

## Task 10: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS — all linting, formatting, and Rust checks pass

- [ ] **Step 2: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Verify app launches**

Run: `cd src-tauri && cargo tauri dev`
Expected: App window opens with sidebar navigation and Dashboard view. Clicking sidebar items navigates to placeholder views.

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A
git commit -m "chore: fix any lint/format issues from foundation setup"
```
