# Export & Project Backup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement RIS export for included articles, project backup as encrypted `.bango.json`, and project import with password-based decryption.

**Architecture:** RIS export builds valid RIS format strings from article data. Project backup serializes all data to JSON with encrypted API keys. Import validates spec version and decrypts with user password.

**Tech Stack:** Rust (serde_json, aes-gcm, pbkdf2), Tauri commands with file dialogs, Vue 3

**Depends on:** Plan 1, Plan 4 (Criteria & LLM Config), Plan 6 (AI Screening)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── export/
│   ├── mod.rs              (new: module declarations)
│   ├── ris_writer.rs       (new: RIS format writer)
│   └── project.rs          (new: project backup/restore)
├── commands/
│   ├── export_cmd.rs       (new: export/import commands)
│   └── mod.rs              (modify: add module)
├── tests/
│   └── export_test.rs      (new: export tests)
```

### TypeScript/Vue (src/)

```
src/
├── components/
│   ├── export-dialog.vue   (new: export options dialog)
│   └── import-dialog.vue   (new: import with password)
├── composables/
│   └── use-export.ts       (new: export/import composable)
```

---

## Task 1: RIS Writer

**Files:**
- Create: `src-tauri/src/export/mod.rs`
- Create: `src-tauri/src/export/ris_writer.rs`
- Create: `src-tauri/tests/export_test.rs`

- [ ] **Step 1: Create `src-tauri/src/export/mod.rs`**

```rust
pub mod project;
pub mod ris_writer;
```

- [ ] **Step 2: Write failing tests in `src-tauri/tests/export_test.rs`**

```rust
use bango_lib::export::ris_writer::{article_to_ris, RisExportArticle};
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

fn make_export_article() -> RisExportArticle {
    RisExportArticle {
        reference_type: Some("JOUR".to_string()),
        title: "Machine Learning for Reviews".to_string(),
        abstract_text: "Abstract text about ML.".to_string(),
        authors: vec!["Smith, John".to_string(), "Doe, Jane".to_string()],
        publication_year: Some(2023),
        doi: Some("10.1234/test".to_string()),
        journal: Some("J Med Inform".to_string()),
        volume: Some("120".to_string()),
        issue: Some("3".to_string()),
        start_page: Some("45".to_string()),
        end_page: Some("58".to_string()),
        keywords: vec!["ml".to_string()],
        tags: vec!["machine-learning".to_string()],
        url: Some("https://example.com".to_string()),
        language: Some("English".to_string()),
        publisher: Some("Elsevier".to_string()),
        issn: Some("1234-5678".to_string()),
        ai_reasoning: Some("Article meets criteria.".to_string()),
        user_notes: None,
        ai_decision: Some("include".to_string()),
        labels: vec!["priority-read".to_string(), "strong-methodology".to_string()],
    }
}

#[test]
fn test_article_to_ris_basic_fields() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.starts_with("TY  - JOUR"));
    assert!(ris.contains("TI  - Machine Learning for Reviews"));
    assert!(ris.contains("AB  - Abstract text about ML."));
    assert!(ris.contains("AU  - Smith, John"));
    assert!(ris.contains("AU  - Doe, Jane"));
    assert!(ris.contains("PY  - 2023"));
    assert!(ris.contains("DO  - 10.1234/test"));
    assert!(ris.ends_with("ER  -\n"));
}

#[test]
fn test_ris_includes_tags_as_keywords() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.contains("KW  - ml"));
    assert!(ris.contains("KW  - machine-learning"));
}

#[test]
fn test_ris_includes_ai_reasoning_as_notes() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.contains("N1  - Article meets criteria."));
}

#[test]
fn test_ris_includes_labels() {
    let article = make_export_article();
    let ris = article_to_ris(&article);
    assert!(ris.contains("C1  -"));
    assert!(ris.contains("priority-read"));
    assert!(ris.contains("strong-methodology"));
}

#[test]
fn test_ris_skips_none_fields() {
    let mut article = make_export_article();
    article.doi = None;
    let ris = article_to_ris(&article);
    assert!(!ris.contains("DO  -"));
}

#[test]
fn test_multiple_articles_to_ris() {
    let article = make_export_article();
    let ris = article_to_ris(&article) + &article_to_ris(&article);
    assert_eq!(ris.matches("ER  -").count(), 2);
}

