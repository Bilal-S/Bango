# Deduplication Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement multi-strategy deduplication that detects exact and fuzzy duplicates among imported articles, supports manual review of fuzzy matches, and advances surviving articles to the Working list.

**Architecture:** Pure Rust dedup engine in `src-tauri/src/dedup/` using normalized Levenshtein distance for title similarity. Tauri commands expose dedup operations to the frontend. The dedup review UI shows side-by-side comparison of fuzzy matches.

**Tech Stack:** Rust (Levenshtein algorithm), Tauri commands, Vue 3

**Depends on:** Plan 1 (Foundation & Database), Plan 2 (RIS Import Pipeline)

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── dedup/
│   ├── mod.rs                (new: module declarations)
│   ├── similarity.rs         (new: Levenshtein + normalization)
│   ├── engine.rs             (new: multi-strategy matching)
│   └── types.rs              (new: dedup result types)
├── commands/
│   ├── dedup.rs              (new: dedup Tauri commands)
│   └── mod.rs                (modify: add dedup commands)
├── db/
│   └── article_repo.rs       (modify: add dedup queries)
├── tests/
│   ├── dedup_test.rs         (new: dedup unit tests)
│   └── dedup_integration_test.rs (new: full pipeline test with real RIS data)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   └── dedup-review.vue      (new: dedup review UI)
├── components/
│   ├── dedup-pair.vue         (new: side-by-side comparison)
│   └── dedup-list.vue         (new: list of duplicate pairs)
├── composables/
│   └── use-dedup.ts           (new: dedup workflow composable)
├── router/
│   └── index.ts               (modify: update dedup route)
```

---

## Task 1: Similarity Functions

**Files:**
- Create: `src-tauri/src/dedup/mod.rs`
- Create: `src-tauri/src/dedup/similarity.rs`
- Create: `src-tauri/src/dedup/types.rs`
- Create: `src-tauri/tests/dedup_test.rs`

- [ ] **Step 1: Create `src-tauri/src/dedup/mod.rs`**

```rust
pub mod engine;
pub mod similarity;
pub mod types;
```

- [ ] **Step 2: Create `src-tauri/src/dedup/types.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    ExactDuplicate,
    FuzzyMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStrategy {
    DoiExact,
    TitleYear,
    FuzzyTitleYear,
    AuthorTitle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatePair {
    pub article_a_id: String,
    pub article_b_id: String,
    pub article_a_title: String,
    pub article_b_title: String,
    pub article_a_authors: Vec<String>,
    pub article_b_authors: Vec<String>,
    pub article_a_year: Option<i32>,
    pub article_b_year: Option<i32>,
    pub similarity: f64,
    pub match_type: MatchType,
    pub strategy: MatchStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupResult {
    pub exact_duplicates: Vec<DuplicatePair>,
    pub fuzzy_matches: Vec<DuplicatePair>,
    pub auto_merged_count: usize,
    pub needs_review_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DedupResolution {
    KeepA,
    KeepB,
    KeepBoth,
}
```

- [ ] **Step 3: Write failing tests in `src-tauri/tests/dedup_test.rs`**

```rust
use bango_lib::dedup::similarity::{normalize_title, levenshtein_similarity, short_title_guard};

#[test]
fn test_normalize_title_strips_punctuation() {
    assert_eq!(normalize_title("Hello, World! (2023)"), "hello world 2023");
}

#[test]
fn test_normalize_title_collapses_whitespace() {
    assert_eq!(normalize_title("  Hello   World  "), "hello world");
}

#[test]
fn test_normalize_title_strips_all_punctuation() {
    assert_eq!(
        normalize_title("A Study of ML.;:!?'''-()[]{}"),
        "a study of ml"
    );
}

#[test]
fn test_levenshtein_identical() {
    assert!((levenshtein_similarity("hello world", "hello world") - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_levenshtein_completely_different() {
    let sim = levenshtein_similarity("abc", "xyz");
    assert!(sim < 0.2, "Expected low similarity, got {}", sim);
}

#[test]
fn test_levenshtein_near_match() {
    let sim = levenshtein_similarity(
        "machine learning approaches to systematic review",
        "machine learning approach to systematic reviews",
    );
    assert!(sim > 0.9, "Expected high similarity, got {}", sim);
}

#[test]
fn test_levenshtein_moderate_match() {
    let sim = levenshtein_similarity(
        "deep learning for cancer detection",
        "deep learning for tumor detection",
    );
    assert!(sim > 0.7 && sim < 0.95, "Expected moderate similarity, got {}", sim);
}

#[test]
fn test_short_title_guard_short() {
    assert!(short_title_guard("ab")); // 2 chars, should be guarded
}

#[test]
fn test_short_title_guard_long_enough() {
    assert!(!short_title_guard("this is a longer title")); // 23 chars, OK
}

#[test]
fn test_short_title_guard_boundary() {
    assert!(short_title_guard("123456789")); // 9 chars, still short
    assert!(!short_title_guard("1234567890")); // 10 chars, OK
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cd src-tauri && cargo test dedup_test --test dedup_test`
Expected: FAIL — modules don't exist

- [ ] **Step 5: Implement `src-tauri/src/dedup/similarity.rs`**

```rust
/// Minimum character length for a title to participate in title-based matching.
const MIN_TITLE_LENGTH: usize = 10;

/// Normalizes a title for comparison:
/// 1. Lowercase
/// 2. Strip all punctuation
/// 3. Collapse whitespace
/// 4. Trim
#[must_use]
pub fn normalize_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let stripped: String = lower
        .chars()
        .map(|c| {
            if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '-' | '(' | ')' | '[' | ']' | '{' | '}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut result = String::with_capacity(stripped.len());
    let mut last_was_space = true; // trim leading
    for c in stripped.chars() {
        if c == ' ' {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    // Trim trailing space
    let trimmed = result.trim_end();
    trimmed.to_string()
}

/// Returns true if the normalized title is too short for title-based matching.
#[must_use]
pub fn short_title_guard(title: &str) -> bool {
    let normalized = normalize_title(title);
    normalized.len() < MIN_TITLE_LENGTH
}

/// Computes Levenshtein distance between two strings.
#[must_use]
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, a_char) in a.chars().enumerate() {
        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Computes normalized similarity (0.0–1.0) based on Levenshtein distance.
#[must_use]
pub fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f64 / max_len as f64)
}
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test dedup_test --test dedup_test`
Expected: PASS — all 10 tests pass

- [ ] **Step 7: Register module in `lib.rs` and commit**

Add `pub mod dedup;` to `src-tauri/src/lib.rs`.

```bash
git add src-tauri/src/dedup/ src-tauri/src/lib.rs src-tauri/tests/dedup_test.rs
git commit -m "feat(dedup): add title similarity and Levenshtein functions"
```

---

## Task 2: Dedup Engine

**Files:**
- Create: `src-tauri/src/dedup/engine.rs`
- Add tests to: `src-tauri/tests/dedup_test.rs`

- [ ] **Step 1: Add engine tests to `src-tauri/tests/dedup_test.rs`**

Append to the test file:

```rust
use bango_lib::dedup::engine::{run_dedup, DedupArticle};
use bango_lib::dedup::types::MatchType;

fn make_article(id: &str, title: &str, authors: &[&str], year: Option<i32>, doi: Option<&str>) -> DedupArticle {
    DedupArticle {
        id: id.to_string(),
        title: title.to_string(),
        authors: authors.iter().map(|a| a.to_string()).collect(),
        publication_year: year,
        doi: doi.map(|d| d.to_string()),
    }
}

#[test]
fn test_doi_exact_match() {
    let articles = vec![
        make_article("1", "Title A", &["Author A"], Some(2023), Some("10.1234/test")),
        make_article("2", "Title B", &["Author B"], Some(2023), Some("10.1234/test")),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1);
    assert!(result.fuzzy_matches.is_empty());
}

#[test]
fn test_title_year_exact_match() {
    let articles = vec![
        make_article("1", "Machine Learning for Systematic Reviews", &["Smith"], Some(2023), None),
        make_article("2", "Machine Learning for Systematic Reviews", &["Smith"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1);
}

#[test]
fn test_title_year_fuzzy_match() {
    let articles = vec![
        make_article("1", "Deep learning approaches for cancer detection", &["Smith"], Some(2023), None),
        make_article("2", "Deep learning approach for cancer detections", &["Jones"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.fuzzy_matches.len(), 1);
}

#[test]
fn test_no_match_different_years() {
    let articles = vec![
        make_article("1", "Machine learning for systematic reviews", &["Smith"], Some(2020), None),
        make_article("2", "Machine learning for systematic reviews", &["Smith"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 0);
    assert_eq!(result.fuzzy_matches.len(), 0);
}

#[test]
fn test_no_match_short_titles() {
    let articles = vec![
        make_article("1", "Short", &["Smith"], Some(2023), None),
        make_article("2", "Short", &["Smith"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    // Short titles skip title-based matching
    assert_eq!(result.exact_duplicates.len(), 0);
    assert_eq!(result.fuzzy_matches.len(), 0);
}

#[test]
fn test_null_year_skips_strategies_2_and_3() {
    let articles = vec![
        make_article("1", "Very similar title about machine learning applications", &["Smith"], None, None),
        make_article("2", "Very similar title about machine learning application", &["Smith"], None, None),
    ];
    let result = run_dedup(&articles);
    // Without year, strategies 2 & 3 are skipped. Strategy 4 (author+title) should catch this.
    assert_eq!(result.exact_duplicates.len() + result.fuzzy_matches.len(), 1);
}

#[test]
fn test_first_author_last_name_match() {
    let articles = vec![
        make_article("1", "Neural network approaches to text classification", &["Smith, John"], Some(2023), None),
        make_article("2", "Neural network approaches to text classifications", &["Smith, Jane"], Some(2023), None),
    ];
    let result = run_dedup(&articles);
    // Author matches, title similarity >= 80%
    assert_eq!(result.exact_duplicates.len() + result.fuzzy_matches.len(), 1);
}

#[test]
fn test_first_match_wins_no_double_matching() {
    let articles = vec![
        make_article("1", "Test Article Title One", &["Smith"], Some(2023), Some("10.1234/same")),
        make_article("2", "Test Article Title One", &["Smith"], Some(2023), Some("10.1234/same")),
    ];
    let result = run_dedup(&articles);
    // Should only match once despite matching multiple strategies
    assert_eq!(result.exact_duplicates.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test dedup_test --test dedup_test`
Expected: FAIL — `DedupArticle` and `run_dedup` not defined

- [ ] **Step 3: Implement `src-tauri/src/dedup/engine.rs`**

```rust
use super::similarity::{levenshtein_similarity, normalize_title, short_title_guard};
use super::types::{DedupResult, DuplicatePair, MatchStrategy, MatchType};

/// Lightweight article representation for dedup comparison.
#[derive(Debug, Clone)]
pub struct DedupArticle {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub doi: Option<String>,
}

/// Runs all dedup strategies against a list of articles.
/// Returns exact duplicates (auto-merge) and fuzzy matches (manual review).
#[must_use]
pub fn run_dedup(articles: &[DedupArticle]) -> DedupResult {
    let mut exact_duplicates = Vec::new();
    let mut fuzzy_matches = Vec::new();
    let mut matched_ids = std::collections::HashSet::new();

    for i in 0..articles.len() {
        if matched_ids.contains(&articles[i].id) {
            continue;
        }

        for j in (i + 1)..articles.len() {
            if matched_ids.contains(&articles[j].id) {
                continue;
            }

            if let Some(pair) = compare_articles(&articles[i], &articles[j]) {
                match pair.match_type {
                    MatchType::ExactDuplicate => {
                        exact_duplicates.push(pair);
                        // Mark the second article as matched (the "duplicate")
                        matched_ids.insert(articles[j].id.clone());
                    }
                    MatchType::FuzzyMatch => {
                        fuzzy_matches.push(pair);
                    }
                }
                break; // First match wins
            }
        }
    }

    let auto_merged_count = exact_duplicates.len();
    let needs_review_count = fuzzy_matches.len();

    DedupResult {
        exact_duplicates,
        fuzzy_matches,
        auto_merged_count,
        needs_review_count,
    }
}

fn compare_articles(a: &DedupArticle, b: &DedupArticle) -> Option<DuplicatePair> {
    // Strategy 1: DOI exact match
    if let (Some(doi_a), Some(doi_b)) = (&a.doi, &b.doi) {
        if !doi_a.is_empty() && !doi_b.is_empty() && doi_a.to_lowercase() == doi_b.to_lowercase() {
            return Some(make_pair(a, b, 1.0, MatchType::ExactDuplicate, MatchStrategy::DoiExact));
        }
    }

    let norm_a = normalize_title(&a.title);
    let norm_b = normalize_title(&b.title);

    // Short-title guard for strategies 2-4
    let a_short = short_title_guard(&a.title);
    let b_short = short_title_guard(&b.title);

    // Strategy 2: Title + Year (>= 95% similarity)
    if !a_short && !b_short {
        if let (Some(year_a), Some(year_b)) = (a.publication_year, b.publication_year) {
            if year_a == year_b {
                let sim = levenshtein_similarity(&norm_a, &norm_b);
                if sim >= 0.95 {
                    return Some(make_pair(a, b, sim, MatchType::ExactDuplicate, MatchStrategy::TitleYear));
                }

                // Strategy 3: Fuzzy Title + Year (70-94% similarity)
                if sim >= 0.70 {
                    return Some(make_pair(a, b, sim, MatchType::FuzzyMatch, MatchStrategy::FuzzyTitleYear));
                }
            }
        }
    }

    // Strategy 4: Author + Title partial
    if !a_short && !b_short {
        if let (Some(first_a), Some(first_b)) = (a.authors.first(), b.authors.first()) {
            let last_a = extract_last_name(first_a);
            let last_b = extract_last_name(first_b);
            if last_a.eq_ignore_ascii_case(&last_b) {
                let sim = levenshtein_similarity(&norm_a, &norm_b);
                if sim >= 0.80 {
                    return Some(make_pair(a, b, sim, MatchType::FuzzyMatch, MatchStrategy::AuthorTitle));
                }
            }
        }
    }

    None
}

fn make_pair(
    a: &DedupArticle,
    b: &DedupArticle,
    similarity: f64,
    match_type: MatchType,
    strategy: MatchStrategy,
) -> DuplicatePair {
    DuplicatePair {
        article_a_id: a.id.clone(),
        article_b_id: b.id.clone(),
        article_a_title: a.title.clone(),
        article_b_title: b.title.clone(),
        article_a_authors: a.authors.clone(),
        article_b_authors: b.authors.clone(),
        article_a_year: a.publication_year,
        article_b_year: b.publication_year,
        similarity,
        match_type,
        strategy,
    }
}

/// Extracts the last name from an author string like "Smith, John" or "John Smith".
fn extract_last_name(author: &str) -> String {
    let trimmed = author.trim();
    if let Some(pos) = trimmed.find(',') {
        // "Smith, John" format
        trimmed[..pos].trim().to_lowercase()
    } else if let Some(pos) = trimmed.rfind(' ') {
        // "John Smith" format
        trimmed[pos + 1..].trim().to_lowercase()
    } else {
        trimmed.to_lowercase()
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test dedup_test --test dedup_test`
Expected: PASS — all tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/dedup/engine.rs src-tauri/tests/dedup_test.rs
git commit -m "feat(dedup): implement multi-strategy deduplication engine"
```

- [ ] **Step 6: Write integration test with real RIS data**

Create `src-tauri/tests/dedup_integration_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::article_repo;
use bango_lib::dedup::engine::{self, DedupArticle};
use bango_lib::ris::parser::parse_ris;
use bango_lib::ris::validator::validate_all;
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_dedup_no_false_positives_on_real_data() {
    // Both real RIS files have distinct articles — no duplicates should be found
    let content1 = fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let content2 = fs::read_to_string(asset_path("11A-Resilience-Intersection-Capabilities.ris")).expect("fixture not found");

    let parsed1 = parse_ris(&content1).expect("Parse failed");
    let parsed2 = parse_ris(&content2).expect("Parse failed");

    let (valid1, _) = validate_all(&parsed1.records);
    let (valid2, _) = validate_all(&parsed2.records);

    let articles: Vec<DedupArticle> = valid1.iter().chain(valid2.iter()).map(|r| DedupArticle {
        id: uuid::Uuid::new_v4().to_string(),
        title: r.title.clone().unwrap_or_default(),
        authors: r.authors.clone(),
        publication_year: r.publication_year,
        doi: r.doi.clone(),
    }).collect();

    // 3 unique articles — no duplicates expected
    assert_eq!(articles.len(), 3);
    let result = engine::run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 0, "Should not find exact duplicates in real data");
    assert_eq!(result.fuzzy_matches.len(), 0, "Should not find fuzzy matches in real data");
}

#[test]
fn test_dedup_detects_doi_duplicate_from_real_data() {
    let content = fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let parsed = parse_ris(&content).expect("Parse failed");
    let (valid, _) = validate_all(&parsed.records);

    let original = &valid[0];
    let mut articles = vec![DedupArticle {
        id: "a1".to_string(),
        title: original.title.clone().unwrap_or_default(),
        authors: original.authors.clone(),
        publication_year: original.publication_year,
        doi: original.doi.clone(),
    }];

    // Add a duplicate with same DOI but different title
    articles.push(DedupArticle {
        id: "a2".to_string(),
        title: "Completely Different Title".to_string(),
        authors: vec!["Other Author".to_string()],
        publication_year: Some(2020),
        doi: original.doi.clone(),
    });

    let result = engine::run_dedup(&articles);
    assert_eq!(result.exact_duplicates.len(), 1, "Should detect DOI duplicate");
}
```

- [ ] **Step 7: Run integration tests**

Run: `cd src-tauri && cargo test dedup_integration_test --test dedup_integration_test`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/tests/dedup_integration_test.rs
git commit -m "test(dedup): add integration tests with real RIS data"
```

---

## Task 3: Dedup Tauri Commands & Article Repo Updates

**Files:**
- Create: `src-tauri/src/commands/dedup.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/db/article_repo.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add dedup-related query methods to `src-tauri/src/db/article_repo.rs`**

Add these functions to `article_repo.rs`:

```rust
pub fn get_imported_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM articles WHERE status = 'imported' AND duplicate_of IS NULL ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_working_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM articles WHERE status = 'working' ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn mark_as_duplicate(conn: &Connection, article_id: &str, surviving_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET duplicate_of = ?1 WHERE id = ?2",
        params![surviving_id, article_id],
    )?;
    Ok(())
}

pub fn move_to_working(conn: &Connection, article_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE articles SET status = 'working' WHERE id = ?1",
        params![article_id],
    )?;
    Ok(())
}

/// Returns the article with the most non-null fields (for merge decisions).
pub fn get_article_field_count(conn: &Connection, id: &str) -> Result<usize, AppError> {
    let article = get_article_by_id(conn, id)?;
    let mut count = 0;
    if article.doi.is_some() { count += 1; }
    if article.journal.is_some() { count += 1; }
    if article.volume.is_some() { count += 1; }
    if article.issue.is_some() { count += 1; }
    if article.start_page.is_some() { count += 1; }
    if article.end_page.is_some() { count += 1; }
    if article.publication_year.is_some() { count += 1; }
    if article.url.is_some() { count += 1; }
    if article.language.is_some() { count += 1; }
    if article.publisher.is_some() { count += 1; }
    if article.issn.is_some() { count += 1; }
    if article.reference_type.is_some() { count += 1; }
    if article.date.is_some() { count += 1; }
    if !article.keywords.is_empty() { count += 1; }
    if article.notes.is_some() { count += 1; }
    if article.abstract_text.is_empty() { count += 0; } else { count += 1; }
    Ok(count)
}
```

- [ ] **Step 2: Create `src-tauri/src/commands/dedup.rs`**

```rust
use serde::Deserialize;
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::dedup::engine::{self, DedupArticle};
use crate::dedup::types::{DedupResolution, DedupResult};
use crate::error::AppError;
use crate::models::audit::{AuditAction, AuditSource};
use crate::models::article::Article;

use uuid::Uuid;

#[tauri::command]
pub fn run_deduplication(db_state: State<'_, DbState>) -> Result<DedupResult, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let imported = article_repo::get_imported_articles(&conn)?;
    let working = article_repo::get_working_articles(&conn)?;

    // Convert to dedup articles for comparison
    let mut dedup_articles: Vec<DedupArticle> = imported.iter()
        .chain(working.iter())
        .map(|a| DedupArticle {
            id: a.id.clone(),
            title: a.title.clone(),
            authors: a.authors.clone(),
            publication_year: a.publication_year,
            doi: a.doi.clone(),
        })
        .collect();

    // Compare only new imported articles against existing imported + working
    let imported_count = imported.len();
    let result = if imported_count > 0 {
        engine::run_dedup(&dedup_articles)
    } else {
        crate::dedup::types::DedupResult {
            exact_duplicates: vec![],
            fuzzy_matches: vec![],
            auto_merged_count: 0,
            needs_review_count: 0,
        }
    };

    // Auto-merge exact duplicates
    for pair in &result.exact_duplicates {
        // Determine which article survives (most metadata fields)
        let count_a = article_repo::get_article_field_count(&conn, &pair.article_a_id).unwrap_or(0);
        let count_b = article_repo::get_article_field_count(&conn, &pair.article_b_id).unwrap_or(0);

        let (surviving_id, duplicate_id) = if count_a >= count_b {
            (&pair.article_a_id, &pair.article_b_id)
        } else {
            (&pair.article_b_id, &pair.article_a_id)
        };

        article_repo::mark_as_duplicate(&conn, duplicate_id, surviving_id)?;

        // Audit entry
        let audit_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'system')",
            rusqlite::params![audit_id, duplicate_id, format!("Merged into article {}", surviving_id)],
        )?;

        // Move surviving article to Working
        article_repo::move_to_working(&conn, surviving_id)?;

        let audit_id2 = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO audit_entries (id, article_id, action, from_status, to_status, details, source) VALUES (?1, ?2, 'status_change', 'imported', 'working', 'Advanced after deduplication', 'system')",
            rusqlite::params![audit_id2, surviving_id],
        )?;
    }

    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFuzzyRequest {
    pub pair_index: usize,
    pub resolution: DedupResolution,
    pub article_a_id: String,
    pub article_b_id: String,
}

#[tauri::command]
pub fn resolve_fuzzy_match(db_state: State<'_, DbState>, request: ResolveFuzzyRequest) -> Result<Article, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    match request.resolution {
        DedupResolution::KeepA => {
            article_repo::mark_as_duplicate(&conn, &request.article_b_id, &request.article_a_id)?;
            article_repo::move_to_working(&conn, &request.article_a_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'user')",
                rusqlite::params![audit_id, request.article_b_id, format!("User chose to keep article A ({})", request.article_a_id)],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_a_id)
        }
        DedupResolution::KeepB => {
            article_repo::mark_as_duplicate(&conn, &request.article_a_id, &request.article_b_id)?;
            article_repo::move_to_working(&conn, &request.article_b_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_merge', ?3, 'user')",
                rusqlite::params![audit_id, request.article_a_id, format!("User chose to keep article B ({})", request.article_b_id)],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_b_id)
        }
        DedupResolution::KeepBoth => {
            article_repo::move_to_working(&conn, &request.article_a_id)?;
            article_repo::move_to_working(&conn, &request.article_b_id)?;
            let audit_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'dedup_flag', 'User marked as not duplicates', 'user')",
                rusqlite::params![audit_id, request.article_a_id],
            )?;
            article_repo::get_article_by_id(&conn, &request.article_a_id)
        }
    }
}
```

- [ ] **Step 3: Update `src-tauri/src/commands/mod.rs` to include dedup**

Add at the top of `commands/mod.rs`:

```rust
pub mod dedup;
```

- [ ] **Step 4: Update `src-tauri/src/lib.rs` to register dedup commands**

Add to the `invoke_handler`:

```rust
commands::dedup::run_deduplication,
commands::dedup::resolve_fuzzy_match,
```

- [ ] **Step 5: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/dedup.rs src-tauri/src/commands/mod.rs src-tauri/src/db/article_repo.rs src-tauri/src/lib.rs
git commit -m "feat(dedup): add Tauri commands for dedup operations"
```

---

## Task 4: Frontend Dedup Review UI

**Files:**
- Create: `src/composables/use-dedup.ts`
- Create: `src/components/dedup-pair.vue`
- Create: `src/views/dedup-review.vue`
- Modify: `src/router/index.ts`

- [ ] **Step 1: Create `src/composables/use-dedup.ts`**

```typescript
import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface DuplicatePair {
  articleAId: string;
  articleBId: string;
  articleATitle: string;
  articleBTitle: string;
  articleAAuthors: string[];
  articleBAuthors: string[];
  articleAYear: number | null;
  articleBYear: number | null;
  similarity: number;
  matchType: 'exactDuplicate' | 'fuzzyMatch';
  strategy: string;
}

export interface DedupResult {
  exactDuplicates: DuplicatePair[];
  fuzzyMatches: DuplicatePair[];
  autoMergedCount: number;
  needsReviewCount: number;
}

export type DedupResolution = 'keepA' | 'keepB' | 'keepBoth';

export function useDedup() {
  const result = ref<DedupResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const resolvedCount = ref(0);

  async function runDeduplication(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      result.value = await tauriCommand<DedupResult>('run_deduplication');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  async function resolveFuzzy(
    pair: DuplicatePair,
    resolution: DedupResolution,
  ): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await tauriCommand('resolve_fuzzy_match', {
        request: {
          pairIndex: 0,
          resolution,
          articleAId: pair.articleAId,
          articleBId: pair.articleBId,
        },
      });
      resolvedCount.value++;

      // Remove resolved pair from fuzzy matches
      if (result.value) {
        result.value.fuzzyMatches = result.value.fuzzyMatches.filter(
          (p) => !(p.articleAId === pair.articleAId && p.articleBId === pair.articleBId),
        );
        result.value.needsReviewCount = result.value.fuzzyMatches.length;
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { result, loading, error, resolvedCount, runDeduplication, resolveFuzzy };
}
```

- [ ] **Step 2: Create `src/components/dedup-pair.vue`**

```vue
<script setup lang="ts">
import type { DuplicatePair, DedupResolution } from '@/composables/use-dedup';

defineProps<{ pair: DuplicatePair }>();
const emit = defineEmits<{ resolve: [resolution: DedupResolution] }>();
</script>

<template>
  <div class="dedup-pair">
    <div class="dedup-pair__similarity">
      {{ (pair.similarity * 100).toFixed(1) }}% similar
    </div>
    <div class="dedup-pair__comparison">
      <div class="dedup-pair__record">
        <h3>Record A</h3>
        <p class="dedup-pair__title">{{ pair.articleATitle }}</p>
        <p class="dedup-pair__meta">{{ pair.articleAAuthors.join('; ') }}</p>
        <p class="dedup-pair__meta">{{ pair.articleAYear ?? 'No year' }}</p>
      </div>
      <div class="dedup-pair__vs">vs</div>
      <div class="dedup-pair__record">
        <h3>Record B</h3>
        <p class="dedup-pair__title">{{ pair.articleBTitle }}</p>
        <p class="dedup-pair__meta">{{ pair.articleBAuthors.join('; ') }}</p>
        <p class="dedup-pair__meta">{{ pair.articleBYear ?? 'No year' }}</p>
      </div>
    </div>
    <div class="dedup-pair__actions">
      <button class="btn btn--primary" @click="emit('resolve', 'keepA')">Keep A</button>
      <button class="btn btn--primary" @click="emit('resolve', 'keepB')">Keep B</button>
      <button class="btn btn--secondary" @click="emit('resolve', 'keepBoth')">Keep Both</button>
    </div>
  </div>
</template>

<style scoped>
.dedup-pair {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dedup-pair__similarity {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
}

.dedup-pair__comparison {
  display: flex;
  gap: var(--space-3);
  align-items: stretch;
}

.dedup-pair__record {
  flex: 1;
  padding: var(--space-3);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-sm);
}

.dedup-pair__record h3 {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  margin-bottom: var(--space-2);
}

.dedup-pair__title {
  font-weight: var(--font-weight-semibold);
  margin-bottom: var(--space-1);
}

.dedup-pair__meta {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.dedup-pair__vs {
  display: flex;
  align-items: center;
  font-size: var(--font-size-label);
  color: var(--color-outline);
  font-weight: var(--font-weight-semibold);
}

.dedup-pair__actions {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
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
</style>
```

- [ ] **Step 3: Create `src/views/dedup-review.vue`**

```vue
<script setup lang="ts">
import { useDedup } from '@/composables/use-dedup';
import DedupPair from '@/components/dedup-pair.vue';
import type { DuplicatePair, DedupResolution } from '@/composables/use-dedup';

const { result, loading, error, resolvedCount, runDeduplication, resolveFuzzy } = useDedup();

function onResolve(pair: DuplicatePair, resolution: DedupResolution): void {
  resolveFuzzy(pair, resolution);
}
</script>

<template>
  <div class="dedup-view">
    <div class="dedup-view__header">
      <h1>Deduplication</h1>
      <button
        class="btn btn--primary"
        :disabled="loading"
        @click="runDeduplication"
      >
        {{ loading ? 'Running...' : 'Run Deduplication' }}
      </button>
    </div>

    <div v-if="error" class="dedup-view__error">{{ error }}</div>

    <div v-if="result" class="dedup-view__content">
      <div class="dedup-view__summary">
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.autoMergedCount }}</span>
          <span class="dedup-view__stat-label">Auto-Merged</span>
        </div>
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.needsReviewCount }}</span>
          <span class="dedup-view__stat-label">Needs Review</span>
        </div>
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ resolvedCount }}</span>
          <span class="dedup-view__stat-label">Resolved</span>
        </div>
      </div>

      <section v-if="result.fuzzyMatches.length > 0">
        <h2>Potential Duplicates ({{ result.fuzzyMatches.length }} remaining)</h2>
        <div class="dedup-view__pairs">
          <DedupPair
            v-for="pair in result.fuzzyMatches"
            :key="`${pair.articleAId}-${pair.articleBId}`"
            :pair="pair"
            @resolve="(r: DedupResolution) => onResolve(pair, r)"
          />
        </div>
      </section>

      <div v-else-if="result.autoMergedCount > 0" class="dedup-view__done">
        <h2>Deduplication Complete</h2>
        <p>{{ result.autoMergedCount }} exact duplicates merged. No fuzzy matches found.</p>
      </div>

      <div v-else class="dedup-view__done">
        <h2>No Duplicates Found</h2>
        <p>All articles are unique. Articles have been advanced to Working status.</p>
      </div>
    </div>

    <div v-if="!result && !loading" class="dedup-view__empty">
      <p>Import articles first, then run deduplication to find and resolve duplicates.</p>
    </div>
  </div>
</template>

<style scoped>
.dedup-view {
  padding: var(--space-6);
  max-width: 1000px;
}

.dedup-view__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-6);
}

.dedup-view__error {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.dedup-view__summary {
  display: flex;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.dedup-view__stat {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  min-width: 100px;
}

.dedup-view__stat-value {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
}

.dedup-view__stat-label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
}

.dedup-view__pairs {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-top: var(--space-4);
}

.dedup-view__done,
.dedup-view__empty {
  padding: var(--space-6);
  text-align: center;
  color: var(--color-on-surface-variant);
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

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **Step 4: Update router**

In `src/router/index.ts`, add:

```typescript
const DedupReview = () => import('@/views/dedup-review.vue');
```

And change the dedup route:

```typescript
{ path: '/dedup', name: 'dedup', component: DedupReview },
```

- [ ] **Step 5: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/composables/use-dedup.ts src/components/dedup-pair.vue src/views/dedup-review.vue src/router/index.ts
git commit -m "feat(dedup): add dedup review UI with side-by-side comparison"
```

---

## Task 5: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Verify dedup flow**

Run: `cd src-tauri && cargo tauri dev`
Expected: App opens, import an RIS file, navigate to Dedup, run deduplication.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any issues from dedup implementation"
```
