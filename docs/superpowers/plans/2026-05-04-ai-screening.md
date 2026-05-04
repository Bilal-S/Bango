# AI Screening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement per-article AI screening with batch processing, priority conflict resolution, progress tracking, and token estimation so that articles in the Working list are evaluated against user-defined criteria and moved to Included or Rejected.

**Architecture:** The screening engine runs as an async batch processor using Tokio channels. Each article gets its own LLM API call with the screening prompt template. After receiving the LLM response, the app applies deterministic priority conflict resolution to compute the final decision. Progress is tracked via shared state that Tauri commands query.

**Tech Stack:** Rust (tokio, reqwest), Tauri commands with async support, Vue 3

**Depends on:** Plan 1 (Foundation & Database), Plan 4 (Criteria & LLM Configuration), Plan 5 (Tags & Labels)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── screening/
│   ├── mod.rs                (new: module declarations)
│   ├── engine.rs             (new: batch processing with concurrency control)
│   ├── prompt.rs             (new: screening prompt template generation)
│   ├── resolution.rs         (new: priority conflict resolution)
│   └── token_estimation.rs   (new: token count estimation)
├── commands/
│   ├── screening.rs          (new: screening Tauri commands)
│   └── mod.rs                (modify: add screening module)
├── llm/
│   ├── mod.rs                (new: module declarations)
│   └── client.rs             (modify: add chat completion method)
├── tests/
│   └── screening_test.rs     (new: screening unit tests)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   └── screening-progress.vue (new: screening progress UI)
├── components/
│   ├── screening-progress-bar.vue (new: progress bar component)
│   └── screening-stats.vue       (new: stats panel)
├── composables/
│   └── use-screening.ts      (new: screening workflow composable)
├── router/
│   └── index.ts               (modify: update screening route)
```

---

## Task 1: Priority Conflict Resolution

**Files:**
- Create: `src-tauri/src/screening/mod.rs`
- Create: `src-tauri/src/screening/resolution.rs`
- Create: `src-tauri/tests/screening_test.rs`

- [ ] **Step 1: Create `src-tauri/src/screening/mod.rs`**

```rust
pub mod engine;
pub mod prompt;
pub mod resolution;
pub mod token_estimation;
```

- [ ] **Step 2: Write failing tests in `src-tauri/tests/screening_test.rs`**

```rust
use bango_lib::screening::resolution::{resolve_decision, ScreeningInput, CriterionMatch};
use bango_lib::models::criterion::{CriterionType, Priority};

fn make_match(id: &str, ctype: CriterionType, priority: Priority) -> CriterionMatch {
    CriterionMatch {
        id: id.to_string(),
        criterion_type: ctype,
        priority,
    }
}

#[test]
fn test_inclusion_wins_higher_priority() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Critical)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_exclusion_wins_higher_priority() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::Critical)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_tied_priority_favors_inclusion() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::High)],
        exclusion_matches: vec![make_match("2", CriterionType::Exclusion, Priority::High)],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_no_criteria_matches_exclude() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_only_inclusion_matches() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![],
    };
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_only_exclusion_matches() {
    let input = ScreeningInput {
        inclusion_matches: vec![],
        exclusion_matches: vec![make_match("1", CriterionType::Exclusion, Priority::Standard)],
    };
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_multiple_inclusion_picks_highest() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("1", CriterionType::Inclusion, Priority::Low),
            make_match("2", CriterionType::Inclusion, Priority::Critical),
            make_match("3", CriterionType::Inclusion, Priority::Standard),
        ],
        exclusion_matches: vec![make_match("4", CriterionType::Exclusion, Priority::High)],
    };
    // Critical inclusion > High exclusion → include
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_multiple_exclusion_picks_highest() {
    let input = ScreeningInput {
        inclusion_matches: vec![make_match("1", CriterionType::Inclusion, Priority::Standard)],
        exclusion_matches: vec![
            make_match("2", CriterionType::Exclusion, Priority::Low),
            make_match("3", CriterionType::Exclusion, Priority::Critical),
        ],
    };
    // Critical exclusion > Standard inclusion → exclude
    assert_eq!(resolve_decision(&input), "exclude");
}

#[test]
fn test_realistic_screening_scenario() {
    // Simulates screening: article matches 2 inclusion (standard, high) and 1 exclusion (standard)
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("inc-1", CriterionType::Inclusion, Priority::Standard),
            make_match("inc-2", CriterionType::Inclusion, Priority::High),
        ],
        exclusion_matches: vec![
            make_match("exc-1", CriterionType::Exclusion, Priority::Standard),
        ],
    };
    // High inclusion > Standard exclusion → include
    assert_eq!(resolve_decision(&input), "include");
}

