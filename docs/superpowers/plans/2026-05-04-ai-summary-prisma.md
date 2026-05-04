# AI Summary & PRISMA Flow Diagram Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement AI-generated structured summaries of included articles and a PRISMA 2020 flow diagram rendered as SVG with export capabilities.

**Architecture:** The summary engine sends included article data to the LLM with a structured prompt, handling batching for large article sets. The PRISMA module computes flow diagram data from article counts and renders SVG. Both are exposed via Tauri commands.

**Tech Stack:** Rust (rusqlite, reqwest), SVG string generation, Vue 3

**Depends on:** Plan 1, Plan 4 (Criteria & LLM Config), Plan 6 (AI Screening)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── summary/
│   ├── mod.rs              (new: module declarations)
│   ├── engine.rs           (new: summary generation with batching)
│   └── prompt.rs           (new: summary prompt template)
├── prisma/
│   ├── mod.rs              (new: module declarations)
│   ├── data.rs             (new: PRISMA data computation)
│   └── svg.rs              (new: SVG rendering)
├── commands/
│   ├── summary.rs          (new: summary commands)
│   ├── prisma.rs           (new: PRISMA commands)
│   └── mod.rs              (modify: add modules)
├── db/
│   └── summary_repo.rs     (new: store/retrieve summaries)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   ├── summary-view.vue    (new: summary display)
│   └── prisma-diagram.vue  (new: PRISMA SVG view)
├── composables/
│   ├── use-summary.ts      (new: summary composable)
│   └── use-prisma.ts       (new: PRISMA composable)
├── router/
│   └── index.ts            (modify: update routes)
```

---

## Task 1: Summary Prompt & Engine

**Files:**
- Create: `src-tauri/src/summary/mod.rs`
- Create: `src-tauri/src/summary/prompt.rs`
- Create: `src-tauri/src/summary/engine.rs`

- [ ] **Step 1: Create `src-tauri/src/summary/mod.rs`**

```rust
pub mod engine;
pub mod prompt;
```

- [ ] **Step 2: Create `src-tauri/src/summary/prompt.rs`**

```rust
pub const SYSTEM_PROMPT: &str = "You are a systematic literature review assistant. Generate a structured summary of the included articles in a systematic review.";

pub struct SummaryPromptInput {
    pub aims: Vec<String>,
    pub target_length: usize,
    pub articles: Vec<ArticleSummary>,
}

pub struct ArticleSummary {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub abstract_text: String,
    pub ai_reasoning: Option<String>,
}

