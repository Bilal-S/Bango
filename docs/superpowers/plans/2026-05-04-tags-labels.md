# Tags & Labels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement tag and label CRUD operations with AI-powered suggestion, so users can manage content categories and workflow markers before and during screening.

**Architecture:** Rust modules for tag/label database operations and AI suggestion prompts. The LLM client (from Plan 4) sends OpenAI-compatible requests with the suggestion prompt templates. The frontend provides a dual-panel management view with colored chips for tags and outlined chips for labels.

**Tech Stack:** Rust (rusqlite, reqwest), Tauri commands, Vue 3

**Depends on:** Plan 1 (Foundation & Database), Plan 4 (Criteria & LLM Configuration)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── commands/
│   ├── tags.rs           (new: tag CRUD + suggest)
│   ├── labels.rs         (new: label CRUD + suggest)
│   └── mod.rs            (modify: add modules)
├── db/
│   ├── tag_repo.rs       (new: tag database operations)
│   ├── label_repo.rs     (new: label database operations)
│   └── mod.rs            (modify: add modules)
├── tests/
│   └── tags_labels_test.rs (new: tag/label tests)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   └── tag-label-management.vue (new: dual-panel management)
├── components/
│   ├── tag-chip.vue       (new: solid colored chip)
│   └── label-chip.vue     (new: outlined chip)
├── composables/
│   ├── use-tags.ts        (modify: add CRUD actions)
│   └── use-labels.ts      (modify: add CRUD actions)
├── router/
│   └── index.ts           (modify: update tags route)
```

---

## Task 1: Tag Repository

**Files:**
- Create: `src-tauri/src/db/tag_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/tests/tags_labels_test.rs`

- [ ] **Step 1: Create `src-tauri/src/db/tag_repo.rs`**

```rust
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::tag::{Tag, TagSource};

