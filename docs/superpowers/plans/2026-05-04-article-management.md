# Article Management & Audit Trail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the core article list view with search, sort, and filter capabilities, a detail panel with audit trail, and article state management for manual overrides.

**Architecture:** Rust repository methods support parametrized search/sort/filter queries on SQLite. Tauri commands accept search parameters and return filtered article lists. The Vue frontend provides a data table with a toolbar and a sliding detail panel.

**Tech Stack:** Rust (rusqlite), Tauri commands, Vue 3

**Depends on:** Plan 1, Plan 2, Plan 3, Plan 5, Plan 6

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── db/
│   ├── article_repo.rs      (modify: add search/filter/sort queries)
│   └── audit_repo.rs        (new: audit trail queries)
├── commands/
│   ├── articles.rs          (new: article query + state commands)
│   └── mod.rs               (modify: add articles module)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   └── article-list.vue     (new: main article list page)
├── components/
│   ├── article-table.vue    (new: data table)
│   ├── article-detail-panel.vue (new: sliding detail panel)
│   ├── article-toolbar.vue  (new: search/filter/sort toolbar)
│   ├── status-badge.vue     (new: status pill badge)
│   ├── confidence-bar.vue   (new: confidence indicator)
│   └── audit-timeline.vue   (new: audit history)
├── composables/
│   └── use-article-search.ts (new: search/filter composable)
├── router/
│   └── index.ts             (modify: update articles route)
```

---

## Task 1: Audit Repository

**Files:**
- Create: `src-tauri/src/db/audit_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`

- [ ] **Step 1: Create `src-tauri/src/db/audit_repo.rs`**

```rust
use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::audit::{AuditAction, AuditEntry, AuditSource};

pub fn get_audit_trail(conn: &Connection, article_id: &str) -> Result<Vec<AuditEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, article_id, timestamp, action, from_status, to_status, details, source \
         FROM audit_entries WHERE article_id = ?1 ORDER BY timestamp DESC"
    )?;
    let rows = stmt.query_map([article_id], row_to_audit)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_audit_entry(
    conn: &Connection,
    article_id: &str,
    action: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    details: Option<&str>,
    source: &str,
) -> Result<AuditEntry, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, timestamp, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, article_id, now, action, from_status, to_status, details, source],
    )?;
    Ok(AuditEntry {
        id,
        article_id: article_id.to_string(),
        timestamp: now,
        action: parse_action(action),
        from_status: from_status.map(String::from),
        to_status: to_status.map(String::from),
        details: details.map(String::from),
        source: parse_source(source),
    })
}

fn row_to_audit(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
    let action_str: String = row.get(3)?;
    let source_str: String = row.get(7)?;
    Ok(AuditEntry {
        id: row.get(0)?,
        article_id: row.get(1)?,
        timestamp: row.get(2)?,
        action: parse_action(&action_str),
        from_status: row.get(4)?,
        to_status: row.get(5)?,
        details: row.get(6)?,
        source: parse_source(&source_str),
    })
}

fn parse_action(s: &str) -> AuditAction {
    match s {
        "import" => AuditAction::Import,
        "dedup_merge" => AuditAction::DedupMerge,
        "dedup_flag" => AuditAction::DedupFlag,
        "status_change" => AuditAction::StatusChange,
        "tag_add" => AuditAction::TagAdd,
        "tag_remove" => AuditAction::TagRemove,
        "label_add" => AuditAction::LabelAdd,
        "label_remove" => AuditAction::LabelRemove,
        "criteria_match" => AuditAction::CriteriaMatch,
        "ai_screen" => AuditAction::AiScreen,
        "manual_override" => AuditAction::ManualOverride,
        "ai_summary" => AuditAction::AiSummary,
        _ => AuditAction::StatusChange,
    }
}

fn parse_source(s: &str) -> AuditSource {
    match s {
        "ai" => AuditSource::Ai,
        "user" => AuditSource::User,
        _ => AuditSource::System,
    }
}
```

- [ ] **Step 2: Add `pub mod audit_repo;` to `src-tauri/src/db/mod.rs`**

- [ ] **Step 3: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/audit_repo.rs src-tauri/src/db/mod.rs
git commit -m "feat(audit): add audit trail repository"
```