#[test]
fn test_ris_roundtrip_with_real_data() {
    // Parse a real RIS file, convert to articles, then export back to RIS
    let content = fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let parsed = parse_ris(&content).expect("Parse failed");
    let record = &parsed.records[0];

    // Verify key fields survive roundtrip
    assert!(record.title.is_some());
    assert!(record.doi.is_some());
    assert!(record.abstract_text.is_some());
    assert!(!record.authors.is_empty());

    // Export to RIS string
    let exported = write_ris(record);

    // Re-parse the exported RIS
    let reparsed = parse_ris(&exported).expect("Re-parse failed");
    assert_eq!(reparsed.records.len(), 1);

    let rerecord = &reparsed.records[0];
    assert_eq!(rerecord.title, record.title);
    assert_eq!(rerecord.doi, record.doi);
    assert_eq!(rerecord.authors, record.authors);
    assert_eq!(rerecord.publication_year, record.publication_year);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd src-tauri && cargo test export_test --test export_test`
Expected: FAIL

- [ ] **Step 4: Implement `src-tauri/src/export/ris_writer.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RisExportArticle {
    pub reference_type: Option<String>,
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
    pub tags: Vec<String>,
    pub url: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub issn: Option<String>,
    pub ai_reasoning: Option<String>,
    pub user_notes: Option<String>,
    pub ai_decision: Option<String>,
    pub labels: Vec<String>,
}

pub fn article_to_ris(article: &RisExportArticle) -> String {
    let mut lines = Vec::new();

    lines.push(format!("TY  - {}", article.reference_type.as_deref().unwrap_or("JOUR")));
    lines.push(format!("TI  - {}", article.title));
    lines.push(format!("AB  - {}", article.abstract_text));

    for author in &article.authors {
        lines.push(format!("AU  - {}", author));
    }

    if let Some(year) = article.publication_year {
        lines.push(format!("PY  - {}", year));
    }
    if let Some(ref doi) = article.doi {
        lines.push(format!("DO  - {}", doi));
    }
    if let Some(ref journal) = article.journal {
        lines.push(format!("T2  - {}", journal));
    }
    if let Some(ref vol) = article.volume {
        lines.push(format!("VL  - {}", vol));
    }
    if let Some(ref issue) = article.issue {
        lines.push(format!("IS  - {}", issue));
    }
    if let Some(ref sp) = article.start_page {
        lines.push(format!("SP  - {}", sp));
    }
    if let Some(ref ep) = article.end_page {
        lines.push(format!("EP  - {}", ep));
    }

    for kw in &article.keywords {
        lines.push(format!("KW  - {}", kw));
    }
    for tag in &article.tags {
        lines.push(format!("KW  - {}", tag));
    }

    if let Some(ref url) = article.url {
        lines.push(format!("UR  - {}", url));
    }
    if let Some(ref lang) = article.language {
        lines.push(format!("LA  - {}", lang));
    }
    if let Some(ref pub_) = article.publisher {
        lines.push(format!("PB  - {}", pub_));
    }
    if let Some(ref issn) = article.issn {
        lines.push(format!("SN  - {}", issn));
    }
    if let Some(ref reasoning) = article.ai_reasoning {
        lines.push(format!("N1  - {}", reasoning));
    }
    if let Some(ref notes) = article.user_notes {
        lines.push(format!("NO  - {}", notes));
    }

    // Labels grouped by decision
    if !article.labels.is_empty() {
        let decision = article.ai_decision.as_deref().unwrap_or("include");
        let (inc, exc): (Vec<_>, Vec<_>) = if decision == "include" {
            (article.labels.clone(), vec![])
        } else {
            (vec![], article.labels.clone())
        };

        let inc_json = serde_json::to_string(&inc).unwrap_or_default();
        let exc_json = serde_json::to_string(&exc).unwrap_or_default();
        lines.push(format!("C1  - {{\"inc\":{},\"exc\":{}}}", inc_json, exc_json));
    }

    lines.push("ER  -".to_string());

    lines.join("\n") + "\n"
}

pub fn articles_to_ris(articles: &[RisExportArticle]) -> String {
    articles.iter().map(article_to_ris).collect()
}
```

- [ ] **Step 5: Add `pub mod export;` to `src-tauri/src/lib.rs`**

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test export_test --test export_test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/export/ src-tauri/src/lib.rs src-tauri/tests/export_test.rs
git commit -m "feat(export): add RIS writer with all supported tags and label export"
```

---

## Task 2: Project Backup & Restore

**Files:**
- Create: `src-tauri/src/export/project.rs`

- [ ] **Step 1: Create `src-tauri/src/export/project.rs`**

```rust
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::aes_gcm;
use crate::db::article_repo;
use crate::db::criteria_repo;
use crate::db::tag_repo;
use crate::db::label_repo;
use crate::db::audit_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::models::llm_config::LlmConfig;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMetadata {
    pub spec_version: String,
    pub exported_at: String,
    pub app_name: String,
    pub app_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackup {
    pub metadata: ExportMetadata,
    pub research_aims: Vec<serde_json::Value>,
    pub criteria: Vec<serde_json::Value>,
    pub articles: Vec<serde_json::Value>,
    pub tags: Vec<serde_json::Value>,
    pub labels: Vec<serde_json::Value>,
    pub audit_entries: Vec<serde_json::Value>,
    pub llm_config: Option<LlmConfigBackup>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigBackup {
    pub provider: String,
    pub endpoint_url: String,
    pub model_name: String,
    pub api_key_encrypted: Option<String>,
}

pub fn export_project(conn: &Connection, password: &str) -> Result<String, AppError> {
    let aims = serialize_table(conn, "SELECT * FROM research_aims")?;
    let criteria = serialize_table(conn, "SELECT * FROM criteria")?;
    let articles = serialize_table(conn, "SELECT * FROM articles")?;
    let tags = serialize_table(conn, "SELECT * FROM tags")?;
    let labels = serialize_table(conn, "SELECT * FROM labels")?;
    let audit = serialize_table(conn, "SELECT * FROM audit_entries")?;

    let llm_backup = llm_config_repo::get_config(conn)?.map(|c| {
        let key = aes_gcm::derive_key_from_password(password);
        let encrypted_key = c.api_key_encrypted.as_ref()
            .and_then(|k| aes_gcm::encrypt(k.as_bytes(), &key).ok());
        LlmConfigBackup {
            provider: c.provider.as_str().to_string(),
            endpoint_url: c.endpoint_url,
            model_name: c.model_name,
            api_key_encrypted: encrypted_key,
        }
    });

    let backup = ProjectBackup {
        metadata: ExportMetadata {
            spec_version: "3.0".to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            app_name: "Bango".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        research_aims: aims,
        criteria,
        articles,
        tags,
        labels,
        audit_entries: audit,
        llm_config: llm_backup,
    };

    serde_json::to_string_pretty(&backup)
        .map_err(|e| AppError::Serialization(e))
}

fn serialize_table(conn: &Connection, query: &str) -> Result<Vec<serde_json::Value>, AppError> {
    let mut stmt = conn.prepare(query)?;
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let rows = stmt.query_map([], |row| {
        let mut map = serde_json::Map::new();
        for (i, name) in column_names.iter().enumerate() {
            let value: serde_json::Value = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                Ok(rusqlite::types::ValueRef::Text(s)) => {
                    let text = s.to_string();
                    // Try to parse as JSON first
                    serde_json::from_str::<serde_json::Value>(&text).unwrap_or_else(|_| serde_json::json!(text))
                }
                Ok(rusqlite::types::ValueRef::Blob(_)) => serde_json::Value::Null,
                Err(_) => serde_json::Value::Null,
            };
            // Convert snake_case column names to camelCase for consistency
            let camel_name = to_camel_case(name);
            map.insert(camel_name, value);
        }
        Ok(serde_json::Value::Object(map))
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn to_camel_case(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    if parts.len() == 1 {
        return s.to_string();
    }
    let mut result = parts[0].to_string();
    for part in &parts[1..] {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(chars);
        }
    }
    result
}

pub fn import_project(conn: &Connection, json_str: &str, password: &str) -> Result<(), AppError> {
    let backup: ProjectBackup = serde_json::from_str(json_str)
        .map_err(|e| AppError::Import(format!("Invalid backup file: {}", e)))?;

    // Check spec version
    let version: i32 = backup.metadata.spec_version.split('.').next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if version > 3 {
        return Err(AppError::Import(format!(
            "Backup is spec version {} but this app supports version 3. Some data may not import correctly.",
            backup.metadata.spec_version
        )));
    }

    // Clear existing data
    conn.execute("DELETE FROM audit_entries", [])?;
    conn.execute("DELETE FROM article_tags", [])?;
    conn.execute("DELETE FROM article_labels", [])?;
    conn.execute("DELETE FROM articles", [])?;
    conn.execute("DELETE FROM criteria", [])?;
    conn.execute("DELETE FROM research_aims", [])?;
    conn.execute("DELETE FROM tags", [])?;
    conn.execute("DELETE FROM labels", [])?;
    conn.execute("DELETE FROM llm_config", [])?;

    // Restore research aims
    for aim in &backup.research_aims {
        let id = aim.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let text = aim.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let created_at = aim.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
        conn.execute(
            "INSERT INTO research_aims (id, text, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, text, created_at],
        )?;
    }

    // Restore criteria
    for c in &backup.criteria {
        let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let ctype = c.get("criterionType").or_else(|| c.get("type")).and_then(|v| v.as_str()).unwrap_or("inclusion");
        let text = c.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let priority = c.get("priority").and_then(|v| v.as_str()).unwrap_or("standard");
        let created_at = c.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
        conn.execute(
            "INSERT INTO criteria (id, type, text, priority, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, ctype, text, priority, created_at],
        )?;
    }

    // Restore tags
    for t in &backup.tags {
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let source = t.get("source").and_then(|v| v.as_str()).unwrap_or("user_created");
        conn.execute(
            "INSERT INTO tags (id, name, source) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, source],
        )?;
    }

    // Restore labels
    for l in &backup.labels {
        let id = l.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let name = l.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let source = l.get("source").and_then(|v| v.as_str()).unwrap_or("user_created");
        conn.execute(
            "INSERT INTO labels (id, name, source) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, source],
        )?;
    }

    // Restore articles
    for a in &backup.articles {
        let id = get_str(a, "id");
        let status = get_str(a, "status");
        let title = get_str(a, "title");
        let abstract_text = get_str_field(a, "abstractText", "abstract_text");
        let authors_json = serde_json::to_string(&a.get("authors").cloned().unwrap_or(serde_json::json!([]))).unwrap_or_default();
        let import_source = get_str_field(a, "importSource", "import_source");

        conn.execute(
            "INSERT INTO articles (id, status, title, abstract_text, authors, import_source, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            rusqlite::params![id, status, title, abstract_text, authors_json, import_source],
        )?;
    }

    // Restore LLM config with decrypted key
    if let Some(ref llm_backup) = backup.llm_config {
        let key = aes_gcm::derive_key_from_password(password);
        let decrypted_key = llm_backup.api_key_encrypted.as_ref()
            .and_then(|enc| aes_gcm::decrypt(enc, &key).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok());

        let machine_key = aes_gcm::derive_key_from_machine();
        let re_encrypted = decrypted_key.as_ref()
            .map(|k| aes_gcm::encrypt(k.as_bytes(), &machine_key))
            .transpose()
            .ok()
            .flatten();

        conn.execute(
            "INSERT INTO llm_config (id, provider, endpoint_url, api_key_encrypted, model_name, temperature, max_concurrent_requests, request_delay_ms, context_window_tokens) \
             VALUES (1, ?1, ?2, ?3, ?4, 0.2, 3, 500, 50000)",
            rusqlite::params![llm_backup.provider, llm_backup.endpoint_url, re_encrypted, llm_backup.model_name],
        )?;
    }

    Ok(())
}

fn get_str(v: &serde_json::Value, key: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn get_str_field(v: &serde_json::Value, camel: &str, snake: &str) -> String {
    v.get(camel).or_else(|| v.get(snake)).and_then(|v| v.as_str()).unwrap_or("").to_string()
}
```

- [ ] **Step 2: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/export/project.rs
git commit -m "feat(export): add project backup/restore with encrypted API keys"
```

---

## Task 3: Export Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/export_cmd.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/export_cmd.rs`**

```rust
use serde::Deserialize;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::export::project;
use crate::export::ris_writer::{articles_to_ris, RisExportArticle};

#[tauri::command]
pub fn export_ris(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let articles = article_repo::get_articles_by_status(&conn, "included")?;

    let export_articles: Vec<RisExportArticle> = articles.iter().map(|a| RisExportArticle {
        reference_type: a.reference_type.clone(),
        title: a.title.clone(),
        abstract_text: a.abstract_text.clone(),
        authors: a.authors.clone(),
        publication_year: a.publication_year,
        doi: a.doi.clone(),
        journal: a.journal.clone(),
        volume: a.volume.clone(),
        issue: a.issue.clone(),
        start_page: a.start_page.clone(),
        end_page: a.end_page.clone(),
        keywords: a.keywords.clone(),
        tags: vec![], // TODO: load from article_tags join
        url: a.url.clone(),
        language: a.language.clone(),
        publisher: a.publisher.clone(),
        issn: a.issn.clone(),
        ai_reasoning: a.ai_reasoning.clone(),
        user_notes: a.user_notes.clone(),
        ai_decision: a.ai_decision.map(|d| d.as_str().to_string()),
        labels: vec![], // TODO: load from article_labels join
    }).collect();

    Ok(articles_to_ris(&export_articles))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProjectRequest {
    pub password: String,
}

#[tauri::command]
pub fn export_project_backup(db_state: State<'_, DbState>, request: ExportProjectRequest) -> Result<String, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    project::export_project(&conn, &request.password)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProjectRequest {
    pub json_content: String,
    pub password: String,
}

#[tauri::command]
pub fn import_project_backup(db_state: State<'_, DbState>, request: ImportProjectRequest) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    project::import_project(&conn, &request.json_content, &request.password)
}
```

- [ ] **Step 2: Update `src-tauri/src/commands/mod.rs` — add `pub mod export_cmd;`**

- [ ] **Step 3: Update `src-tauri/src/lib.rs` invoke handler**

Add:

```rust
commands::export_cmd::export_ris,
commands::export_cmd::export_project_backup,
commands::export_cmd::import_project_backup,
```

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/export_cmd.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(export): add Tauri commands for RIS export and project backup/restore"
```

---

## Task 4: Frontend Export/Import Dialogs

**Files:**
- Create: `src/composables/use-export.ts`
- Create: `src/components/export-dialog.vue`
- Create: `src/components/import-dialog.vue`

- [ ] **Step 1: Create `src/composables/use-export.ts`**

```typescript
import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);

  async function exportRis(): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const risContent = await tauriCommand<string>('export_ris');
      downloadFile(risContent, 'included-articles.ris', 'application/x-research-info-systems');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function exportProject(password: string): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const jsonContent = await tauriCommand<string>('export_project_backup', {
        request: { password },
      });
      downloadFile(jsonContent, 'bango-project.bango.json', 'application/json');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function importProject(file: File, password: string): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const content = await file.text();
      await tauriCommand('import_project_backup', {
        request: { jsonContent: content, password },
      });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  return { exporting, error, exportRis, exportProject, importProject };
}