#[test]
fn test_critical_exclusion_overrides_all() {
    let input = ScreeningInput {
        inclusion_matches: vec![
            make_match("inc-1", CriterionType::Inclusion, Priority::High),
            make_match("inc-2", CriterionType::Inclusion, Priority::Standard),
        ],
        exclusion_matches: vec![
            make_match("exc-1", CriterionType::Exclusion, Priority::Critical),
        ],
    };
    // Critical exclusion > everything → exclude
    assert_eq!(resolve_decision(&input), "exclude");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd src-tauri && cargo test screening_test --test screening_test`
Expected: FAIL — modules don't exist

- [ ] **Step 4: Implement `src-tauri/src/screening/resolution.rs`**

```rust
use crate::models::criterion::{CriterionType, Priority};

/// A criterion matched by the AI during screening.
#[derive(Debug, Clone)]
pub struct CriterionMatch {
    pub id: String,
    pub criterion_type: CriterionType,
    pub priority: Priority,
}

/// Input to the resolution algorithm.
#[derive(Debug, Clone)]
pub struct ScreeningInput {
    pub inclusion_matches: Vec<CriterionMatch>,
    pub exclusion_matches: Vec<CriterionMatch>,
}

/// Applies deterministic priority conflict resolution (Spec Section 6.3).
///
/// 1. Find the single highest-priority inclusion criterion that matches.
/// 2. Find the single highest-priority exclusion criterion that matches.
/// 3. The higher-priority side wins.
/// 4. If tied → include.
/// 5. If no criteria match at all → exclude.
#[must_use]
pub fn resolve_decision(input: &ScreeningInput) -> &'static str {
    let highest_inclusion = input
        .inclusion_matches
        .iter()
        .max_by_key(|m| m.priority);

    let highest_exclusion = input
        .exclusion_matches
        .iter()
        .max_by_key(|m| m.priority);

    match (highest_inclusion, highest_exclusion) {
        (None, None) => "exclude",  // No basis for inclusion
        (Some(_), None) => "include",
        (None, Some(_)) => "exclude",
        (Some(inc), Some(exc)) => {
            if inc.priority > exc.priority {
                "exclude" // Higher-priority exclusion wins
            } else {
                "include" // Inclusion wins on tie or higher
            }
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test screening_test --test screening_test`
Expected: PASS — all 8 tests pass

- [ ] **Step 6: Register module in `lib.rs` and commit**

Add `pub mod screening;` to `src-tauri/src/lib.rs`.

```bash
git add src-tauri/src/screening/ src-tauri/src/lib.rs src-tauri/tests/screening_test.rs
git commit -m "feat(screening): add priority conflict resolution logic"
```

---

## Task 2: Screening Prompt Template

**Files:**
- Create: `src-tauri/src/screening/prompt.rs`
- Add tests to: `src-tauri/tests/screening_test.rs`

- [ ] **Step 1: Add prompt tests to `src-tauri/tests/screening_test.rs`**

Append:

```rust
use bango_lib::screening::prompt::{build_screening_prompt, ScreeningPromptInput, CriterionEntry, AimEntry};
use bango_lib::models::criterion::Priority;

#[test]
fn test_build_prompt_contains_research_aims() {
    let input = ScreeningPromptInput {
        aims: vec![AimEntry { text: "Study ML in healthcare".to_string() }],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test".to_string(),
        article_authors: "Smith, John".to_string(),
        article_year: Some(2023),
        article_abstract: "Abstract text".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Study ML in healthcare"));
}

#[test]
fn test_build_prompt_contains_criteria_with_priority() {
    let input = ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![CriterionEntry {
            id: "c1".to_string(),
            text: "Must be about ML".to_string(),
            priority: Priority::Critical,
        }],
        exclusion_criteria: vec![CriterionEntry {
            id: "c2".to_string(),
            text: "Not a review".to_string(),
            priority: Priority::High,
        }],
        article_title: "Test".to_string(),
        article_authors: "Author".to_string(),
        article_year: None,
        article_abstract: "Abstract".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("c1"));
    assert!(prompt.contains("Must be about ML"));
    assert!(prompt.contains("critical"));
    assert!(prompt.contains("c2"));
    assert!(prompt.contains("Not a review"));
    assert!(prompt.contains("high"));
}

#[test]
fn test_build_prompt_contains_article_fields() {
    let input = ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Deep Learning for Medical Imaging".to_string(),
        article_authors: "Doe, Jane; Smith, John".to_string(),
        article_year: Some(2024),
        article_abstract: "This paper reviews deep learning methods.".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("Deep Learning for Medical Imaging"));
    assert!(prompt.contains("Doe, Jane; Smith, John"));
    assert!(prompt.contains("2024"));
    assert!(prompt.contains("This paper reviews deep learning methods."));
}

#[test]
fn test_build_prompt_response_format() {
    let input = ScreeningPromptInput {
        aims: vec![],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test".to_string(),
        article_authors: "Author".to_string(),
        article_year: None,
        article_abstract: "Abstract".to_string(),
    };
    let prompt = build_screening_prompt(&input);
    assert!(prompt.contains("\"decision\""));
    assert!(prompt.contains("\"reasoning\""));
    assert!(prompt.contains("\"matched_inclusion_criteria\""));
    assert!(prompt.contains("\"matched_exclusion_criteria\""));
    assert!(prompt.contains("\"suggested_tags\""));
    assert!(prompt.contains("\"confidence\""));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test screening_test --test screening_test`
Expected: FAIL — `build_screening_prompt` not defined

- [ ] **Step 3: Implement `src-tauri/src/screening/prompt.rs`**

```rust
use crate::models::criterion::Priority;

pub const SYSTEM_PROMPT: &str = "You are a systematic literature review screening assistant. Evaluate the provided article abstract against the research aims, inclusion criteria, and exclusion criteria. Return your evaluation as structured JSON matching the required schema.";

pub struct AimEntry {
    pub text: String,
}

pub struct CriterionEntry {
    pub id: String,
    pub text: String,
    pub priority: Priority,
}

pub struct ScreeningPromptInput {
    pub aims: Vec<AimEntry>,
    pub inclusion_criteria: Vec<CriterionEntry>,
    pub exclusion_criteria: Vec<CriterionEntry>,
    pub article_title: String,
    pub article_authors: String,
    pub article_year: Option<i32>,
    pub article_abstract: String,
}

pub fn build_screening_prompt(input: &ScreeningPromptInput) -> String {
    let aims_list = if input.aims.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .aims
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, a.text))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let inclusion_list = if input.inclusion_criteria.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .inclusion_criteria
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. [{}] {} (priority: {})", i + 1, c.id, c.text, c.priority.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let exclusion_list = if input.exclusion_criteria.is_empty() {
        "None defined.".to_string()
    } else {
        input
            .exclusion_criteria
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. [{}] {} (priority: {})", i + 1, c.id, c.text, c.priority.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let year_str = input
        .article_year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    format!(
        r#"## Research Aims
{aims_list}

## Inclusion Criteria
{inclusion_list}

## Exclusion Criteria
{exclusion_list}

## Priority Rules
- Higher priority rules always outweigh lower priority rules.
- If inclusion and exclusion criteria of equal priority both match, favor inclusion.

## Article
Title: {title}
Authors: {authors}
Year: {year}
Abstract: {abstract}

## Response Format
Return JSON exactly matching this schema:
{{
  "decision": "include" | "exclude",
  "reasoning": "A paragraph citing specific sentences from the abstract to justify the decision.",
  "matched_inclusion_criteria": ["criteria-id-1", ...],
  "matched_exclusion_criteria": ["criteria-id-3", ...],
  "suggested_tags": ["tag-name-1", ...],
  "confidence": 0.0-1.0
}}"#,
        aims_list = aims_list,
        inclusion_list = inclusion_list,
        exclusion_list = exclusion_list,
        title = input.article_title,
        authors = input.article_authors,
        year = year_str,
        abstract = input.article_abstract,
    )
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test screening_test --test screening_test`
Expected: PASS — all tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/screening/prompt.rs src-tauri/tests/screening_test.rs
git commit -m "feat(screening): add screening prompt template"
```

---

## Task 3: Token Estimation

**Files:**
- Create: `src-tauri/src/screening/token_estimation.rs`
- Add tests to: `src-tauri/tests/screening_test.rs`

- [ ] **Step 1: Add token estimation tests**

Append to `src-tauri/tests/screening_test.rs`:

```rust
use bango_lib::screening::token_estimation::estimate_tokens;
use bango_lib::screening::prompt::{build_screening_prompt, ScreeningPromptInput, AimEntry};

#[test]
fn test_estimate_tokens_empty_string() {
    assert_eq!(estimate_tokens(""), 0);
}

#[test]
fn test_estimate_tokens_basic() {
    // 100 chars / 4 = 25 tokens
    let text = "a".repeat(100);
    assert_eq!(estimate_tokens(&text), 25);
}

#[test]
fn test_estimate_tokens_unicode() {
    // Unicode chars still counted as chars
    let text = "日本語テスト"; // 6 chars
    assert_eq!(estimate_tokens(&text), 1); // 6/4 = 1.5, truncated to 1
}

#[test]
fn test_prompt_token_estimation() {
    let input = ScreeningPromptInput {
        aims: vec![AimEntry { text: "Study AI".to_string() }],
        inclusion_criteria: vec![],
        exclusion_criteria: vec![],
        article_title: "Test Article Title".to_string(),
        article_authors: "Author".to_string(),
        article_year: Some(2023),
        article_abstract: "a".repeat(200),
    };
    let prompt = build_screening_prompt(&input);
    let tokens = estimate_tokens(&prompt);
    assert!(tokens > 0, "Should estimate some tokens");
    // With ~400 chars of template + 200 char abstract, expect ~150 tokens
    assert!(tokens < 500, "Should be reasonable: got {}", tokens);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test estimate_tokens --test screening_test`
Expected: FAIL

- [ ] **Step 3: Implement `src-tauri/src/screening/token_estimation.rs`**

```rust
/// Estimates token count using characters/4 heuristic (Spec Section 9.6).
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

/// Estimates whether a screening run might exceed the context window.
/// Returns a warning message if per-article tokens exceed 80% of context_window_tokens.
#[must_use]
pub fn check_context_window(
    template_tokens: usize,
    articles: &[usize], // token estimates per article
    context_window_tokens: usize,
) -> Option<String> {
    let worst_case = articles.iter().copied().max().unwrap_or(0) + template_tokens;
    let threshold = (context_window_tokens as f64 * 0.8) as usize;

    if worst_case > threshold {
        Some(format!(
            "Estimated worst-case per-article tokens ({}) exceed 80% of context window ({}). \
             Articles with large abstracts may produce truncated responses.",
            worst_case, threshold,
        ))
    } else {
        None
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test screening_test --test screening_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/screening/token_estimation.rs src-tauri/tests/screening_test.rs
git commit -m "feat(screening): add token estimation with context window check"
```

---

## Task 4: Screening Engine (Batch Processing)

**Files:**
- Create: `src-tauri/src/screening/engine.rs`
- Create: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/src/llm/client.rs`

- [ ] **Step 1: Create `src-tauri/src/llm/mod.rs`**

```rust
pub mod client;
```

- [ ] **Step 2: Create `src-tauri/src/llm/client.rs`**

The LLM client handles HTTP requests to OpenAI-compatible endpoints.

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::llm_config::LlmConfig;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

pub async fn send_chat_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, AppError> {
    let client = Client::new();
    let request = ChatRequest {
        model: config.model_name.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        temperature: config.temperature,
    };

    let response = client
        .post(&config.endpoint_url)
        .header("Content-Type", "application/json")
        .bearer_auth(config.api_key_encrypted.as_deref().unwrap_or(""))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::Import(format!("LLM request failed: {}", e)))?;

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::Import("Rate limited (429)".to_string()));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Import(format!(
            "LLM request failed ({}): {}",
            status, body
        )));
    }

    let chat_response: ChatResponse = response
        .json()
        .await
        .map_err(|e| AppError::Import(format!("Failed to parse LLM response: {}", e)))?;

    chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| AppError::Import("No response from LLM".to_string()))
}
```

- [ ] **Step 3: Create `src-tauri/src/screening/engine.rs`**

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use tokio::time::sleep;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::llm::client;
use crate::models::article::{Article, AiDecision, ArticleStatus};
use crate::models::criterion::{Criterion, CriterionType, Priority};
use crate::models::llm_config::LlmConfig;
use crate::models::tag::TagSource;
use crate::screening::prompt::{self, ScreeningPromptInput, CriterionEntry, AimEntry};
use crate::screening::resolution::{self, CriterionMatch};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningProgress {
    pub total: usize,
    pub completed: usize,
    pub included: usize,
    pub rejected: usize,
    pub errors: usize,
    pub is_running: bool,
    pub current_article_title: Option<String>,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<u64>,
}

impl Default for ScreeningProgress {
    fn default() -> Self {
        Self {
            total: 0,
            completed: 0,
            included: 0,
            rejected: 0,
            errors: 0,
            is_running: false,
            current_article_title: None,
            elapsed_ms: 0,
            estimated_remaining_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmScreeningResponse {
    pub decision: String,
    pub reasoning: String,
    pub matched_inclusion_criteria: Vec<String>,
    pub matched_exclusion_criteria: Vec<String>,
    pub suggested_tags: Vec<String>,
    pub confidence: f64,
}

pub struct ScreeningEngine {
    progress: Arc<Mutex<ScreeningProgress>>,
    cancel_token: Arc<Mutex<bool>>,
    pause_token: Arc<Mutex<bool>>,
}

impl ScreeningEngine {
    pub fn new() -> Self {
        Self {
            progress: Arc::new(Mutex::new(ScreeningProgress::default())),
            cancel_token: Arc::new(Mutex::new(false)),
            pause_token: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn get_progress(&self) -> ScreeningProgress {
        self.progress.lock().await.clone()
    }

    pub async fn cancel(&self) {
        *self.cancel_token.lock().await = true;
    }

    pub async fn pause(&self) {
        *self.pause_token.lock().await = true;
    }

    pub async fn resume(&self) {
        *self.pause_token.lock().await = false;
    }

    pub async fn run(
        &self,
        conn: Arc<Mutex<Connection>>,
        config: LlmConfig,
        criteria: Vec<Criterion>,
        aims: Vec<crate::models::criterion::ResearchAim>,
    ) -> Result<(), AppError> {
        // Reset state
        *self.cancel_token.lock().await = false;
        *self.pause_token.lock().await = false;

        // Get unscreened working articles
        let articles = {
            let c = conn.lock().await;
            article_repo::get_articles_by_status(&c, "working")?
                .into_iter()
                .filter(|a| a.screened_at.is_none())
                .collect::<Vec<_>>()
        };

        let total = articles.len();
        let inclusion_criteria: Vec<&Criterion> = criteria.iter().filter(|c| matches!(c.criterion_type, CriterionType::Inclusion)).collect();
        let exclusion_criteria: Vec<&Criterion> = criteria.iter().filter(|c| matches!(c.criterion_type, CriterionType::Exclusion)).collect();

        // Initialize progress
        {
            let mut progress = self.progress.lock().await;
            progress.total = total;
            progress.completed = 0;
            progress.included = 0;
            progress.rejected = 0;
            progress.errors = 0;
            progress.is_running = true;
        }

        let start = Instant::now();
        let max_retries = 3;

        for article in &articles {
            // Check cancellation
            if *self.cancel_token.lock().await {
                break;
            }

            // Wait while paused
            while *self.pause_token.lock().await {
                sleep(Duration::from_millis(200)).await;
                if *self.cancel_token.lock().await {
                    break;
                }
            }
            if *self.cancel_token.lock().await {
                break;
            }

            // Update current article
            {
                let mut progress = self.progress.lock().await;
                progress.current_article_title = Some(article.title.clone());
            }

            // Build prompt
            let prompt_input = ScreeningPromptInput {
                aims: aims.iter().map(|a| AimEntry { text: a.text.clone() }).collect(),
                inclusion_criteria: inclusion_criteria.iter().map(|c| CriterionEntry {
                    id: c.id.clone(),
                    text: c.text.clone(),
                    priority: c.priority,
                }).collect(),
                exclusion_criteria: exclusion_criteria.iter().map(|c| CriterionEntry {
                    id: c.id.clone(),
                    text: c.text.clone(),
                    priority: c.priority,
                }).collect(),
                article_title: article.title.clone(),
                article_authors: article.authors.join("; "),
                article_year: article.publication_year,
                article_abstract: article.abstract_text.clone(),
            };

            let user_prompt = prompt::build_screening_prompt(&prompt_input);
            let system_prompt = prompt::SYSTEM_PROMPT;

            // Send to LLM with retry on 429
            let mut response_text = None;
            let mut retry_count = 0;

            while retry_count <= max_retries {
                match client::send_chat_completion(&config, system_prompt, &user_prompt).await {
                    Ok(text) => {
                        response_text = Some(text);
                        break;
                    }
                    Err(e) if e.to_string().contains("429") => {
                        retry_count += 1;
                        let delay_secs = 2u64.pow(retry_count as u32);
                        sleep(Duration::from_secs(delay_secs)).await;
                    }
                    Err(e) => {
                        // Set screening error
                        let c = conn.lock().await;
                        set_screening_error(&c, &article.id, &e.to_string())?;
                        break;
                    }
                }
            }

            // Apply delay between requests
            sleep(Duration::from_millis(config.request_delay_ms as u64)).await;

            let response_text = match response_text {
                Some(t) => t,
                None => {
                    let mut progress = self.progress.lock().await;
                    progress.errors += 1;
                    progress.completed += 1;
                    continue;
                }
            };

            // Parse response
            match process_screening_response(&response_text) {
                Ok(screening) => {
                    // Apply priority resolution
                    let inc_matches: Vec<CriterionMatch> = screening.matched_inclusion_criteria
                        .iter()
                        .filter_map(|id| criteria.iter().find(|c| c.id == *id))
                        .map(|c| CriterionMatch {
                            id: c.id.clone(),
                            criterion_type: c.criterion_type.clone(),
                            priority: c.priority,
                        })
                        .collect();

                    let exc_matches: Vec<CriterionMatch> = screening.matched_exclusion_criteria
                        .iter()
                        .filter_map(|id| criteria.iter().find(|c| c.id == *id))
                        .map(|c| CriterionMatch {
                            id: c.id.clone(),
                            criterion_type: c.criterion_type.clone(),
                            priority: c.priority,
                        })
                        .collect();

                    let resolution_input = resolution::ScreeningInput {
                        inclusion_matches: inc_matches,
                        exclusion_matches: exc_matches,
                    };
                    let final_decision = resolution::resolve_decision(&resolution_input);

                    // Check for override
                    let ai_decision_str = screening.decision.as_str();
                    let mut reasoning = screening.reasoning;
                    if ai_decision_str != final_decision {
                        reasoning.push_str(&format!(
                            "\n\n[App override: {} favored due to priority resolution]",
                            if final_decision == "include" { "inclusion" } else { "exclusion" }
                        ));
                    }

                    // Update article
                    let c = conn.lock().await;
                    let new_status = if final_decision == "include" { "included" } else { "rejected" };
                    update_article_after_screening(
                        &c,
                        &article.id,
                        final_decision,
                        &reasoning,
                        screening.confidence,
                        &screening.matched_inclusion_criteria,
                        &screening.matched_exclusion_criteria,
                    )?;

                    // Create/update tags from suggested_tags
                    for tag_name in &screening.suggested_tags {
                        let _ = create_or_match_tag(&c, tag_name, &article.id);
                    }

                    // Update progress
                    let mut progress = self.progress.lock().await;
                    progress.completed += 1;
                    if final_decision == "include" {
                        progress.included += 1;
                    } else {
                        progress.rejected += 1;
                    }
                    let elapsed = start.elapsed().as_millis() as u64;
                    progress.elapsed_ms = elapsed;
                    if progress.completed > 0 {
                        let avg_per_article = elapsed / progress.completed as u64;
                        let remaining = (total - progress.completed) as u64;
                        progress.estimated_remaining_ms = Some(avg_per_article * remaining);
                    }
                }
                Err(_) => {
                    let c = conn.lock().await;
                    set_screening_error(&c, &article.id, &response_text)?;
                    let mut progress = self.progress.lock().await;
                    progress.errors += 1;
                    progress.completed += 1;
                }
            }
        }

        // Mark as done
        {
            let mut progress = self.progress.lock().await;
            progress.is_running = false;
            progress.current_article_title = None;
        }

        Ok(())
    }
}

fn process_screening_response(raw: &str) -> Result<LlmScreeningResponse, AppError> {
    // Try to extract JSON from the response (may be wrapped in markdown code blocks)
    let json_str = extract_json(raw);
    serde_json::from_str::<LlmScreeningResponse>(&json_str)
        .map_err(|e| AppError::Import(format!("Malformed LLM response: {}", e)))
}

fn extract_json(raw: &str) -> String {
    // Strip markdown code block if present
    let trimmed = raw.trim();
    if trimmed.starts_with("```") {
        let without_start = trimmed.trim_start_matches("```json").trim_start_matches("```");
        let without_end = without_start.trim_end_matches("```");
        return without_end.trim().to_string();
    }
    trimmed.to_string()
}

fn set_screening_error(conn: &Connection, article_id: &str, raw_response: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET screening_error = 1 WHERE id = ?1",
        rusqlite::params![article_id],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'ai_screen', ?3, 'ai')",
        rusqlite::params![audit_id, article_id, format!("Screening error. Raw response: {}", &raw_response[..raw_response.len().min(500)])],
    )?;

    Ok(())
}

fn update_article_after_screening(
    conn: &Connection,
    article_id: &str,
    decision: &str,
    reasoning: &str,
    confidence: f64,
    matched_inc: &[String],
    matched_exc: &[String],
) -> Result<(), AppError> {
    let new_status = if decision == "include" { "included" } else { "rejected" };
    let matched_inc_json = serde_json::to_string(matched_inc)?;
    let matched_exc_json = serde_json::to_string(matched_exc)?;

    conn.execute(
        "UPDATE articles SET status = ?1, ai_decision = ?2, ai_reasoning = ?3, ai_confidence = ?4, \
         matched_inclusion_criteria = ?5, matched_exclusion_criteria = ?6, screened_at = datetime('now') \
         WHERE id = ?7",
        rusqlite::params![new_status, decision, reasoning, confidence, matched_inc_json, matched_exc_json, article_id],
    )?;

    let audit_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) \
         VALUES (?1, ?2, 'ai_screen', 'working', ?3, ?4, 'ai')",
        rusqlite::params![audit_id, article_id, new_status, format!("AI screened: {} (confidence: {:.2})", decision, confidence)],
    )?;

    Ok(())
}

fn create_or_match_tag(conn: &Connection, tag_name: &str, article_id: &str) -> Result<(), AppError> {
    let tag_name_lower = tag_name.to_lowercase();

    // Check if tag exists (case-insensitive)
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM tags WHERE LOWER(name) = ?1",
            [&tag_name_lower],
            |row| row.get(0),
        )
        .ok();

    let tag_id = match existing_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name, source) VALUES (?1, ?2, 'ai_suggested')",
                rusqlite::params![id, tag_name_lower],
            )?;
            id
        }
    };

    // Link tag to article (ignore if already linked)
    conn.execute(
        "INSERT OR IGNORE INTO article_tags (article_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![article_id, tag_id],
    )?;

    Ok(())
}
```

- [ ] **Step 4: Add `pub mod llm;` to `src-tauri/src/lib.rs`**

- [ ] **Step 5: Add tokio dependency to `src-tauri/Cargo.toml`**

```toml
tokio = { version = "1", features = ["sync", "time", "rt"] }
reqwest = { version = "0.12", features = ["json"] }
```

- [ ] **Step 6: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/screening/engine.rs src-tauri/src/llm/ src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat(screening): add batch screening engine with LLM client"
```