- [ ] **Add audit trail tests**

Create `src-tauri/tests/audit_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::audit_repo;

#[test]
fn test_create_and_retrieve_audit_entry() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // Insert an article first
    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('art-1', 'imported', 'Test', 'Abstract', '[\"Author\"]')",
        [],
    ).unwrap();

    audit_repo::create_entry(&conn, "art-1", "import", None, None, Some("Imported from test.ris"), "system").unwrap();

    let entries = audit_repo::get_entries_for_article(&conn, "art-1").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "import");
    assert_eq!(entries[0].source, "system");
}

#[test]
fn test_audit_tracks_status_changes() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    conn.execute(
        "INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('art-2', 'working', 'Test', 'Abstract', '[\"Author\"]')",
        [],
    ).unwrap();

    audit_repo::create_entry(&conn, "art-2", "status_change", Some("imported"), Some("working"), None, "system").unwrap();
    audit_repo::create_entry(&conn, "art-2", "ai_screen", Some("working"), Some("included"), Some("AI screened: include"), "ai").unwrap();

    let entries = audit_repo::get_entries_for_article(&conn, "art-2").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].from_status, Some("imported".to_string()));
    assert_eq!(entries[1].to_status, Some("included".to_string()));
}
```

Run: `cd src-tauri && cargo test audit_test --test audit_test`
Expected: PASS

```bash
git add src-tauri/tests/audit_test.rs
git commit -m "test(audit): add audit trail repository tests"
```

---

## Task 2: Article Query Commands with Search/Filter/Sort

**Files:**
- Create: `src-tauri/src/commands/articles.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/db/article_repo.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add search/filter methods to `src-tauri/src/db/article_repo.rs`**

Add these functions:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleQuery {
    pub status: Option<String>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub tag_ids: Option<Vec<String>>,
    pub label_ids: Option<Vec<String>>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub manual_override_only: bool,
    pub screening_errors_only: bool,
}

pub fn query_articles(conn: &Connection, query: &ArticleQuery) -> Result<Vec<Article>, AppError> {
    let mut sql = String::from("SELECT * FROM articles WHERE 1=1");
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref status) = query.status {
        sql.push_str(&format!(" AND status = ?{}", param_idx));
        param_values.push(Box::new(status.clone()));
        param_idx += 1;
    }

    if let Some(ref search) = query.search {
        sql.push_str(&format!(" AND (LOWER(title) LIKE ?{} OR LOWER(abstract_text) LIKE ?{})", param_idx, param_idx));
        let pattern = format!("%{}%", search.to_lowercase());
        param_values.push(Box::new(pattern));
        param_idx += 1;
    }

    if let Some(ref year_from) = query.year_from {
        sql.push_str(&format!(" AND publication_year >= ?{}", param_idx));
        param_values.push(Box::new(*year_from));
        param_idx += 1;
    }

    if let Some(ref year_to) = query.year_to {
        sql.push_str(&format!(" AND publication_year <= ?{}", param_idx));
        param_values.push(Box::new(*year_to));
        param_idx += 1;
    }

    if query.manual_override_only {
        sql.push_str(" AND manual_override = 1");
    }

    if query.screening_errors_only {
        sql.push_str(" AND screening_error = 1");
    }

    // Sort
    let sort_by = query.sort_by.as_deref().unwrap_or("imported_at");
    let sort_dir = query.sort_dir.as_deref().unwrap_or("DESC");
    let order_clause = match sort_by {
        "title" => format!(" ORDER BY title {}", sort_dir),
        "publicationYear" => format!(" ORDER BY publication_year {} NULLS LAST", sort_dir),
        "aiConfidence" => format!(" ORDER BY ai_confidence {} NULLS LAST", sort_dir),
        _ => format!(" ORDER BY imported_at {}", sort_dir),
    };
    sql.push_str(&order_clause);

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn update_article_status(
    conn: &Connection,
    article_id: &str,
    new_status: &str,
) -> Result<(), AppError> {
    let old_status: String = conn.query_row(
        "SELECT status FROM articles WHERE id = ?1",
        [article_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE articles SET status = ?1, manual_override = 1 WHERE id = ?2",
        params![new_status, article_id],
    )?;

    crate::db::audit_repo::create_audit_entry(
        conn,
        article_id,
        "status_change",
        Some(&old_status),
        Some(new_status),
        Some("Manual status change"),
        "user",
    )?;

    Ok(())
}
```

