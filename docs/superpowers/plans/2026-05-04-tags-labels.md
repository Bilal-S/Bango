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

- [ ] **Step 2: Update `src-tauri/src/db/mod.rs` — add `pub mod tag_repo;` and `pub mod label_repo;`**

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

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` — add `pub mod tags;` and `pub mod labels;`**

- [ ] **Step 4: Update `src-tauri/src/lib.rs` invoke handler — add all tag/label commands**

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

- [ ] **Step 1: Create `src/components/tag-chip.vue`**

```vue
<script setup lang="ts">
defineProps<{ name: string }>();
defineEmits<{ remove: [] }>();
</script>

<template>
  <span class="tag-chip">
    {{ name }}
    <button class="tag-chip__remove" @click="$emit('remove')">×</button>
  </span>
</template>

<style scoped>
.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  background-color: var(--color-surface-container-high);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface);
}

.tag-chip__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  font-size: 12px;
  color: var(--color-on-surface-variant);
  cursor: pointer;
  padding: 0;
  line-height: 1;
}

.tag-chip__remove:hover {
  background-color: var(--color-surface-dim);
}
</style>
```

- [ ] **Step 2: Create `src/components/label-chip.vue`**

```vue
<script setup lang="ts">
defineProps<{ name: string }>();
defineEmits<{ remove: [] }>();
</script>

<template>
  <span class="label-chip">
    {{ name }}
    <button class="label-chip__remove" @click="$emit('remove')">×</button>
  </span>
</template>

<style scoped>
.label-chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface);
  background: transparent;
}

.label-chip__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  font-size: 12px;
  color: var(--color-on-surface-variant);
  cursor: pointer;
  padding: 0;
  line-height: 1;
}

.label-chip__remove:hover {
  background-color: var(--color-surface-container-high);
}
</style>
```

- [ ] **Step 3: Create `src/views/tag-label-management.vue`**

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { Tag, Label } from '@/types';
import TagChip from '@/components/tag-chip.vue';
import LabelChip from '@/components/label-chip.vue';

const tags = ref<Tag[]>([]);
const labels = ref<Label[]>([]);
const newTagName = ref('');
const newLabelName = ref('');
const loading = ref(false);
const suggestingTags = ref(false);
const suggestingLabels = ref(false);

onMounted(async () => {
  await Promise.all([fetchTags(), fetchLabels()]);
});

async function fetchTags(): Promise<void> {
  tags.value = await tauriCommand<Tag[]>('get_tags');
}

async function fetchLabels(): Promise<void> {
  labels.value = await tauriCommand<Label[]>('get_labels');
}

async function addTag(): Promise<void> {
  if (!newTagName.value.trim()) return;
  await tauriCommand('create_tag', { request: { name: newTagName.value.trim() } });
  newTagName.value = '';
  await fetchTags();
}

async function removeTag(id: string): Promise<void> {
  await tauriCommand('delete_tag', { id });
  await fetchTags();
}

async function suggestTags(): Promise<void> {
  suggestingTags.value = true;
  try {
    await tauriCommand('suggest_tags');
    await fetchTags();
  } finally {
    suggestingTags.value = false;
  }
}

async function addLabel(): Promise<void> {
  if (!newLabelName.value.trim()) return;
  await tauriCommand('create_label', { request: { name: newLabelName.value.trim() } });
  newLabelName.value = '';
  await fetchLabels();
}

async function removeLabel(id: string): Promise<void> {
  await tauriCommand('delete_label', { id });
  await fetchLabels();
}

async function suggestLabels(): Promise<void> {
  suggestingLabels.value = true;
  try {
    await tauriCommand('suggest_labels');
    await fetchLabels();
  } finally {
    suggestingLabels.value = false;
  }
}
</script>

<template>
  <div class="tag-label-view">
    <h1>Tags & Labels</h1>

    <div class="tag-label-view__panels">
      <!-- Tags Panel -->
      <section class="panel">
        <div class="panel__header">
          <h2>Tags</h2>
          <button class="btn btn--secondary" :disabled="suggestingTags" @click="suggestTags">
            {{ suggestingTags ? 'Generating...' : 'Suggest Tags' }}
          </button>
        </div>
        <div class="panel__input-row">
          <input
            v-model="newTagName"
            type="text"
            placeholder="Add tag..."
            class="input"
            @keyup.enter="addTag"
          />
          <button class="btn btn--primary" @click="addTag">Add</button>
        </div>
        <div class="panel__chips">
          <TagChip
            v-for="tag in tags"
            :key="tag.id"
            :name="tag.name"
            @remove="removeTag(tag.id)"
          />
          <p v-if="tags.length === 0" class="panel__empty">No tags yet</p>
        </div>
      </section>

      <!-- Labels Panel -->
      <section class="panel">
        <div class="panel__header">
          <h2>Labels</h2>
          <button class="btn btn--secondary" :disabled="suggestingLabels" @click="suggestLabels">
            {{ suggestingLabels ? 'Generating...' : 'Suggest Labels' }}
          </button>
        </div>
        <div class="panel__input-row">
          <input
            v-model="newLabelName"
            type="text"
            placeholder="Add label..."
            class="input"
            @keyup.enter="addLabel"
          />
          <button class="btn btn--primary" @click="addLabel">Add</button>
        </div>
        <div class="panel__chips">
          <LabelChip
            v-for="label in labels"
            :key="label.id"
            :name="label.name"
            @remove="removeLabel(label.id)"
          />
          <p v-if="labels.length === 0" class="panel__empty">No labels yet</p>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.tag-label-view {
  padding: var(--space-6);
}

.tag-label-view h1 {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
  margin-bottom: var(--space-6);
}

.tag-label-view__panels {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-6);
}

.panel {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.panel__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.panel__header h2 {
  font-size: var(--font-size-h2);
}

.panel__input-row {
  display: flex;
  gap: var(--space-2);
}

.input {
  flex: 1;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  outline: none;
}

.input:focus {
  border-color: var(--color-primary);
  border-width: 2px;
}

.panel__chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  min-height: 40px;
}

.panel__empty {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.btn {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
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