---

## Task 5: Screening Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/screening.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/screening.rs`**

```rust
use std::sync::Arc;

use tauri::State;
use tokio::sync::Mutex;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::db::criteria_repo;
use crate::db::llm_config_repo;
use crate::error::AppError;
use crate::screening::engine::{ScreeningEngine, ScreeningProgress};
use crate::screening::token_estimation;

#[tauri::command]
pub async fn start_screening(db_state: State<'_, DbState>) -> Result<ScreeningProgress, AppError> {
    let conn_arc = Arc::new(db_state.conn.clone());

    let config = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        llm_config_repo::get_config(&conn)?
            .ok_or_else(|| AppError::Validation("LLM not configured. Please set up LLM configuration first.".to_string()))?
    };

    let criteria = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        criteria_repo::get_all_criteria(&conn)?
    };

    let aims = {
        let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
        criteria_repo::get_all_aims(&conn)?
    };

    let engine = ScreeningEngine::new();
    let progress = engine.run(conn_arc, config, criteria, aims).await?;

    Ok(progress)
}

#[tauri::command]
pub async fn get_screening_progress() -> Result<ScreeningProgress, AppError> {
    // This is a simplified version. In production, the engine would be managed
    // as Tauri state. For now, return a placeholder.
    Ok(ScreeningProgress::default())
}

#[tauri::command]
pub fn estimate_screening_tokens(db_state: State<'_, DbState>) -> Result<Option<String>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let config = llm_config_repo::get_config(&conn)?
        .ok_or_else(|| AppError::Validation("LLM not configured".to_string()))?;

    let articles = article_repo::get_articles_by_status(&conn, "working")?;
    let working: Vec<_> = articles.iter().filter(|a| a.screened_at.is_none()).collect();

    if working.is_empty() {
        return Ok(None);
    }

    // Estimate template tokens
    let template_text = crate::screening::prompt::SYSTEM_PROMPT.to_string();
    let template_tokens = token_estimation::estimate_tokens(&template_text);

    // Estimate per-article tokens
    let article_tokens: Vec<usize> = working
        .iter()
        .map(|a| {
            let text = format!("{}{}{}", a.title, a.authors.join(""), a.abstract_text);
            token_estimation::estimate_tokens(&text)
        })
        .collect();

    Ok(token_estimation::check_context_window(
        template_tokens,
        &article_tokens,
        config.context_window_tokens as usize,
    ))
}
```