- [ ] **Step 2: Create `src-tauri/src/commands/articles.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::article_repo::{self, ArticleQuery};
use crate::db::audit_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::Article;
use crate::models::audit::AuditEntry;

#[tauri::command]
pub fn query_articles(db_state: State<'_, DbState>, query: ArticleQuery) -> Result<Vec<Article>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::query_articles(&conn, &query)
}

#[tauri::command]
pub fn get_article(db_state: State<'_, DbState>, id: String) -> Result<Article, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_article_by_id(&conn, &id)
}

#[tauri::command]
pub fn update_article_status(db_state: State<'_, DbState>, id: String, newStatus: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::update_article_status(&conn, &id, &newStatus)
}

#[tauri::command]
pub fn get_audit_trail(db_state: State<'_, DbState>, articleId: String) -> Result<Vec<AuditEntry>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    audit_repo::get_audit_trail(&conn, &articleId)
}

#[tauri::command]
pub fn update_article_notes(db_state: State<'_, DbState>, id: String, notes: String) -> Result<(), AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    conn.execute(
        "UPDATE articles SET user_notes = ?1 WHERE id = ?2",
        rusqlite::params![notes, id],
    )?;
    Ok(())
}
```

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` — add `pub mod articles;`**

- [ ] **Step 4: Update `src-tauri/src/lib.rs` invoke handler**

Add:

```rust
commands::articles::query_articles,
commands::articles::get_article,
commands::articles::update_article_status,
commands::articles::get_audit_trail,
commands::articles::update_article_notes,
```

- [ ] **Step 5: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/articles.rs src-tauri/src/commands/mod.rs src-tauri/src/db/article_repo.rs src-tauri/src/db/audit_repo.rs src-tauri/src/lib.rs
git commit -m "feat(articles): add search/filter/sort queries and state management commands"
```

---

## Task 3: Frontend Article List View

**Files:**
- Create: `src/composables/use-article-search.ts`
- Create: `src/components/status-badge.vue`
- Create: `src/components/confidence-bar.vue`
- Create: `src/components/article-toolbar.vue`
- Create: `src/components/article-table.vue`
- Create: `src/views/article-list.vue`
- Modify: `src/router/index.ts`

> **Design references:**
> - For the article list/table: read `docs/design-reference/03-article-list.html` and `docs/design-reference/03-article-list.png`.
> - For the detail panel: read `docs/design-reference/04-article-detail.html` and `docs/design-reference/04-article-detail.png`.
> Extract the exact layout structure, spacing, and component hierarchy from the Stitch HTML. Implement only v3-scoped elements per `docs/design-reference/00-design-patterns.md` Section 14.

- [ ] **Step 1: Create `src/composables/use-article-search.ts`**

```typescript
import { ref, reactive } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, AuditEntry } from '@/types';

export interface ArticleQuery {
  status: string | null;
  search: string | null;
  sortBy: string | null;
  sortDir: string | null;
  yearFrom: number | null;
  yearTo: number | null;
  manualOverrideOnly: boolean;
  screeningErrorsOnly: boolean;
}

export function useArticleSearch() {
  const articles = ref<Article[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const selectedArticle = ref<Article | null>(null);
  const auditTrail = ref<AuditEntry[]>([]);
  const showDetail = ref(false);

  const query = reactive<ArticleQuery>({
    status: null,
    search: null,
    sortBy: null,
    sortDir: null,
    yearFrom: null,
    yearTo: null,
    manualOverrideOnly: false,
    screeningErrorsOnly: false,
  });

  async function search(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('query_articles', { query });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function selectArticle(id: string): Promise<void> {
    try {
      selectedArticle.value = await tauriCommand<Article>('get_article', { id });
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
      showDetail.value = true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function moveArticle(id: string, newStatus: string): Promise<void> {
    await tauriCommand('update_article_status', { id, newStatus });
    await selectArticle(id);
    await search();
  }

  async function updateNotes(id: string, notes: string): Promise<void> {
    await tauriCommand('update_article_notes', { id, notes });
  }

  function closeDetail(): void {
    showDetail.value = false;
    selectedArticle.value = null;
    auditTrail.value = [];
  }

  return {
    articles, loading, error, query,
    selectedArticle, auditTrail, showDetail,
    search, selectArticle, moveArticle, updateNotes, closeDetail,
  };
}
```