pub fn build_summary_prompt(input: &SummaryPromptInput) -> String {
    let aims_list = if input.aims.is_empty() {
        "None defined.".to_string()
    } else {
        input.aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a)).collect::<Vec<_>>().join("\n")
    };

    let articles_text = input.articles.iter().map(|a| {
        let year_str = a.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string());
        let reasoning = a.ai_reasoning.as_ref().map(|r| format!("\nAI Reasoning: {}", r)).unwrap_or_default();
        format!("---\nTitle: {}\nAuthors: {}\nYear: {}\nAbstract: {}{}\n---", a.title, a.authors.join("; "), year_str, a.abstract_text, reasoning)
    }).collect::<Vec<_>>().join("\n");

    format!(
        r#"## Task
Generate a structured summary of the included articles in a systematic literature review. Focus on the research aims provided.

## Research Aims
{aims}

## Target Length
Approximately {target_length} words.

## Included Articles
{articles}

## Response Format
Return JSON exactly matching this schema:
{{
  "key_themes": "A paragraph describing the main topics and findings across included studies.",
  "research_trends": "A paragraph describing patterns and directions in the literature vis-a-vis the research aims.",
  "methodological_strengths": "A paragraph describing common robust methodologies observed.",
  "common_weaknesses": "A paragraph describing limitations frequently cited across studies.",
  "gaps_in_literature": "A paragraph describing under-explored areas relative to the research aims."
}}"#,
        aims = aims_list,
        target_length = input.target_length,
        articles = articles_text,
    )
}
```

- [ ] **Step 3: Create `src-tauri/src/summary/engine.rs`**

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::article_repo;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::llm::client;
use crate::screening::token_estimation;
use crate::summary::prompt::{self, ArticleSummary, SummaryPromptInput};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryOutput {
    pub key_themes: String,
    pub research_trends: String,
    pub methodological_strengths: String,
    pub common_weaknesses: String,
    pub gaps_in_literature: String,
}

pub async fn generate_summary(
    conn: Arc<Mutex<Connection>>,
    target_length: usize,
) -> Result<SummaryOutput, AppError> {
    let (config, aims, articles) = {
        let c = conn.lock().await;
        let config = llm_config_repo::get_config(&c)?.ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;
        let aim_list = criteria_repo::get_all_aims(&c)?;
        let aim_texts: Vec<String> = aim_list.iter().map(|a| a.text.clone()).collect();
        let included = article_repo::get_articles_by_status(&c, "included")?;
        let summaries: Vec<ArticleSummary> = included.iter().map(|a| ArticleSummary {
            title: a.title.clone(),
            authors: a.authors.clone(),
            year: a.publication_year,
            abstract_text: a.abstract_text.clone(),
            ai_reasoning: a.ai_reasoning.clone(),
        }).collect();
        (config, aim_texts, summaries)
    };

    if articles.is_empty() {
        return Err(AppError::Validation("No included articles to summarize".to_string()));
    }

    // Check if batching is needed (80% of context window)
    let context_limit = (config.context_window_tokens as f64 * 0.8) as usize;

    // Simple heuristic: estimate tokens for all articles combined
    let total_chars: usize = articles.iter().map(|a| {
        a.title.len() + a.abstract_text.len() + a.authors.join("; ").len() + a.ai_reasoning.as_ref().map(|r| r.len()).unwrap_or(0)
    }).sum();
    let estimated_tokens = total_chars / 4;

    let response = if estimated_tokens > context_limit {
        // Batch: split articles into chunks, summarize each, then synthesize
        let batch_size = (articles.len() / 2).max(1);
        let batch_a = &articles[..batch_size];
        let batch_b = &articles[batch_size..];

        let summary_a = summarize_batch(&config, &aims, target_length / 2, batch_a).await?;
        let summary_b = summarize_batch(&config, &aims, target_length / 2, batch_b).await?;

        // Synthesize
        synthesize_batches(&config, &aims, target_length, &summary_a, &summary_b).await?
    } else {
        summarize_batch(&config, &aims, target_length, &articles).await?
    };

    Ok(response)
}

async fn summarize_batch(
    config: &crate::models::llm_config::LlmConfig,
    aims: &[String],
    target_length: usize,
    articles: &[ArticleSummary],
) -> Result<SummaryOutput, AppError> {
    let input = SummaryPromptInput {
        aims: aims.to_vec(),
        target_length,
        articles: articles.to_vec(),
    };
    let user_prompt = prompt::build_summary_prompt(&input);
    let response = client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &user_prompt).await?;
    parse_summary_response(&response)
}

async fn synthesize_batches(
    config: &crate::models::llm_config::LlmConfig,
    aims: &[String],
    target_length: usize,
    a: &SummaryOutput,
    b: &SummaryOutput,
) -> Result<SummaryOutput, AppError> {
    let synthesis_prompt = format!(
        r#"## Task
Combine two partial summaries into a single coherent summary. Maintain focus on the research aims.

## Research Aims
{aims}

## Target Length
Approximately {target_length} words.

## Partial Summary A
Key Themes: {a_themes}
Research Trends: {a_trends}
Methodological Strengths: {a_methods}
Common Weaknesses: {a_weaknesses}
Gaps in Literature: {a_gaps}

## Partial Summary B
Key Themes: {b_themes}
Research Trends: {b_trends}
Methodological Strengths: {b_methods}
Common Weaknesses: {b_weaknesses}
Gaps in Literature: {b_gaps}

## Response Format
Return JSON exactly matching this schema:
{{
  "key_themes": "...",
  "research_trends": "...",
  "methodological_strengths": "...",
  "common_weaknesses": "...",
  "gaps_in_literature": "..."
}}"#,
        aims = aims.iter().enumerate().map(|(i, a)| format!("{}. {}", i + 1, a)).collect::<Vec<_>>().join("\n"),
        target_length = target_length,
        a_themes = a.key_themes,
        a_trends = a.research_trends,
        a_methods = a.methodological_strengths,
        a_weaknesses = a.common_weaknesses,
        a_gaps = a.gaps_in_literature,
        b_themes = b.key_themes,
        b_trends = b.research_trends,
        b_methods = b.methodological_strengths,
        b_weaknesses = b.common_weaknesses,
        b_gaps = b.gaps_in_literature,
    );

    let response = client::send_chat_completion(config, prompt::SYSTEM_PROMPT, &synthesis_prompt).await?;
    parse_summary_response(&response)
}

fn parse_summary_response(raw: &str) -> Result<SummaryOutput, AppError> {
    let json_str = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    serde_json::from_str::<SummaryOutput>(json_str)
        .map_err(|e| AppError::Import(format!("Failed to parse summary response: {}", e)))
}
```