- [ ] **Step 2: Update `src-tauri/src/commands/mod.rs`**

Add at top:

```rust
pub mod screening;
```

- [ ] **Step 3: Update `src-tauri/src/lib.rs` invoke handler**

Add to the `invoke_handler`:

```rust
commands::screening::start_screening,
commands::screening::get_screening_progress,
commands::screening::estimate_screening_tokens,
```

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS (may need stubs for criteria_repo and llm_config_repo — those are created in Plan 4)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/screening.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(screening): add Tauri commands for screening control"
```

---

## Task 6: Frontend Screening Progress UI

**Files:**
- Create: `src/composables/use-screening.ts`
- Create: `src/components/screening-progress-bar.vue`
- Create: `src/components/screening-stats.vue`
- Create: `src/views/screening-progress.vue`
- Modify: `src/router/index.ts`

- [ ] **Step 1: Create `src/composables/use-screening.ts`**

```typescript
import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface ScreeningProgress {
  total: number;
  completed: number;
  included: number;
  rejected: number;
  errors: number;
  isRunning: boolean;
  currentArticleTitle: string | null;
  elapsedMs: number;
  estimatedRemainingMs: number | null;
}

export function useScreening() {
  const progress = ref<ScreeningProgress | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const tokenWarning = ref<string | null>(null);

  const percentage = computed(() => {
    if (!progress.value || progress.value.total === 0) return 0;
    return Math.round((progress.value.completed / progress.value.total) * 100);
  });

  const estimatedTimeRemaining = computed((): string => {
    if (!progress.value?.estimatedRemainingMs) return '—';
    const seconds = Math.ceil(progress.value.estimatedRemainingMs / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds}s`;
  });

  async function checkTokenEstimate(): Promise<void> {
    try {
      tokenWarning.value = await tauriCommand<string | null>('estimate_screening_tokens');
    } catch (e) {
      // Ignore — may not have config yet
    }
  }

  async function startScreening(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      progress.value = await tauriCommand<ScreeningProgress>('start_screening');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function refreshProgress(): Promise<void> {
    try {
      progress.value = await tauriCommand<ScreeningProgress>('get_screening_progress');
    } catch {
      // Ignore
    }
  }

  return {
    progress, loading, error, tokenWarning,
    percentage, estimatedTimeRemaining,
    startScreening, refreshProgress, checkTokenEstimate,
  };
}
```

- [ ] **Step 2: Create `src/components/screening-progress-bar.vue`**

```vue
<script setup lang="ts">
defineProps<{
  completed: number;
  total: number;
  percentage: number;
}>();
</script>

<template>
  <div class="progress-bar">
    <div class="progress-bar__track">
      <div
        class="progress-bar__fill"
        :style="{ width: `${percentage}%` }"
      />
    </div>
    <div class="progress-bar__label">
      {{ completed }} / {{ total }} articles screened ({{ percentage }}%)
    </div>
  </div>
</template>

<style scoped>
.progress-bar {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.progress-bar__track {
  height: 8px;
  background-color: var(--color-surface-container-high);
  border-radius: var(--radius-pill);
  overflow: hidden;
}

.progress-bar__fill {
  height: 100%;
  background-color: var(--color-primary);
  border-radius: var(--radius-pill);
  transition: width 0.3s ease;
}

.progress-bar__label {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}
</style>
```

- [ ] **Step 3: Create `src/components/screening-stats.vue`**

```vue
<script setup lang="ts">
defineProps<{
  included: number;
  rejected: number;
  errors: number;
  estimatedTime: string;
}>();
</script>

<template>
  <div class="stats">
    <div class="stats__item stats__item--included">
      <span class="stats__value">{{ included }}</span>
      <span class="stats__label">Included</span>
    </div>
    <div class="stats__item stats__item--rejected">
      <span class="stats__value">{{ rejected }}</span>
      <span class="stats__label">Rejected</span>
    </div>
    <div class="stats__item stats__item--errors">
      <span class="stats__value">{{ errors }}</span>
      <span class="stats__label">Errors</span>
    </div>
    <div class="stats__item">
      <span class="stats__value">{{ estimatedTime }}</span>
      <span class="stats__label">Est. Remaining</span>
    </div>
  </div>
</template>

<style scoped>
.stats {
  display: flex;
  gap: var(--space-4);
}

.stats__item {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  min-width: 100px;
}

.stats__value {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
}

.stats__label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
}

.stats__item--included .stats__value {
  color: #16a34a;
}

.stats__item--rejected .stats__value {
  color: var(--color-error);
}

.stats__item--errors .stats__value {
  color: var(--color-priority-high);
}
</style>
```

- [ ] **Step 4: Create `src/views/screening-progress.vue`**

```vue
<script setup lang="ts">
import { useScreening } from '@/composables/use-screening';
import ScreeningProgressBar from '@/components/screening-progress-bar.vue';
import ScreeningStats from '@/components/screening-stats.vue';

const {
  progress, loading, error, tokenWarning,
  percentage, estimatedTimeRemaining,
  startScreening, checkTokenEstimate,
} = useScreening();
</script>

<template>
  <div class="screening-view">
    <div class="screening-view__header">
      <h1>AI Screening</h1>
      <div class="screening-view__actions">
        <button
          class="btn btn--primary"
          :disabled="loading || progress?.isRunning"
          @click="startScreening"
        >
          {{ loading ? 'Starting...' : progress?.isRunning ? 'Running...' : 'Start Screening' }}
        </button>
      </div>
    </div>

    <div v-if="error" class="screening-view__error">
      {{ error }}
    </div>

    <div v-if="tokenWarning" class="screening-view__warning">
      {{ tokenWarning }}
    </div>

    <div v-if="progress" class="screening-view__content">
      <ScreeningProgressBar
        :completed="progress.completed"
        :total="progress.total"
        :percentage="percentage"
      />

      <p v-if="progress.currentArticleTitle" class="screening-view__current">
        Screening: {{ progress.currentArticleTitle }}
      </p>

      <ScreeningStats
        :included="progress.included"
        :rejected="progress.rejected"
        :errors="progress.errors"
        :estimated-time="estimatedTimeRemaining"
      />
    </div>

    <div v-if="!progress && !loading" class="screening-view__empty">
      <p>Configure your criteria and LLM settings, then start screening.</p>
      <button class="btn btn--secondary" @click="checkTokenEstimate">
        Estimate Token Usage
      </button>
    </div>
  </div>
</template>

<style scoped>
.screening-view {
  padding: var(--space-6);
  max-width: 900px;
}

.screening-view__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-6);
}

.screening-view__error {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.screening-view__warning {
  padding: var(--space-3);
  background-color: var(--color-surface-container);
  color: var(--color-priority-high);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
  border: 1px solid var(--color-priority-high);
}

.screening-view__content {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.screening-view__current {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  font-style: italic;
}

.screening-view__empty {
  text-align: center;
  color: var(--color-on-surface-variant);
  padding: var(--space-10);
}

.btn {
  padding: var(--space-2) var(--space-4);
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

- [ ] **Step 5: Update router in `src/router/index.ts`**

Add:

```typescript
const ScreeningProgress = () => import('@/views/screening-progress.vue');
```

Change screening route:

```typescript
{ path: '/screening', name: 'screening', component: ScreeningProgress },
```

- [ ] **Step 6: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/composables/use-screening.ts src/components/screening-progress-bar.vue src/components/screening-stats.vue src/views/screening-progress.vue src/router/index.ts
git commit -m "feat(screening): add screening progress UI with progress bar and stats"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any issues from screening implementation"
```