- [ ] **Step 2: Create `src/components/status-badge.vue`**

```vue
<script setup lang="ts">
defineProps<{ status: string }>();
</script>

<template>
  <span class="status-badge" :class="`status-badge--${status}`">
    {{ status }}
  </span>
</template>

<style scoped>
.status-badge {
  display: inline-block;
  padding: 2px 10px;
  border-radius: var(--radius-pill);
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: capitalize;
}

.status-badge--imported { background-color: #e5e7eb; color: #374151; }
.status-badge--working { background-color: #dbeafe; color: #1d4ed8; }
.status-badge--included { background-color: #dcfce7; color: #166534; }
.status-badge--rejected { background-color: #fee2e2; color: #991b1b; }
</style>
```

- [ ] **Step 3: Create `src/components/confidence-bar.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ confidence: number | null }>();
const percentage = computed(() => props.confidence !== null ? Math.round(props.confidence * 100) : 0);
</script>

<template>
  <div class="confidence-bar">
    <div class="confidence-bar__track">
      <div
        class="confidence-bar__fill"
        :style="{ width: `${percentage}%` }"
      />
    </div>
    <span class="confidence-bar__label">{{ confidence !== null ? `${percentage}%` : '—' }}</span>
  </div>
</template>

<style scoped>
.confidence-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.confidence-bar__track {
  width: 60px;
  height: 4px;
  background-color: var(--color-surface-container-high);
  border-radius: var(--radius-pill);
  overflow: hidden;
}

.confidence-bar__fill {
  height: 100%;
  background-color: var(--color-primary);
  border-radius: var(--radius-pill);
}

.confidence-bar__label {
  font-size: 11px;
  color: var(--color-on-surface-variant);
  min-width: 30px;
}
</style>
```

- [ ] **Step 4: Create `src/components/article-toolbar.vue`**

```vue
<script setup lang="ts">
import type { ArticleQuery } from '@/composables/use-article-search';

defineProps<{ query: ArticleQuery }>();
const emit = defineEmits<{
  search: [];
  update: [key: string, value: unknown];
}>();
</script>

<template>
  <div class="toolbar">
    <input
      type="text"
      placeholder="Search articles..."
      class="toolbar__search"
      :value="query.search"
      @input="emit('update', 'search', ($event.target as HTMLInputElement).value || null)"
      @keyup.enter="emit('search')"
    />
    <select
      class="toolbar__select"
      :value="query.status || ''"
      @change="emit('update', 'status', ($event.target as HTMLSelectElement).value || null)"
    >
      <option value="">All Status</option>
      <option value="imported">Imported</option>
      <option value="working">Working</option>
      <option value="included">Included</option>
      <option value="rejected">Rejected</option>
    </select>
    <select
      class="toolbar__select"
      :value="query.sortBy || 'imported_at'"
      @change="emit('update', 'sortBy', ($event.target as HTMLSelectElement).value)"
    >
      <option value="imported_at">Date Added</option>
      <option value="title">Title</option>
      <option value="publicationYear">Year</option>
      <option value="aiConfidence">Confidence</option>
    </select>
    <button class="btn btn--primary" @click="emit('search')">Search</button>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  padding: var(--space-3) 0;
  border-bottom: 1px solid var(--color-border);
}

.toolbar__search {
  flex: 1;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  outline: none;
}

.toolbar__search:focus { border-color: var(--color-primary); }

.toolbar__select {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  background-color: var(--color-surface);
}

.btn {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }
</style>
```