pub fn get_all_tags(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, source FROM tags ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let source_str: String = row.get(2)?;
        let source = match source_str.as_str() {
            "ai_suggested" => TagSource::AiSuggested,
            "ris_keyword" => TagSource::RisKeyword,
            _ => TagSource::UserCreated,
        };
        Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            source,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_tag(conn: &Connection, name: &str, source: &str) -> Result<Tag, AppError> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tags (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    let source_enum = match source {
        "ai_suggested" => TagSource::AiSuggested,
        "ris_keyword" => TagSource::RisKeyword,
        _ => TagSource::UserCreated,
    };
    Ok(Tag {
        id,
        name: name.to_string(),
        source: source_enum,
    })
}

pub fn rename_tag(conn: &Connection, id: &str, new_name: &str) -> Result<Tag, AppError> {
    conn.execute(
        "UPDATE tags SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )?;
    let tag = get_all_tags(conn)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Tag {} not found", id)))?;
    Ok(tag)
}

pub fn delete_tag(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn merge_tags(conn: &Connection, source_id: &str, target_id: &str) -> Result<Tag, AppError> {
    // Re-link all articles from source tag to target tag
    conn.execute(
        "UPDATE OR IGNORE article_tags SET tag_id = ?1 WHERE tag_id = ?2",
        params![target_id, source_id],
    )?;
    // Delete the source tag
    delete_tag(conn, source_id)?;
    get_all_tags(conn)?
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::NotFound(format!("Tag {} not found", target_id)))
}

pub fn get_article_count_for_tag(conn: &Connection, tag_id: &str) -> Result<usize, AppError> {
    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM article_tags WHERE tag_id = ?1",
            params![tag_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}

pub fn create_tags_batch(conn: &Connection, names: &[String], source: &str) -> Result<Vec<Tag>, AppError> {
    let mut tags = Vec::with_capacity(names.len());
    for name in names {
        // Skip duplicates (case-insensitive)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get::<_, usize>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !exists {
            tags.push(create_tag(conn, name, source)?);
        }
    }
    Ok(tags)
}
```

- [ ] **Step 2: Update `src-tauri/src/db/mod.rs` - add `pub mod tag_repo;` and `pub mod label_repo;`**

- [ ] **Step 3: Write failing tests in `src-tauri/tests/tags_labels_test.rs`**

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::tag_repo;

#[test]
fn test_create_and_get_tags() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let tag = tag_repo::create_tag(&conn, "machine-learning", "user_created").unwrap();
    assert_eq!(tag.name, "machine-learning");

    let tags = tag_repo::get_all_tags(&conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "machine-learning");
}

#[test]
fn test_rename_tag() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let tag = tag_repo::create_tag(&conn, "ml", "user_created").unwrap();
    let renamed = tag_repo::rename_tag(&conn, &tag.id, "machine-learning").unwrap();
    assert_eq!(renamed.name, "machine-learning");
}

#[test]
fn test_delete_tag() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let tag = tag_repo::create_tag(&conn, "test", "user_created").unwrap();
    tag_repo::delete_tag(&conn, &tag.id).unwrap();
    assert!(tag_repo::get_all_tags(&conn).unwrap().is_empty());
}

#[test]
fn test_create_tags_batch_skips_duplicates() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    tag_repo::create_tag(&conn, "ml", "user_created").unwrap();
    let names = vec!["ml".to_string(), "dl".to_string()];
    let created = tag_repo::create_tags_batch(&conn, &names, "ai_suggested").unwrap();
    assert_eq!(created.len(), 1); // Only "dl" created, "ml" skipped
    assert_eq!(created[0].name, "dl");
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test tags_labels_test --test tags_labels_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/tag_repo.rs src-tauri/src/db/mod.rs src-tauri/tests/tags_labels_test.rs
git commit -m "feat(tags): add tag repository with CRUD and batch creation"
```

---

## Task 2: Label Repository

**Files:**
- Create: `src-tauri/src/db/label_repo.rs`

- [ ] **Step 1: Create `src-tauri/src/db/label_repo.rs`**

```rust
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::label::{Label, LabelSource};

pub fn get_all_labels(conn: &Connection) -> Result<Vec<Label>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, source FROM labels ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let source_str: String = row.get(2)?;
        let source = match source_str.as_str() {
            "ai_generated" => LabelSource::AiGenerated,
            _ => LabelSource::UserCreated,
        };
        Ok(Label {
            id: row.get(0)?,
            name: row.get(1)?,
            source,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_label(conn: &Connection, name: &str, source: &str) -> Result<Label, AppError> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO labels (id, name, source) VALUES (?1, ?2, ?3)",
        params![id, name, source],
    )?;
    let source_enum = match source {
        "ai_generated" => LabelSource::AiGenerated,
        _ => LabelSource::UserCreated,
    };
    Ok(Label {
        id,
        name: name.to_string(),
        source: source_enum,
    })
}

pub fn rename_label(conn: &Connection, id: &str, new_name: &str) -> Result<Label, AppError> {
    conn.execute(
        "UPDATE labels SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )?;
    get_all_labels(conn)?
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Label {} not found", id)))
}

pub fn delete_label(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM labels WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn merge_labels(conn: &Connection, source_id: &str, target_id: &str) -> Result<Label, AppError> {
    conn.execute(
        "UPDATE OR IGNORE article_labels SET label_id = ?1 WHERE label_id = ?2",
        params![target_id, source_id],
    )?;
    delete_label(conn, source_id)?;
    get_all_labels(conn)?
        .into_iter()
        .find(|l| l.id == target_id)
        .ok_or_else(|| AppError::NotFound(format!("Label {} not found", target_id)))
}

pub fn get_article_count_for_label(conn: &Connection, label_id: &str) -> Result<usize, AppError> {
    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM article_labels WHERE label_id = ?1",
            params![label_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(count)
}

pub fn create_labels_batch(conn: &Connection, names: &[String], source: &str) -> Result<Vec<Label>, AppError> {
    let mut labels = Vec::with_capacity(names.len());
    for name in names {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE LOWER(name) = LOWER(?1)",
                params![name],
                |row| row.get::<_, usize>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !exists {
            labels.push(create_label(conn, name, source)?);
        }
    }
    Ok(labels)
}
```

- [ ] **Step 2: Add label tests to `src-tauri/tests/tags_labels_test.rs`**

```rust
use bango_lib::db::label_repo;

#[test]
fn test_create_and_get_labels() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let label = label_repo::create_label(&conn, "priority-read", "user_created").unwrap();
    assert_eq!(label.name, "priority-read");

    let labels = label_repo::get_all_labels(&conn).unwrap();
    assert_eq!(labels.len(), 1);
}

#[test]
fn test_rename_label() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let label = label_repo::create_label(&conn, "old-name", "user_created").unwrap();
    let renamed = label_repo::rename_label(&conn, &label.id, "new-name").unwrap();
    assert_eq!(renamed.name, "new-name");
}

#[test]
fn test_delete_label() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let label = label_repo::create_label(&conn, "test", "user_created").unwrap();
    label_repo::delete_label(&conn, &label.id).unwrap();
    assert!(label_repo::get_all_labels(&conn).unwrap().is_empty());
}

#[test]
fn test_tag_label_isolation() {
    // Tags and labels live in separate tables - names can overlap
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    tag_repo::create_tag(&conn, "machine-learning", "user_created").unwrap();
    label_repo::create_label(&conn, "machine-learning", "user_created").unwrap();

    let tags = tag_repo::get_all_tags(&conn).unwrap();
    let labels = label_repo::get_all_labels(&conn).unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(labels.len(), 1);
    assert_eq!(tags[0].name, labels[0].name); // Same name, different entities
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test tags_labels_test --test tags_labels_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/label_repo.rs src-tauri/tests/tags_labels_test.rs
git commit -m "feat(labels): add label repository with CRUD operations"
```

---

## Task 3: Tag & Label Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/tags.rs`
- Create: `src-tauri/src/commands/labels.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/tags.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::tag_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::tag::Tag;
use crate::models::llm_config::LlmConfig;
use crate::db::llm_config_repo;
use crate::db::criteria_repo;

#[tauri::command]
pub fn get_tags(db_state: State<'_, DbState>) -> Result<Vec<Tag>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::get_all_tags(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagRequest {
    pub name: String,
}

#[tauri::command]
pub fn create_tag(db_state: State<'_, DbState>, request: CreateTagRequest) -> Result<Tag, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::create_tag(&conn, &request.name, "user_created")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTagRequest {
    pub id: String,
    pub new_name: String,
}

#[tauri::command]
pub fn rename_tag(db_state: State<'_, DbState>, request: RenameTagRequest) -> Result<Tag, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::rename_tag(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_tag(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    tag_repo::delete_tag(&conn, &id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestTagsResult {
    pub tags: Vec<Tag>,
}

#[tauri::command]
pub async fn suggest_tags(db_state: State<'_, DbState>) -> Result<SuggestTagsResult, AppError> {
    let (config, keywords, inclusion_criteria, exclusion_criteria) = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        let config = llm_config_repo::get_config(&conn)?.ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let articles = crate::db::article_repo::get_articles_by_status(&conn, "working")?;
        let keywords: Vec<String> = articles.iter()
            .flat_map(|a| a.keywords.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion")?;
        let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion")?;
        (config, keywords, inc, exc)
    };

    let keywords_str = keywords.join(", ");
    let inc_list: Vec<String> = inclusion_criteria.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.text)).collect();
    let exc_list: Vec<String> = exclusion_criteria.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.text)).collect();

    let user_prompt = format!(
        r#"## Task
Generate a concise set of content-category tags for organizing articles in a systematic literature review. Tags should represent meaningful topic, methodology, or relevance categories.

## Article Keywords
{keywords}

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Response Format
Return JSON exactly matching this schema:
{{
  "tags": ["tag-name-1", "tag-name-2", ...]
}}

Rules:
- Generate 10-30 tags.
- Each tag should be a short, lowercase, hyphenated string (e.g., "machine-learning", "clinical-trial").
- Tags should be derived from the keywords and criteria provided.
- Do not duplicate or overlap concepts."#,
        keywords = keywords_str,
        inclusion = inc_list.join("\n"),
        exclusion = exc_list.join("\n"),
    );

    let system_prompt = "You are a systematic literature review assistant. Generate a set of content-category tags for organizing articles in a literature review.";
    let response = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    // Parse response
    let json_str = response.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| AppError::Import(format!("Failed to parse tag suggestion response: {}", e)))?;
    let tag_names: Vec<String> = parsed["tags"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let tags = tag_repo::create_tags_batch(&conn, &tag_names, "ai_suggested")?;

    Ok(SuggestTagsResult { tags })
}
```

- [ ] **Step 2: Create `src-tauri/src/commands/labels.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::connection::DbState;
use crate::db::label_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::models::label::Label;
use crate::db::llm_config_repo;
use crate::db::criteria_repo;

#[tauri::command]
pub fn get_labels(db_state: State<'_, DbState>) -> Result<Vec<Label>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::get_all_labels(&conn)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelRequest {
    pub name: String,
}

#[tauri::command]
pub fn create_label(db_state: State<'_, DbState>, request: CreateLabelRequest) -> Result<Label, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::create_label(&conn, &request.name, "user_created")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLabelRequest {
    pub id: String,
    pub new_name: String,
}

#[tauri::command]
pub fn rename_label(db_state: State<'_, DbState>, request: RenameLabelRequest) -> Result<Label, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::rename_label(&conn, &request.id, &request.new_name)
}

#[tauri::command]
pub fn delete_label(db_state: State<'_, DbState>, id: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    label_repo::delete_label(&conn, &id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestLabelsResult {
    pub labels: Vec<Label>,
}

#[tauri::command]
pub async fn suggest_labels(db_state: State<'_, DbState>) -> Result<SuggestLabelsResult, AppError> {
    let (config, inclusion_criteria, exclusion_criteria) = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        let config = llm_config_repo::get_config(&conn)?.ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let inc = criteria_repo::get_criteria_by_type(&conn, "inclusion")?;
        let exc = criteria_repo::get_criteria_by_type(&conn, "exclusion")?;
        (config, inc, exc)
    };

    let inc_list: Vec<String> = inclusion_criteria.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.text)).collect();
    let exc_list: Vec<String> = exclusion_criteria.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.text)).collect();

    let user_prompt = format!(
        r#"## Task
Generate a set of workflow labels for tracking articles through the screening process. Labels should represent organizational or process categories (e.g., "priority-read", "disputed", "needs-full-text", "strong-methodology").

## Inclusion Criteria
{inclusion}

## Exclusion Criteria
{exclusion}

## Response Format
Return JSON exactly matching this schema:
{{
  "labels": ["label-name-1", "label-name-2", ...]
}}

Rules:
- Generate 5-15 labels.
- Each label should be a short, descriptive string.
- Labels should help categorize articles by their screening status or quality indicators.
- Do not duplicate or overlap concepts."#,
        inclusion = inc_list.join("\n"),
        exclusion = exc_list.join("\n"),
    );

    let system_prompt = "You are a systematic literature review assistant. Generate a set of workflow labels for tracking the screening process.";
    let response = client::send_chat_completion(&config, system_prompt, &user_prompt).await?;

    let json_str = response.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| AppError::Import(format!("Failed to parse label suggestion response: {}", e)))?;
    let label_names: Vec<String> = parsed["labels"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let labels = label_repo::create_labels_batch(&conn, &label_names, "ai_generated")?;

    Ok(SuggestLabelsResult { labels })
}
```

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` - add `pub mod tags;` and `pub mod labels;`**

- [ ] **Step 4: Update `src-tauri/src/lib.rs` invoke handler - add all tag/label commands**

```rust
commands::tags::get_tags,
commands::tags::create_tag,
commands::tags::rename_tag,
commands::tags::delete_tag,
commands::tags::suggest_tags,
commands::labels::get_labels,
commands::labels::create_label,
commands::labels::rename_label,
commands::labels::delete_label,
commands::labels::suggest_labels,
```

- [ ] **Step 5: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/tags.rs src-tauri/src/commands/labels.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(tags-labels): add Tauri commands for tag/label CRUD and AI suggestion"
```

---

## Task 4: Frontend Tag & Label Management UI

**Files:**
- Create: `src/components/tag-chip.vue`
- Create: `src/components/label-chip.vue`
- Create: `src/views/tag-label-management.vue`
- Modify: `src/stores/tags.ts`
- Modify: `src/stores/labels.ts`
- Modify: `src/router/index.ts`

> **Design reference:** Before implementing, read `docs/design-reference/08-tags-labels.html` and `docs/design-reference/08-tags-labels.png`. Extract the exact layout structure, spacing, and component hierarchy from the Stitch HTML. Implement only v3-scoped elements per `docs/design-reference/00-design-patterns.md` Section 14.

- [ ] **Step 1: Create `src/components/tag-chip.vue`** - Hash-based multi-color solid chip per design reference

```vue
<script setup lang="ts">
defineProps<{ name: string }>();

const TAG_COLORS = [
  { bg: 'bg-blue-100', text: 'text-blue-700', border: 'border-blue-200' },
  { bg: 'bg-green-100', text: 'text-green-700', border: 'border-green-200' },
  { bg: 'bg-purple-100', text: 'text-purple-700', border: 'border-purple-200' },
  { bg: 'bg-amber-100', text: 'text-amber-700', border: 'border-amber-200' },
  { bg: 'bg-cyan-100', text: 'text-cyan-700', border: 'border-cyan-200' },
  { bg: 'bg-rose-100', text: 'text-rose-700', border: 'border-rose-200' },
] as const;

function getColor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length];
}
</script>

<template>
  <span
    :class="[getColor(name).bg, getColor(name).text, getColor(name).border]"
    class="inline-flex items-center px-2 py-0.5 rounded-lg font-mono text-[11px] border"
  >
    {{ name }}
  </span>
</template>
```

- [ ] **Step 2: Create `src/components/label-chip.vue`** - Outlined chip with colored dot indicator per design reference

```vue
<script setup lang="ts">
defineProps<{ name: string }>();

const DOT_COLORS = [
  'bg-blue-400',
  'bg-green-400',
  'bg-purple-400',
  'bg-amber-400',
  'bg-cyan-400',
  'bg-rose-400',
] as const;

function getDotColor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return DOT_COLORS[Math.abs(hash) % DOT_COLORS.length];
}
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-lg border border-outline-variant font-mono text-[11px] text-on-surface"
  >
    <span :class="getDotColor(name)" class="w-1.5 h-1.5 rounded-full flex-shrink-0"></span>
    {{ name }}
  </span>
</template>
```

- [ ] **Step 3: Create `src/views/tag-label-management.vue`** - Uses Pinia stores with error handling, Tailwind utilities, design-reference layout

> **Key patterns:** Uses `useTagsStore`/`useLabelsStore` Pinia stores (not raw `tauriCommand`). Includes `error`/`loading` states with retry UI. Layout uses Tailwind with `@theme` tokens from `base.css`. No scoped CSS needed.

```vue
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import TagChip from '@/components/tag-chip.vue';
import LabelChip from '@/components/label-chip.vue';

const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();

const newTagName = ref('');
const newLabelName = ref('');
const editingTagId = ref<string | null>(null);
const editingTagName = ref('');
const editingLabelId = ref<string | null>(null);
const editingLabelName = ref('');

const isLoading = computed(() => tagsStore.loading && labelsStore.loading);
const hasError = computed(() => tagsStore.error || labelsStore.error);
const errorMessage = computed(() => tagsStore.error || labelsStore.error || 'Unknown error');

onMounted(async () => {
  await Promise.all([tagsStore.fetchTags(), labelsStore.fetchLabels()]);
});

async function addTag(): Promise<void> {
  const name = newTagName.value.trim();
  if (!name) return;
  await tagsStore.createTag(name);
  newTagName.value = '';
}

async function addLabel(): Promise<void> {
  const name = newLabelName.value.trim();
  if (!name) return;
  await labelsStore.createLabel(name);
  newLabelName.value = '';
}

function startEditingTag(id: string, currentName: string): void {
  editingTagId.value = id;
  editingTagName.value = currentName;
}

async function saveTagEdit(): Promise<void> {
  if (!editingTagId.value) return;
  const name = editingTagName.value.trim();
  if (!name) { cancelTagEdit(); return; }
  await tagsStore.renameTag(editingTagId.value, name);
  editingTagId.value = null;
  editingTagName.value = '';
}

function cancelTagEdit(): void {
  editingTagId.value = null;
  editingTagName.value = '';
}

function startEditingLabel(id: string, currentName: string): void {
  editingLabelId.value = id;
  editingLabelName.value = currentName;
}

async function saveLabelEdit(): Promise<void> {
  if (!editingLabelId.value) return;
  const name = editingLabelName.value.trim();
  if (!name) { cancelLabelEdit(); return; }
  await labelsStore.renameLabel(editingLabelId.value, name);
  editingLabelId.value = null;
  editingLabelName.value = '';
}

function cancelLabelEdit(): void {
  editingLabelId.value = null;
  editingLabelName.value = '';
}

async function retry(): Promise<void> {
  await Promise.all([tagsStore.fetchTags(), labelsStore.fetchLabels()]);
}
</script>

<template>
  <div class="p-container-padding bg-surface-container-low min-h-full">
    <div class="max-w-7xl mx-auto space-y-stack-gap">
      <!-- Page Header -->
      <div class="flex items-center justify-between pb-4">
        <div>
          <h1 class="font-display text-display text-on-surface">Tag & Label Management</h1>
          <p class="font-body-main text-body-main text-on-surface-variant mt-1">
            Organize your academic taxonomy and workflow states.
          </p>
        </div>
      </div>

      <!-- Error State -->
      <div
        v-if="hasError && !isLoading"
        class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm p-6 text-center"
      >
        <span class="material-symbols-outlined text-error text-[32px] mb-2 block">cloud_off</span>
        <h2 class="font-h2 text-h2 text-on-surface mb-1">Unable to load tags & labels</h2>
        <p class="font-body-sm text-body-sm text-on-surface-variant mb-4">{{ errorMessage }}</p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2 bg-primary-container text-on-primary rounded-lg font-body-main text-body-main font-medium hover:opacity-90 transition-opacity"
          @click="retry"
        >
          <span class="material-symbols-outlined text-[18px]">refresh</span>
          Retry
        </button>
      </div>

      <!-- Loading State -->
      <div
        v-else-if="isLoading"
        class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm p-6 text-center"
      >
        <span class="material-symbols-outlined text-primary text-[32px] mb-2 block animate-spin">progress_activity</span>
        <p class="font-body-main text-body-main text-on-surface-variant">Loading tags & labels…</p>
      </div>

      <!-- Dual-Panel Layout (matches design reference 08-tags-labels.html) -->
      <div v-else class="grid grid-cols-1 lg:grid-cols-2 gap-container-padding items-start">
        <!-- Tags Panel -->
        <section class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col h-[700px]">
          <div class="p-5 border-b border-surface-variant bg-surface-bright flex-shrink-0">
            <div class="flex items-center justify-between mb-4">
              <div>
                <h2 class="font-h2 text-h2 text-on-surface flex items-center gap-2">
                  <span class="material-symbols-outlined text-primary text-[20px]">sell</span>
                  Tags
                </h2>
                <p class="font-body-sm text-body-sm text-on-surface-variant mt-0.5">
                  Content-category labels for grouping related research.
                </p>
              </div>
              <span class="bg-surface-variant text-on-surface-variant px-2 py-0.5 rounded-full font-label-caps text-label-caps">
                {{ tagsStore.tags.length }} Total
              </span>
            </div>
            <div class="flex gap-2">
              <div class="relative flex-1">
                <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-outline text-[18px]">add</span>
                <input
                  v-model="newTagName"
                  class="w-full pl-9 pr-3 py-2 bg-surface-container-lowest border border-outline-variant rounded-lg focus:border-primary focus:ring-1 focus:ring-primary font-body-main text-body-main text-on-surface transition-all"
                  placeholder="Add new tag..."
                  type="text"
                  @keyup.enter="addTag"
                />
              </div>
              <button
                class="flex items-center gap-2 px-4 py-2 bg-secondary-container text-on-secondary-container hover:bg-secondary-fixed transition-colors rounded-lg font-body-main text-body-main font-medium border border-secondary-fixed-dim whitespace-nowrap disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="tagsStore.suggesting"
                @click="tagsStore.suggestTags()"
              >
                <span class="material-symbols-outlined text-[18px]">auto_awesome</span>
                {{ tagsStore.suggesting ? 'Generating...' : 'Generate with AI' }}
              </button>
            </div>
          </div>
          <div class="p-5 overflow-y-auto flex-1 space-y-3">
            <div
              v-for="tag in tagsStore.tags"
              :key="tag.id"
              class="flex items-center justify-between group p-2 hover:bg-surface-container rounded-lg transition-colors"
            >
              <div class="flex items-center gap-3">
                <template v-if="editingTagId === tag.id">
                  <input
                    v-model="editingTagName"
                    class="px-2 py-1 bg-surface-container-lowest border border-primary rounded-lg focus:ring-1 focus:ring-primary font-mono text-mono text-on-surface transition-all w-48"
                    @keyup.enter="saveTagEdit"
                    @keyup.escape="cancelTagEdit"
                  />
                </template>
                <template v-else>
                  <TagChip :name="tag.name" />
                </template>
              </div>
              <div class="flex items-center gap-4">
                <span class="font-body-sm text-body-sm text-on-surface-variant">{{ tag.articleCount }} articles</span>
                <div class="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <template v-if="editingTagId === tag.id">
                    <button class="p-1 text-primary hover:bg-surface-variant rounded transition-colors" @click="saveTagEdit">
                      <span class="material-symbols-outlined text-[16px]">check</span>
                    </button>
                    <button class="p-1 text-outline hover:bg-surface-variant rounded transition-colors" @click="cancelTagEdit">
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                  <template v-else>
                    <button class="p-1 text-outline hover:text-primary rounded hover:bg-surface-variant transition-colors" @click="startEditingTag(tag.id, tag.name)">
                      <span class="material-symbols-outlined text-[16px]">edit</span>
                    </button>
                    <button class="p-1 text-outline hover:text-error rounded hover:bg-error-container transition-colors" @click="tagsStore.deleteTag(tag.id)">
                      <span class="material-symbols-outlined text-[16px]">close</span>
                    </button>
                  </template>
                </div>
              </div>
            </div>
            <p v-if="tagsStore.tags.length === 0" class="text-on-surface-variant font-body-sm text-body-sm text-center py-8">
              No tags yet. Add one above or generate from AI.
            </p>
          </div>
        </section>

        <!-- Labels Panel (mirrors Tags Panel with secondary color accent) -->
        <section class="bg-surface-container-lowest rounded-xl border border-surface-variant shadow-sm overflow-hidden flex flex-col h-[700px]">
          <!-- Same structure as Tags Panel with label-specific bindings -->
          <!-- (Full template matches source file in src/views/tag-label-management.vue) -->
        </section>
      </div>
    </div>
  </div>
</template>
```

> **Note:** The Labels Panel template mirrors the Tags Panel with `labelsStore` bindings and `secondary` color accents. See the actual source file `src/views/tag-label-management.vue` for the complete template.

- [ ] **Step 3b: Update `src/stores/tags.ts` and `src/stores/labels.ts`** - Add error handling

Both stores must wrap `tauriCommand` calls in `try/catch`, expose an `error` ref, and set it on failure. See current source files for exact implementation. Key pattern:

```typescript
const error = ref<string | null>(null);

async function fetchTags(): Promise<void> {
  loading.value = true;
  error.value = null;
  try {
    tags.value = await tauriCommand<TagWithCount[]>('get_tags_with_counts');
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}
```

- [ ] **Step 4: Update router**

In `src/router/index.ts`, add:

```typescript
const TagLabelManagement = () => import('@/views/tag-label-management.vue');
```

Change tags route:

```typescript
{ path: '/tags', name: 'tags', component: TagLabelManagement },
```

- [ ] **Step 5: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/components/tag-chip.vue src/components/label-chip.vue src/views/tag-label-management.vue src/router/index.ts
git commit -m "feat(tags-labels): add dual-panel tag/label management UI"
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
git commit -m "chore: fix any issues from tags/labels implementation"
```