- [ ] **Step 4: Add `pub mod summary;` and `pub mod prisma;` to `src-tauri/src/lib.rs`**

- [ ] **Step 5: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/summary/ src-tauri/src/lib.rs
git commit -m "feat(summary): add AI summary engine with batching support"
```

---

## Task 2: PRISMA Data & SVG

**Files:**
- Create: `src-tauri/src/prisma/mod.rs`
- Create: `src-tauri/src/prisma/data.rs`
- Create: `src-tauri/src/prisma/svg.rs`

- [ ] **Step 1: Create `src-tauri/src/prisma/mod.rs`**

```rust
pub mod data;
pub mod svg;
```

- [ ] **Step 2: Create `src-tauri/src/prisma/data.rs`**

```rust
use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismaData {
    pub records_identified: usize,
    pub duplicates_removed: usize,
    pub records_screened: usize,
    pub records_excluded: usize,
    pub studies_included: usize,
    pub exclusion_reasons: Vec<ExclusionReason>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionReason {
    pub criterion_id: String,
    pub criterion_text: String,
    pub count: usize,
}

pub fn compute_prisma_data(conn: &Connection) -> Result<PrismaData, AppError> {
    let records_identified: usize = conn
        .query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))
        .unwrap_or(0);

    let duplicates_removed: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE duplicate_of IS NOT NULL", [], |row| row.get(0))
        .unwrap_or(0);

    let records_screened = records_identified.saturating_sub(duplicates_removed);

    let records_excluded: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'rejected'", [], |row| row.get(0))
        .unwrap_or(0);

    let studies_included: usize = conn
        .query_row("SELECT COUNT(*) FROM articles WHERE status = 'included'", [], |row| row.get(0))
        .unwrap_or(0);

    // Exclusion reasons: count articles per matched exclusion criterion
    let mut exclusion_reasons = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT matched_exclusion_criteria FROM articles WHERE status = 'rejected' AND matched_exclusion_criteria IS NOT NULL"
    )?;
    let criterion_counts: std::collections::HashMap<String, usize> = {
        let mut counts = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })?;
        for row in rows {
            if let Ok(json_str) = row {
                if let Ok(ids) = serde_json::from_str::<Vec<String>>(&json_str) {
                    for id in ids {
                        *counts.entry(id).or_insert(0) += 1;
                    }
                }
            }
        }
        counts
    };

    for (criterion_id, count) in criterion_counts {
        let text: String = conn
            .query_row("SELECT text FROM criteria WHERE id = ?1", [&criterion_id], |row| row.get(0))
            .unwrap_or_else(|_| criterion_id.clone());
        exclusion_reasons.push(ExclusionReason {
            criterion_id,
            criterion_text: text,
            count,
        });
    }

    exclusion_reasons.sort_by(|a, b| b.count.cmp(&a.count));

    Ok(PrismaData {
        records_identified,
        duplicates_removed,
        records_screened,
        records_excluded,
        studies_included,
        exclusion_reasons,
    })
}
```

- [ ] **Step 3: Create `src-tauri/src/prisma/svg.rs`**

```rust
use super::data::PrismaData;