- [ ] **Step 5: Create `src/components/article-table.vue`**

```vue
<script setup lang="ts">
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import ConfidenceBar from './confidence-bar.vue';

defineProps<{ articles: Article[] }>();
defineEmits<{ select: [id: string] }>();
</script>

<template>
  <table class="article-table">
    <thead>
      <tr>
        <th>Title</th>
        <th>Authors</th>
        <th>Year</th>
        <th>Status</th>
        <th>Confidence</th>
      </tr>
    </thead>
    <tbody>
      <tr
        v-for="article in articles"
        :key="article.id"
        class="article-table__row"
        @click="$emit('select', article.id)"
      >
        <td class="article-table__title">{{ article.title }}</td>
        <td>{{ article.authors.slice(0, 2).join('; ') }}{{ article.authors.length > 2 ? ' et al.' : '' }}</td>
        <td>{{ article.publicationYear ?? '—' }}</td>
        <td><StatusBadge :status="article.status" /></td>
        <td><ConfidenceBar :confidence="article.aiConfidence" /></td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
.article-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-caption);
}

.article-table th {
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  color: var(--color-on-surface-variant);
}

.article-table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  color: var(--color-on-surface);
}

.article-table__row {
  cursor: pointer;
}

.article-table__row:hover td {
  background-color: var(--color-hover);
}

.article-table__title {
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}
</style>
```

- [ ] **Step 6: Create `src/views/article-list.vue`**