function downloadFile(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
```

- [ ] **Step 2: Create `src/components/export-dialog.vue`**

```vue
<script setup lang="ts">
import { ref } from 'vue';
import { useExport } from '@/composables/use-export';

const emit = defineEmits<{ close: [] }>();
const { exporting, error, exportRis, exportProject } = useExport();
const password = ref('');
const showBackup = ref(false);
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <h2>Export</h2>

      <div v-if="error" class="dialog__error">{{ error }}</div>

      <div v-if="!showBackup" class="dialog__options">
        <button class="btn btn--primary" :disabled="exporting" @click="exportRis()">
          Export Included Articles (RIS)
        </button>
        <button class="btn btn--secondary" @click="showBackup = true">
          Export Project Backup
        </button>
      </div>

      <div v-if="showBackup" class="dialog__backup">
        <p>Enter a password to encrypt your API keys in the backup:</p>
        <input v-model="password" type="password" placeholder="Password" class="input" />
        <div class="dialog__actions">
          <button class="btn btn--primary" :disabled="exporting || !password" @click="exportProject(password)">
            {{ exporting ? 'Exporting...' : 'Export Backup' }}
          </button>
          <button class="btn btn--secondary" @click="showBackup = false">Back</button>
        </div>
      </div>

      <button class="btn btn--ghost" @click="emit('close')">Cancel</button>
    </div>
  </div>
</template>

<style scoped>
.dialog-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.3); display: flex; align-items: center; justify-content: center; z-index: 100; }
.dialog { background: white; padding: var(--space-6); border-radius: var(--radius-md); width: 420px; display: flex; flex-direction: column; gap: var(--space-4); }
.dialog h2 { font-size: var(--font-size-h1); }
.dialog__error { padding: var(--space-3); background-color: var(--color-error-container); color: var(--color-error); border-radius: var(--radius-default); font-size: var(--font-size-caption); }
.dialog__options { display: flex; flex-direction: column; gap: var(--space-3); }
.dialog__backup { display: flex; flex-direction: column; gap: var(--space-3); }
.dialog__backup p { font-size: var(--font-size-caption); color: var(--color-on-surface-variant); }
.dialog__actions { display: flex; gap: var(--space-2); }
.input { padding: var(--space-2) var(--space-3); border: 1px solid var(--color-outline); border-radius: var(--radius-default); font-size: var(--font-size-caption); }
.btn { padding: var(--space-2) var(--space-4); border-radius: var(--radius-default); font-size: var(--font-size-caption); font-weight: var(--font-weight-semibold); cursor: pointer; text-align: left; }
.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); text-align: center; }
.btn--secondary { background-color: var(--color-surface-container-high); color: var(--color-on-surface); text-align: center; }
.btn--ghost { color: var(--color-on-surface-variant); text-align: center; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

- [ ] **Step 3: Create `src/components/import-dialog.vue`**

```vue
<script setup lang="ts">
import { ref } from 'vue';
import { useExport } from '@/composables/use-export';

const emit = defineEmits<{ close: []; imported: [] }>();
const { exporting, error, importProject } = useExport();
const password = ref('');
const selectedFile = ref<File | null>(null);

function onFileChange(event: Event): void {
  const target = event.target as HTMLInputElement;
  selectedFile.value = target.files?.[0] ?? null;
}

async function doImport(): Promise<void> {
  if (!selectedFile.value) return;
  await importProject(selectedFile.value, password.value);
  if (!error.value) {
    emit('imported');
    emit('close');
  }
}
</script>

<template>
  <div class="dialog-overlay" @click.self="emit('close')">
    <div class="dialog">
      <h2>Import Project</h2>

      <div v-if="error" class="dialog__error">{{ error }}</div>

      <div class="dialog__form">
        <label class="field__label">Backup File (.bango.json)</label>
        <input type="file" accept=".bango.json,.json" class="input" @change="onFileChange" />

        <label class="field__label">Password (for API keys)</label>
        <input v-model="password" type="password" placeholder="Enter password" class="input" />
        <p class="hint">If password is incorrect, the project will import without API keys.</p>
      </div>

      <div class="dialog__actions">
        <button class="btn btn--primary" :disabled="exporting || !selectedFile" @click="doImport">
          {{ exporting ? 'Importing...' : 'Import' }}
        </button>
        <button class="btn btn--ghost" @click="emit('close')">Cancel</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.3); display: flex; align-items: center; justify-content: center; z-index: 100; }
.dialog { background: white; padding: var(--space-6); border-radius: var(--radius-md); width: 420px; display: flex; flex-direction: column; gap: var(--space-4); }
.dialog h2 { font-size: var(--font-size-h1); }
.dialog__error { padding: var(--space-3); background-color: var(--color-error-container); color: var(--color-error); border-radius: var(--radius-default); font-size: var(--font-size-caption); }
.dialog__form { display: flex; flex-direction: column; gap: var(--space-3); }
.field__label { font-size: var(--font-size-label); font-weight: var(--font-weight-semibold); text-transform: uppercase; letter-spacing: var(--letter-spacing-label); color: var(--color-on-surface-variant); }
.input { padding: var(--space-2) var(--space-3); border: 1px solid var(--color-outline); border-radius: var(--radius-default); font-size: var(--font-size-caption); }
.hint { font-size: 11px; color: var(--color-on-surface-variant); }
.dialog__actions { display: flex; gap: var(--space-2); }
.btn { padding: var(--space-2) var(--space-4); border-radius: var(--radius-default); font-size: var(--font-size-caption); font-weight: var(--font-weight-semibold); cursor: pointer; }
.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }
.btn--ghost { color: var(--color-on-surface-variant); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

- [ ] **Step 4: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/composables/use-export.ts src/components/export-dialog.vue src/components/import-dialog.vue
git commit -m "feat(export): add export/import dialogs with password encryption"
```

---

## Task 5: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any issues from export implementation"
```