pub fn render_prisma_svg(data: &PrismaData) -> String {
    let width = 600;
    let box_w = 280;
    let box_h = 50;
    let x_center = width / 2;
    let x_box = x_center - box_w / 2;

    let mut y = 40;

    // Phase 1: Identification
    let identification_svg = render_box(x_box, y, box_w, box_h, &format!("Records identified (n = {})", data.records_identified));
    y += box_h + 15;
    let arrow1 = render_arrow(x_center, y - 15, x_center, y);
    let dup_svg = render_side_box(x_box + box_w + 20, y - box_h - 15, 200, box_h, &format!("Duplicates removed (n = {})", data.duplicates_removed));
    y += 15;

    // Phase 2: Screening
    let screening_svg = render_box(x_box, y, box_w, box_h, &format!("Records screened (n = {})", data.records_screened));
    y += box_h + 15;
    let arrow2 = render_arrow(x_center, y - 15, x_center, y);
    let excluded_svg = render_side_box(x_box + box_w + 20, y - box_h - 15, 200, box_h, &format!("Records excluded (n = {})", data.records_excluded));
    y += 15;

    // Phase 3: Eligibility (same as screening for abstract-only)
    let eligibility_svg = render_box(x_box, y, box_w, box_h, &format!("Articles assessed (n = {})", data.records_screened));
    y += box_h + 15;
    let arrow3 = render_arrow(x_center, y - 15, x_center, y);
    y += 15;

    // Phase 4: Included
    let included_svg = render_box(x_box, y, box_w, box_h, &format!("Studies included (n = {})", data.studies_included));

    let height = y + box_h + 40;

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="Inter, system-ui, sans-serif">
  <rect width="{width}" height="{height}" fill="#ffffff"/>
  {identification_svg}
  {arrow1}
  {dup_svg}
  {screening_svg}
  {arrow2}
  {excluded_svg}
  {eligibility_svg}
  {arrow3}
  {included_svg}
</svg>"#,
        width = width,
        height = height,
        identification_svg = identification_svg,
        arrow1 = arrow1,
        dup_svg = dup_svg,
        screening_svg = screening_svg,
        arrow2 = arrow2,
        excluded_svg = excluded_svg,
        eligibility_svg = eligibility_svg,
        arrow3 = arrow3,
        included_svg = included_svg,
    )
}

fn render_box(x: i32, y: i32, w: i32, h: i32, text: &str) -> String {
    let text_x = x + w / 2;
    let text_y = y + h / 2 + 5;
    format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#f0ecf9" stroke="#4f46e5" stroke-width="1.5"/>
  <text x="{text_x}" y="{text_y}" text-anchor="middle" font-size="13" font-weight="600" fill="#1b1b24">{text}</text>"#,
        x = x, y = y, w = w, h = h, text_x = text_x, text_y = text_y, text = escape_xml(text),
    )
}

fn render_side_box(x: i32, y: i32, w: i32, h: i32, text: &str) -> String {
    let text_x = x + w / 2;
    let text_y = y + h / 2 + 5;
    format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="#fee2e2" stroke="#ef4444" stroke-width="1"/>
  <text x="{text_x}" y="{text_y}" text-anchor="middle" font-size="12" fill="#991b1b">{text}</text>"#,
        x = x, y = y, w = w, h = h, text_x = text_x, text_y = text_y, text = escape_xml(text),
    )
}