```vue
<script setup lang="ts">
import { onMounted } from 'vue';
import { useArticleSearch } from '@/composables/use-article-search';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import StatusBadge from '@/components/status-badge.vue';

const {
  articles, loading, query,
  selectedArticle, auditTrail, showDetail,
  search, selectArticle, moveArticle, closeDetail,
} = useArticleSearch();

onMounted(search);

function handleUpdate(key: string, value: unknown): void {
  (query as Record<string, unknown>)[key] = value;
}
</script>

<template>
  <div class="article-list">
    <div class="article-list__main">
      <h1>Articles</h1>
      <ArticleToolbar :query="query" @search="search" @update="handleUpdate" />
      <div v-if="loading" class="article-list__loading">Loading...</div>
      <ArticleTable v-else :articles="articles" @select="selectArticle" />
      <div v-if="!loading && articles.length === 0" class="article-list__empty">
        No articles found. Import an RIS file to get started.
      </div>
    </div>

    <!-- Detail Panel -->
    <div v-if="showDetail && selectedArticle" class="article-list__detail">
      <div class="detail__header">
        <h2>{{ selectedArticle.title }}</h2>
        <button class="btn-icon" @click="closeDetail">×</button>
      </div>

      <div class="detail__section">
        <h3>Abstract</h3>
        <p class="detail__abstract">{{ selectedArticle.abstractText }}</p>
      </div>

      <div class="detail__meta">
        <div v-if="selectedArticle.doi"><strong>DOI:</strong> {{ selectedArticle.doi }}</div>
        <div v-if="selectedArticle.journal"><strong>Journal:</strong> {{ selectedArticle.journal }}</div>
        <div v-if="selectedArticle.publicationYear"><strong>Year:</strong> {{ selectedArticle.publicationYear }}</div>
        <div><strong>Authors:</strong> {{ selectedArticle.authors.join('; ') }}</div>
      </div>

      <div v-if="selectedArticle.aiDecision" class="detail__section detail__ai-card">
        <h3>AI Decision: <StatusBadge :status="selectedArticle.status" /></h3>
        <div class="detail__confidence">Confidence: {{ selectedArticle.aiConfidence ? Math.round(selectedArticle.aiConfidence * 100) + '%' : '—' }}</div>
        <p class="detail__reasoning">{{ selectedArticle.aiReasoning }}</p>
      </div>

      <div class="detail__actions">
        <span class="detail__actions-label">Move to:</span>
        <button v-if="selectedArticle.status !== 'included'" class="btn btn--small" @click="moveArticle(selectedArticle.id, 'included')">Included</button>
        <button v-if="selectedArticle.status !== 'rejected'" class="btn btn--small" @click="moveArticle(selectedArticle.id, 'rejected')">Rejected</button>
        <button v-if="selectedArticle.status !== 'working'" class="btn btn--small" @click="moveArticle(selectedArticle.id, 'working')">Working</button>
      </div>

      <div class="detail__section">
        <h3>Audit Trail</h3>
        <div class="audit-timeline">
          <div v-for="entry in auditTrail" :key="entry.id" class="audit-entry">
            <span class="audit-entry__action">{{ entry.action }}</span>
            <span class="audit-entry__time">{{ entry.timestamp }}</span>
            <span v-if="entry.details" class="audit-entry__details">{{ entry.details }}</span>
          </div>
          <div v-if="auditTrail.length === 0" class="audit-entry__empty">No audit entries</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.article-list {
  display: flex;
  height: 100%;
}

.article-list__main {
  flex: 1;
  padding: var(--space-6);
  overflow-y: auto;
}

.article-list__main h1 {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
}

.article-list__loading,
.article-list__empty {
  text-align: center;
  padding: var(--space-10);
  color: var(--color-on-surface-variant);
}

.article-list__detail {
  width: 400px;
  border-left: 1px solid var(--color-border);
  padding: var(--space-4);
  overflow-y: auto;
  background-color: var(--color-surface-container-low);
}

.detail__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: var(--space-4);
}

.detail__header h2 {
  font-size: var(--font-size-h2);
  padding-right: var(--space-4);
}

.btn-icon {
  width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  border-radius: 50%; font-size: 18px;
  color: var(--color-on-surface-variant);
  cursor: pointer;
}

.detail__section { margin-bottom: var(--space-4); }
.detail__section h3 { font-size: var(--font-size-caption); color: var(--color-on-surface-variant); text-transform: uppercase; margin-bottom: var(--space-2); }

.detail__abstract { font-size: var(--font-size-caption); line-height: 1.6; }

.detail__meta { display: flex; flex-direction: column; gap: var(--space-1); margin-bottom: var(--space-4); font-size: var(--font-size-caption); }

.detail__ai-card { padding: var(--space-3); background-color: var(--color-surface-container); border-radius: var(--radius-default); }
.detail__confidence { font-size: var(--font-size-caption); color: var(--color-on-surface-variant); margin-bottom: var(--space-2); }
.detail__reasoning { font-size: var(--font-size-caption); line-height: 1.5; }

.detail__actions { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-4); padding: var(--space-3) 0; border-top: 1px solid var(--color-border); }
.detail__actions-label { font-size: var(--font-size-label); color: var(--color-on-surface-variant); text-transform: uppercase; }

.btn--small { padding: var(--space-1) var(--space-3); border-radius: var(--radius-default); font-size: 12px; cursor: pointer; background-color: var(--color-surface-container-high); color: var(--color-on-surface); }
.btn--small:hover { background-color: var(--color-surface-container-highest); }

.audit-timeline { display: flex; flex-direction: column; gap: var(--space-1); }
.audit-entry { padding: var(--space-2); border-left: 2px solid var(--color-outline-variant); font-size: 12px; }
.audit-entry__action { font-weight: var(--font-weight-semibold); }
.audit-entry__time { display: block; color: var(--color-on-surface-variant); font-size: 11px; }
.audit-entry__details { display: block; color: var(--color-on-surface-variant); margin-top: 2px; }
.audit-entry__empty { color: var(--color-on-surface-variant); font-size: var(--font-size-caption); }
</style>
```

- [ ] **Step 7: Update router**

In `src/router/index.ts`, add:

```typescript
const ArticleList = () => import('@/views/article-list.vue');
```

Change articles route:

```typescript
{ path: '/articles', name: 'articles', component: ArticleList },
```

- [ ] **Step 8: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/composables/use-article-search.ts src/components/status-badge.vue src/components/confidence-bar.vue src/components/article-toolbar.vue src/components/article-table.vue src/views/article-list.vue src/router/index.ts
git commit -m "feat(articles): add article list view with search, sort, filter, and detail panel"
```

---

## Task 4: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any issues from article management implementation"
```