fn render_arrow(x1: i32, y1: i32, x2: i32, y2: i32) -> String {
    format!(
        r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#777587" stroke-width="1.5"/>
  <polygon points="{x2},{y2} {x2-4},{y2-8} {x2+4},{y2-8}" fill="#777587"/>"#,
        x1 = x1, y1 = y1, x2 = x2, y2 = y2,
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
```

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/prisma/
git commit -m "feat(prisma): add PRISMA 2020 data computation and SVG rendering"
```

- [ ] **Write PRISMA data accuracy tests**

Create `src-tauri/tests/prisma_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::prisma::data::compute_prisma_data;

#[test]
fn test_prisma_counts_from_empty_database() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    let data = compute_prisma_data(&conn).unwrap();
    assert_eq!(data.records_identified, 0);
    assert_eq!(data.duplicates_removed, 0);
    assert_eq!(data.records_screened, 0);
    assert_eq!(data.records_excluded, 0);
    assert_eq!(data.studies_included, 0);
}

#[test]
fn test_prisma_counts_with_articles() {
    let conn = create_connection().unwrap();
    run_migrations(&conn).unwrap();

    // Insert articles in various states
    conn.execute("INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a1', 'imported', 'T1', 'A1', '[]')", []).unwrap();
    conn.execute("INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a2', 'working', 'T2', 'A2', '[]')", []).unwrap();
    conn.execute("INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a3', 'included', 'T3', 'A3', '[]')", []).unwrap();
    conn.execute("INSERT INTO articles (id, status, title, abstract_text, authors) VALUES ('a4', 'rejected', 'T4', 'A4', '[]')", []).unwrap();
    conn.execute("INSERT INTO articles (id, status, title, abstract_text, authors, duplicate_of) VALUES ('a5', 'imported', 'T5', 'A5', '[]', 'a1')", []).unwrap();

    let data = compute_prisma_data(&conn).unwrap();
    assert_eq!(data.records_identified, 5); // All articles
    assert_eq!(data.duplicates_removed, 1); // a5 has duplicate_of
    assert_eq!(data.studies_included, 1); // Only a3
    assert_eq!(data.records_excluded, 1); // Only a4
}
```

Run: `cd src-tauri && cargo test prisma_test --test prisma_test`
Expected: PASS

```bash
git add src-tauri/tests/prisma_test.rs
git commit -m "test(prisma): add PRISMA data accuracy tests"
```

---

## Task 3: Summary & PRISMA Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/summary.rs`
- Create: `src-tauri/src/commands/prisma.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/summary.rs`**

```rust
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::summary::engine::{self, SummaryOutput};

#[tauri::command]
pub async fn generate_summary(db_state: State<'_, DbState>, targetLength: Option<usize>) -> Result<SummaryOutput, AppError> {
    let conn = Arc::new(db_state.conn.clone());
    let length = targetLength.unwrap_or(1000);
    engine::generate_summary(conn, length).await
}
```

- [ ] **Step 2: Create `src-tauri/src/commands/prisma.rs`**

```rust
use tauri::State;

use crate::db::connection::DbState;
use crate::error::AppError;
use crate::prisma::data::{self, PrismaData};
use crate::prisma::svg;

#[tauri::command]
pub fn get_prisma_data(db_state: State<'_, DbState>) -> Result<PrismaData, AppError> {
    let conn = db_state.conn.lock().map_err(|e| crate::error::AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    data::compute_prisma_data(&conn)
}

#[tauri::command]
pub fn get_prisma_svg(db_state: State<'_, DbState>) -> Result<String, AppError> {
    let conn = db_state.conn.lock().map_err(|e| crate::error::AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    let prisma_data = data::compute_prisma_data(&conn)?;
    Ok(svg::render_prisma_svg(&prisma_data))
}
```

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` — add `pub mod summary;` and `pub mod prisma;`**

- [ ] **Step 4: Update `src-tauri/src/lib.rs` invoke handler**

Add:

```rust
commands::summary::generate_summary,
commands::prisma::get_prisma_data,
commands::prisma::get_prisma_svg,
```

- [ ] **Step 5: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/summary.rs src-tauri/src/commands/prisma.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(summary-prisma): add Tauri commands for summary generation and PRISMA diagram"
```

---

## Task 4: Frontend Summary & PRISMA Views

**Files:**
- Create: `src/composables/use-summary.ts`
- Create: `src/composables/use-prisma.ts`
- Create: `src/views/summary-view.vue`
- Create: `src/views/prisma-diagram.vue`
- Modify: `src/router/index.ts`

> **Design reference:** Before implementing, read `docs/design-reference/09-prisma-diagram.html` and `docs/design-reference/09-prisma-diagram.png`. Extract the exact layout structure, spacing, and component hierarchy from the Stitch HTML. Implement only v3-scoped elements per `docs/design-reference/00-design-patterns.md` Section 14.

- [ ] **Step 1: Create `src/composables/use-summary.ts`**

```typescript
import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface SummaryOutput {
  keyThemes: string;
  researchTrends: string;
  methodologicalStrengths: string;
  commonWeaknesses: string;
  gapsInLiterature: string;
}

export function useSummary() {
  const summary = ref<SummaryOutput | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function generate(targetLength = 1000): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      summary.value = await tauriCommand<SummaryOutput>('generate_summary', { targetLength });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { summary, loading, error, generate };
}
```

- [ ] **Step 2: Create `src/composables/use-prisma.ts`**

```typescript
import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface PrismaData {
  recordsIdentified: number;
  duplicatesRemoved: number;
  recordsScreened: number;
  recordsExcluded: number;
  studiesIncluded: number;
  exclusionReasons: ExclusionReason[];
}

export interface ExclusionReason {
  criterionId: string;
  criterionText: string;
  count: number;
}

export function usePrisma() {
  const svgContent = ref<string | null>(null);
  const data = ref<PrismaData | null>(null);
  const loading = ref(false);

  async function loadDiagram(): Promise<void> {
    loading.value = true;
    try {
      const [svg, prismaData] = await Promise.all([
        tauriCommand<string>('get_prisma_svg'),
        tauriCommand<PrismaData>('get_prisma_data'),
      ]);
      svgContent.value = svg;
      data.value = prismaData;
    } finally {
      loading.value = false;
    }
  }

  async function exportSvg(): Promise<void> {
    if (!svgContent.value) return;
    const blob = new Blob([svgContent.value], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'prisma-flow-diagram.svg';
    a.click();
    URL.revokeObjectURL(url);
  }

  async function exportPng(): Promise<void> {
    if (!svgContent.value) return;
    const img = new Image();
    const blob = new Blob([svgContent.value], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    img.onload = () => {
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth * 2;
      canvas.height = img.naturalHeight * 2;
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        canvas.toBlob((pngBlob) => {
          if (pngBlob) {
            const pngUrl = URL.createObjectURL(pngBlob);
            const a = document.createElement('a');
            a.href = pngUrl;
            a.download = 'prisma-flow-diagram.png';
            a.click();
            URL.revokeObjectURL(pngUrl);
          }
        });
      }
      URL.revokeObjectURL(url);
    };
    img.src = url;
  }

  return { svgContent, data, loading, loadDiagram, exportSvg, exportPng };
}
```

- [ ] **Step 3: Create `src/views/summary-view.vue`**

```vue
<script setup lang="ts">
import { useSummary } from '@/composables/use-summary';

const { summary, loading, error, generate } = useSummary();
</script>

<template>
  <div class="summary-view">
    <div class="summary-view__header">
      <h1>AI Summary</h1>
      <button class="btn btn--primary" :disabled="loading" @click="generate()">
        {{ loading ? 'Generating...' : 'Generate Summary' }}
      </button>
    </div>

    <div v-if="error" class="summary-view__error">{{ error }}</div>

    <div v-if="summary" class="summary-view__content">
      <section class="summary-section">
        <h2>Key Themes</h2>
        <p>{{ summary.keyThemes }}</p>
      </section>
      <section class="summary-section">
        <h2>Research Trends</h2>
        <p>{{ summary.researchTrends }}</p>
      </section>
      <section class="summary-section">
        <h2>Methodological Strengths</h2>
        <p>{{ summary.methodologicalStrengths }}</p>
      </section>
      <section class="summary-section">
        <h2>Common Weaknesses</h2>
        <p>{{ summary.commonWeaknesses }}</p>
      </section>
      <section class="summary-section">
        <h2>Gaps in Literature</h2>
        <p>{{ summary.gapsInLiterature }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.summary-view { padding: var(--space-6); max-width: 800px; }
.summary-view__header { display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-6); }
.summary-view__header h1 { font-size: var(--font-size-display); font-weight: var(--font-weight-semibold); letter-spacing: var(--letter-spacing-display); }
.summary-view__error { padding: var(--space-3); background-color: var(--color-error-container); color: var(--color-error); border-radius: var(--radius-default); margin-bottom: var(--space-4); }
.summary-view__content { display: flex; flex-direction: column; gap: var(--space-4); }
.summary-section { border: 1px solid var(--color-border); border-radius: var(--radius-default); padding: var(--space-4); }
.summary-section h2 { font-size: var(--font-size-h2); margin-bottom: var(--space-2); }
.summary-section p { font-size: var(--font-size-caption); line-height: 1.6; }
.btn { padding: var(--space-2) var(--space-4); border-radius: var(--radius-default); font-size: var(--font-size-caption); font-weight: var(--font-weight-semibold); cursor: pointer; }
.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

- [ ] **Step 4: Create `src/views/prisma-diagram.vue`**

```vue
<script setup lang="ts">
import { onMounted } from 'vue';
import { usePrisma } from '@/composables/use-prisma';

const { svgContent, data, loading, loadDiagram, exportSvg, exportPng } = usePrisma();

onMounted(loadDiagram);
</script>

<template>
  <div class="prisma-view">
    <div class="prisma-view__header">
      <h1>PRISMA 2020 Flow Diagram</h1>
      <div class="prisma-view__actions">
        <button class="btn btn--secondary" @click="exportSvg">Export SVG</button>
        <button class="btn btn--secondary" @click="exportPng">Export PNG</button>
        <button class="btn btn--primary" :disabled="loading" @click="loadDiagram">Refresh</button>
      </div>
    </div>

    <div v-if="svgContent" class="prisma-view__diagram" v-html="svgContent" />
    <div v-if="data" class="prisma-view__summary">
      <span>{{ data.recordsIdentified }} identified → {{ data.duplicatesRemoved }} duplicates removed → {{ data.recordsScreened }} screened → {{ data.recordsExcluded }} excluded → {{ data.studiesIncluded }} included</span>
    </div>
  </div>
</template>

<style scoped>
.prisma-view { padding: var(--space-6); }
.prisma-view__header { display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-6); }
.prisma-view__header h1 { font-size: var(--font-size-display); font-weight: var(--font-weight-semibold); letter-spacing: var(--letter-spacing-display); }
.prisma-view__actions { display: flex; gap: var(--space-2); }
.prisma-view__diagram { display: flex; justify-content: center; padding: var(--space-4); background-color: white; border: 1px solid var(--color-border); border-radius: var(--radius-default); }
.prisma-view__summary { margin-top: var(--space-4); font-size: var(--font-size-caption); color: var(--color-on-surface-variant); text-align: center; }
.btn { padding: var(--space-2) var(--space-3); border-radius: var(--radius-default); font-size: var(--font-size-caption); font-weight: var(--font-weight-semibold); cursor: pointer; }
.btn--primary { background-color: var(--color-primary); color: var(--color-on-primary); }
.btn--secondary { background-color: var(--color-surface-container-high); color: var(--color-on-surface); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

- [ ] **Step 5: Update router**

In `src/router/index.ts`:

```typescript
const SummaryView = () => import('@/views/summary-view.vue');
const PrismaDiagram = () => import('@/views/prisma-diagram.vue');
```

Add routes (replacing placeholder):

```typescript
{ path: '/summary', name: 'summary', component: SummaryView },
{ path: '/prisma', name: 'prisma', component: PrismaDiagram },
```

- [ ] **Step 6: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/composables/use-summary.ts src/composables/use-prisma.ts src/views/summary-view.vue src/views/prisma-diagram.vue src/router/index.ts
git commit -m "feat(summary-prisma): add summary display and PRISMA diagram views"
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
git commit -m "chore: fix any issues from summary and PRISMA implementation"
```
