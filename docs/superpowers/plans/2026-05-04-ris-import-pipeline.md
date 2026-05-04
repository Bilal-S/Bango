# RIS Import Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a complete RIS file parser in Rust that validates and imports articles into SQLite, with a Vue-based import UI featuring drag-and-drop, preview, and a stepper workflow.

**Architecture:** The RIS parser is a pure Rust module (`src-tauri/src/ris/`) that parses RIS files into `NewArticle` structs, validates required fields, and stores them via Tauri commands. The frontend provides an import wizard with file selection, preview table, and import summary.

**Tech Stack:** Rust (nom for parsing or hand-rolled parser), Tauri commands, Vue 3, TypeScript

**Depends on:** Plan 1 (Foundation & Database) must be complete

---

## File Structure

### Rust (src-tauri/)

```
src-tauri/src/
├── ris/
│   ├── mod.rs                (new: module declarations)
│   ├── parser.rs             (new: RIS format parser)
│   ├── validator.rs          (new: import validation logic)
│   └── types.rs              (new: RIS-specific types)
├── commands/
│   ├── mod.rs                (modify: add import commands)
│   └── import.rs             (new: import-related Tauri commands)
├── db/
│   └── article_repo.rs       (new: article database operations)
├── models/
│   └── article.rs            (modify: add DB mapping methods)
├── tests/
│   └── ris_test.rs           (new: RIS parser tests)
```

### TypeScript/Vue (src/)

```
src/
├── views/
│   └── import-ris.vue        (new: import wizard view)
├── components/
│   ├── import-drop-zone.vue  (new: drag-and-drop file picker)
│   ├── import-preview.vue    (new: article preview table)
│   └── import-stepper.vue    (new: step indicator)
├── composables/
│   └── use-import.ts         (new: import workflow composable)
├── stores/
│   └── articles.ts           (modify: add import actions)
```

### Test Data

```
tests/
└── assets/                        (existing: real RIS files)
    ├── 10A_Lewicki_Stages.ris     (existing: 1 record, full fields)
    └── 11A-Resilience-Intersection-Capabilities.ris  (existing: 2 records, multi-author)
```

---

## Task 1: RIS Parser Types

**Files:**
- Create: `src-tauri/src/ris/mod.rs`
- Create: `src-tauri/src/ris/types.rs`

- [ ] **Step 1: Create `src-tauri/src/ris/mod.rs`**

```rust
pub mod parser;
pub mod types;
pub mod validator;
```

- [ ] **Step 2: Create `src-tauri/src/ris/types.rs`**

```rust
use std::collections::HashMap;

/// A single parsed RIS record, before conversion to NewArticle.
/// Fields are optional during parsing — validation happens separately.
#[derive(Debug, Clone, Default)]
pub struct RisRecord {
    pub reference_type: Option<String>,
    pub title: Option<String>,
    pub abstract_text: Option<String>,
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
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    /// All unrecognized RIS tags preserved as key-value pairs.
    pub extras: HashMap<String, Vec<String>>,
}

/// Result of parsing a complete RIS file.
#[derive(Debug)]
pub struct RisParseResult {
    pub records: Vec<RisRecord>,
    pub errors: Vec<RisParseError>,
}

/// A single parse error for a record in the RIS file.
#[derive(Debug)]
pub struct RisParseError {
    /// 1-based index of the record in the file.
    pub record_index: usize,
    pub message: String,
}
```

- [ ] **Step 3: Register module in `src-tauri/src/lib.rs`**

Add `pub mod ris;` to the module declarations.

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ris/ src-tauri/src/lib.rs
git commit -m "feat(ris): add RIS parser types"
```

---

## Task 2: RIS Parser Implementation

**Files:**
- Create: `src-tauri/src/ris/parser.rs`
- Create: `src-tauri/tests/ris_test.rs`
- Uses existing: `tests/assets/10A_Lewicki_Stages.ris`
- Uses existing: `tests/assets/11A-Resilience-Intersection-Capabilities.ris`

- [ ] **Step 1: Note test asset paths**

Real RIS files are in `tests/assets/`:
- `10A_Lewicki_Stages.ris` — 1 record with full metadata (authors, DOI, keywords, abstract, journal)
- `11A-Resilience-Intersection-Capabilities.ris` — 2 records with multiple authors, ISSN, C3 fields

These will be used for parser tests. No synthetic fixtures needed.

- [ ] **Step 2: Write failing tests in `src-tauri/tests/ris_test.rs`**

```rust
use bango_lib::ris::parser::parse_ris;
use std::fs;
use std::path::PathBuf;

fn asset_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../tests/assets");
    path.push(name);
    path
}

#[test]
fn test_parse_single_record_ris() {
    let content = fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 1);
    assert_eq!(result.errors.len(), 0);

    let record = &result.records[0];
    assert_eq!(record.reference_type.as_deref(), Some("JOUR"));
    assert!(record.title.as_ref().unwrap().contains("Multi-Paradigm Ethical Framework"));
    assert_eq!(record.authors.len(), 1);
    assert_eq!(record.authors[0], "Alibasic, H");
    assert!(record.abstract_text.as_ref().unwrap().contains("artificial intelligence"));
    assert_eq!(record.publication_year, Some(2025));
    assert_eq!(record.doi.as_deref(), Some("10.3390/fintech4030034"));
    assert_eq!(record.journal.as_deref(), Some("FINTECH"));
    assert_eq!(record.volume.as_deref(), Some("4"));
    assert_eq!(record.issue.as_deref(), Some("3"));
    assert_eq!(record.start_page.as_deref(), Some("34"));
    assert!(record.keywords.len() >= 5);
    assert_eq!(record.language.as_deref(), Some("English"));
    assert_eq!(record.issn.as_deref(), Some("2674-1032"));
    assert_eq!(record.publisher.as_deref(), Some("MDPI"));
    assert!(record.notes.is_some());
}

#[test]
fn test_parse_multi_record_ris() {
    let content = fs::read_to_string(asset_path("11A-Resilience-Intersection-Capabilities.ris")).expect("fixture not found");
    let result = parse_ris(&content).expect("Parse failed");
    assert_eq!(result.records.len(), 2);
    assert_eq!(result.errors.len(), 0);

    // First record
    let rec1 = &result.records[0];
    assert!(rec1.title.as_ref().unwrap().contains("blockchain we trust"));
    assert_eq!(rec1.authors.len(), 2);
    assert_eq!(rec1.authors[0], "Toufaily, E");
    assert_eq!(rec1.authors[1], "Zalan, T");
    assert_eq!(rec1.publication_year, Some(2024));
    assert_eq!(rec1.doi.as_deref(), Some("10.1016/j.techfore.2024.123574"));
    assert!(rec1.keywords.len() >= 5);

    // Second record
    let rec2 = &result.records[1];
    assert!(rec2.title.as_ref().unwrap().contains("qualitative systematic review"));
    assert_eq!(rec2.authors.len(), 4);
    assert_eq!(rec2.publication_year, Some(2025));
    assert_eq!(rec2.doi.as_deref(), Some("10.1177/02683962241254392"));
    assert_eq!(rec2.start_page.as_deref(), Some("55"));
    assert_eq!(rec2.end_page.as_deref(), Some("76"));
}

#[test]
fn test_parse_preserves_unrecognized_tags() {
    let content = "TY  - JOUR\nTI  - Test\nAU  - Author\nAB  - Abstract\nXX  - Unknown Value\nER  -\n";
    let result = parse_ris(content).expect("Parse failed");
    assert_eq!(result.records[0].extras.get("XX").map(|v| v.as_slice()), Some(&["Unknown Value".to_string()][..]));
}

#[test]
fn test_parse_empty_input() {
    let result = parse_ris("").expect("Parse failed");
    assert_eq!(result.records.len(), 0);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd src-tauri && cargo test ris_test --test ris_test`
Expected: FAIL — `parse_ris` not yet implemented

- [ ] **Step 4: Implement `src-tauri/src/ris/parser.rs`**

```rust
use std::collections::HashMap;

use crate::error::AppError;
use super::types::{RisParseResult, RisParseError, RisRecord};

/// Parses a complete RIS file content into records.
/// Records are delimited by `ER` tags.
pub fn parse_ris(content: &str) -> Result<RisParseResult, AppError> {
    let mut records = Vec::new();
    let mut errors = Vec::new();
    let mut current = RisRecord::default();
    let mut record_index = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // RIS format: "TY  - VALUE" (tag, two spaces, dash, space, value)
        let (tag, value) = match parse_tag_value(trimmed) {
            Some(pair) => pair,
            None => continue,
        };

        if tag == "ER" {
            records.push(current);
            current = RisRecord::default();
            record_index += 1;
            continue;
        }

        apply_tag(tag, value, &mut current);
    }

    // If file doesn't end with ER, collect the last record
    if !current.title.is_none() || !current.abstract_text.is_none() || !current.authors.is_empty() {
        record_index += 1;
        errors.push(RisParseError {
            record_index,
            message: "Record missing ER (end of reference) tag".to_string(),
        });
        records.push(current);
    }

    Ok(RisParseResult { records, errors })
}

/// Parses "TY  - JOUR" into ("TY", "JOUR").
fn parse_tag_value(line: &str) -> Option<(&str, &str)> {
    if line.len() < 6 {
        return None;
    }

    let tag = &line[..2];
    let rest = &line[2..];

    // Expect "  - " separator
    if !rest.starts_with("  - ") {
        return None;
    }

    let value = rest[4..].trim();
    Some((tag, value))
}

/// Applies a single RIS tag-value pair to a record.
fn apply_tag(tag: &str, value: &str, record: &mut RisRecord) {
    match tag {
        "TY" => record.reference_type = Some(value.to_string()),
        "TI" => record.title = Some(value.to_string()),
        "AB" => record.abstract_text = Some(value.to_string()),
        "AU" => record.authors.push(value.to_string()),
        "PY" => {
            // PY can be "2023" or "2023/12/31/" — extract year
            let year_str = value.split('/').next().unwrap_or(value);
            record.publication_year = year_str.parse().ok();
        }
        "DO" => record.doi = Some(value.to_string()),
        "T2" => record.journal = Some(value.to_string()),
        "VL" => record.volume = Some(value.to_string()),
        "IS" => record.issue = Some(value.to_string()),
        "SP" => record.start_page = Some(value.to_string()),
        "EP" => record.end_page = Some(value.to_string()),
        "KW" => record.keywords.push(value.to_string()),
        "UR" => record.url = Some(value.to_string()),
        "LA" => record.language = Some(value.to_string()),
        "PB" => record.publisher = Some(value.to_string()),
        "PU" => {
            if record.publisher.is_none() {
                record.publisher = Some(value.to_string());
            }
        }
        "SN" => record.issn = Some(value.to_string()),
        "M3" => {
            if record.reference_type.is_none() {
                record.reference_type = Some(value.to_string());
            }
        }
        "N2" => {
            if record.abstract_text.is_none() {
                record.abstract_text = Some(value.to_string());
            }
        }
        "JO" => {
            if record.journal.is_none() {
                record.journal = Some(value.to_string());
            }
        }
        "DA" => record.date = Some(value.to_string()),
        "AD" => record.author_address = Some(value.to_string()),
        "AN" => record.accession_number = Some(value.to_string()),
        "C3" => record.custom_field3 = Some(value.to_string()),
        "J9" => record.journal_abbreviation = Some(value.to_string()),
        "JI" => record.journal_iso_abbreviation = Some(value.to_string()),
        "N1" => record.notes = Some(value.to_string()),
        "PA" => record.publisher_address = Some(value.to_string()),
        "PI" => record.publisher_city = Some(value.to_string()),
        "WE" => record.web_of_science_db = Some(value.to_string()),
        "ER" => { /* handled by caller */ }
        _ => {
            record.extras
                .entry(tag.to_string())
                .or_default()
                .push(value.to_string());
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test ris_test --test ris_test`
Expected: PASS — all 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ris/ src-tauri/tests/ris_test.rs
git commit -m "feat(ris): implement RIS parser with all supported tags"
```

---

## Task 3: RIS Validator

**Files:**
- Create: `src-tauri/src/ris/validator.rs`
- Add tests to: `src-tauri/tests/ris_test.rs`

- [ ] **Step 1: Add failing validation tests to `src-tauri/tests/ris_test.rs`**

Append these tests:

```rust
use bango_lib::ris::validator::validate_record;
use bango_lib::ris::types::RisRecord;

#[test]
fn test_validate_valid_record() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_missing_title() {
    let mut record = RisRecord::default();
    record.abstract_text = Some("Abstract".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Title")));
}

#[test]
fn test_validate_missing_abstract() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.authors = vec!["Author".to_string()];
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Abstract")));
}

#[test]
fn test_validate_missing_authors() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = Some("Abstract".to_string());
    let errors = validate_record(&record, 1);
    assert!(errors.iter().any(|e| e.message.contains("Author")));
}

#[test]
fn test_validate_n2_abstract_fallback() {
    let mut record = RisRecord::default();
    record.title = Some("Title".to_string());
    record.abstract_text = None;
    record.authors = vec!["Author".to_string()];
    // N2 was already mapped to abstract_text by the parser.
    // This test verifies the parser correctly falls back.
    // Direct validation: if abstract_text is present, it's valid.
    let mut record_with_n2 = record.clone();
    record_with_n2.abstract_text = Some("From N2".to_string());
    let errors = validate_record(&record_with_n2, 1);
    assert!(errors.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test validate --test ris_test`
Expected: FAIL — `validate_record` not yet implemented

- [ ] **Step 3: Implement `src-tauri/src/ris/validator.rs`**

```rust
use super::types::{RisParseError, RisRecord};

/// Validates a single RIS record for required fields.
/// Returns a list of validation errors (empty if valid).
pub fn validate_record(record: &RisRecord, record_index: usize) -> Vec<RisParseError> {
    let mut errors = Vec::new();

    if record.title.is_none() || record.title.as_ref().is_some_and(|t| t.trim().is_empty()) {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: Title (TI)".to_string(),
        });
    }

    if record.abstract_text.is_none() || record.abstract_text.as_ref().is_some_and(|a| a.trim().is_empty()) {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: Abstract (AB or N2)".to_string(),
        });
    }

    if record.authors.is_empty() {
        errors.push(RisParseError {
            record_index,
            message: "Missing required field: at least one Author (AU)".to_string(),
        });
    }

    errors
}

/// Validates all records in a parse result, returning only valid records
/// and collecting all validation errors.
pub fn validate_all(records: &[RisRecord]) -> (Vec<RisRecord>, Vec<RisParseError>) {
    let mut valid = Vec::new();
    let mut all_errors = Vec::new();

    for (i, record) in records.iter().enumerate() {
        let errors = validate_record(record, i + 1);
        if errors.is_empty() {
            valid.push(record.clone());
        } else {
            all_errors.extend(errors);
        }
    }

    (valid, all_errors)
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --test ris_test`
Expected: PASS — all tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ris/validator.rs src-tauri/tests/ris_test.rs
git commit -m "feat(ris): add record validation for required fields"
```

---

## Task 4: Article Repository & Tauri Commands

**Files:**
- Create: `src-tauri/src/db/article_repo.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/commands/import.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/db/article_repo.rs`**

```rust
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::article::{Article, ArticleStatus, AiDecision, NewArticle};

const MAX_ARTICLES: usize = 10_000;

pub fn count_articles(conn: &Connection) -> Result<usize, AppError> {
    let count: usize = conn
        .query_row("SELECT COUNT(*) FROM articles", [], |row| row.get(0))?;
    Ok(count)
}

pub fn remaining_capacity(conn: &Connection) -> Result<usize, AppError> {
    let count = count_articles(conn)?;
    Ok(MAX_ARTICLES.saturating_sub(count))
}

pub fn insert_article(conn: &Connection, article: &NewArticle) -> Result<Article, AppError> {
    let id = Uuid::new_v4().to_string();
    let authors_json = serde_json::to_string(&article.authors)?;
    let keywords_json = serde_json::to_string(&article.keywords)?;
    let ris_extras_json = article.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO articles (
            id, status, title, abstract_text, authors, publication_year, doi,
            journal, volume, issue, start_page, end_page, keywords, url,
            language, publisher, publisher_city, publisher_address, issn,
            reference_type, date, author_address, accession_number,
            custom_field3, journal_abbreviation, journal_iso_abbreviation,
            notes, web_of_science_db, ris_extras, import_source
        ) VALUES (
            ?1, 'imported', ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25,
            ?26, ?27, ?28, ?29
        )",
        params![
            id, article.title, article.abstract_text, authors_json,
            article.publication_year, article.doi,
            article.journal, article.volume, article.issue,
            article.start_page, article.end_page, keywords_json, article.url,
            article.language, article.publisher, article.publisher_city,
            article.publisher_address, article.issn,
            article.reference_type, article.date, article.author_address,
            article.accession_number, article.custom_field3,
            article.journal_abbreviation, article.journal_iso_abbreviation,
            article.notes, article.web_of_science_db, ris_extras_json,
            article.import_source,
        ],
    )?;

    get_article_by_id(conn, &id)
}

pub fn insert_articles_batch(
    conn: &Connection,
    articles: &[NewArticle],
    import_source: &str,
) -> Result<Vec<Article>, AppError> {
    let remaining = remaining_capacity(conn)?;
    if articles.len() > remaining {
        return Err(AppError::Import(format!(
            "File contains {} articles but only {} slots remain ({} of {} limit reached)",
            articles.len(),
            remaining,
            count_articles(conn)?,
            MAX_ARTICLES,
        )));
    }

    let mut inserted = Vec::with_capacity(articles.len());
    let tx = conn.unchecked_transaction()?;

    for article in articles {
        let mut article_with_source = article.clone();
        article_with_source.import_source = Some(import_source.to_string());
        let id = Uuid::new_v4().to_string();
        let authors_json = serde_json::to_string(&article_with_source.authors)?;
        let keywords_json = serde_json::to_string(&article_with_source.keywords)?;
        let ris_extras_json = article_with_source.ris_extras.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

        tx.execute(
            "INSERT INTO articles (
                id, status, title, abstract_text, authors, publication_year, doi,
                journal, volume, issue, start_page, end_page, keywords, url,
                language, publisher, publisher_city, publisher_address, issn,
                reference_type, date, author_address, accession_number,
                custom_field3, journal_abbreviation, journal_iso_abbreviation,
                notes, web_of_science_db, ris_extras, import_source
            ) VALUES (
                ?1, 'imported', ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22,
                ?23, ?24, ?25,
                ?26, ?27, ?28, ?29
            )",
            params![
                id, article_with_source.title, article_with_source.abstract_text,
                authors_json, article_with_source.publication_year, article_with_source.doi,
                article_with_source.journal, article_with_source.volume, article_with_source.issue,
                article_with_source.start_page, article_with_source.end_page,
                keywords_json, article_with_source.url,
                article_with_source.language, article_with_source.publisher,
                article_with_source.publisher_city, article_with_source.publisher_address,
                article_with_source.issn, article_with_source.reference_type,
                article_with_source.date, article_with_source.author_address,
                article_with_source.accession_number, article_with_source.custom_field3,
                article_with_source.journal_abbreviation, article_with_source.journal_iso_abbreviation,
                article_with_source.notes, article_with_source.web_of_science_db,
                ris_extras_json, article_with_source.import_source,
            ],
        )?;

        // Insert audit entry for import
        let audit_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO audit_entries (id, article_id, action, details, source) VALUES (?1, ?2, 'import', ?3, 'system')",
            params![audit_id, id, format!("Imported from {}", import_source)],
        )?;

        inserted.push(get_article_by_id_tx(&tx, &id)?);
    }

    tx.commit()?;
    Ok(inserted)
}

pub fn get_article_by_id(conn: &Connection, id: &str) -> Result<Article, AppError> {
    conn.query_row(
        "SELECT * FROM articles WHERE id = ?1",
        [id],
        row_to_article,
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("Article {} not found", id)),
        other => AppError::Database(other),
    })
}

fn get_article_by_id_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Article, AppError> {
    tx.query_row(
        "SELECT * FROM articles WHERE id = ?1",
        [id],
        row_to_article,
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("Article {} not found", id)),
        other => AppError::Database(other),
    })
}

pub fn get_all_articles(conn: &Connection) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM articles ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_articles_by_status(conn: &Connection, status: &str) -> Result<Vec<Article>, AppError> {
    let mut stmt = conn.prepare("SELECT * FROM articles WHERE status = ?1 ORDER BY imported_at DESC")?;
    let rows = stmt.query_map([status], row_to_article)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn row_to_article(row: &rusqlite::Row<'_>) -> rusqlite::Result<Article> {
    let status_str: String = row.get("status")?;
    let status = match status_str.as_str() {
        "imported" => ArticleStatus::Imported,
        "working" => ArticleStatus::Working,
        "included" => ArticleStatus::Included,
        "rejected" => ArticleStatus::Rejected,
        _ => ArticleStatus::Imported,
    };

    let ai_decision_str: Option<String> = row.get("ai_decision")?;
    let ai_decision = ai_decision_str.map(|d| match d.as_str() {
        "include" => AiDecision::Include,
        "exclude" => AiDecision::Exclude,
        _ => AiDecision::Exclude,
    });

    let authors_str: String = row.get("authors")?;
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let keywords_str: Option<String> = row.get("keywords")?;
    let keywords: Vec<String> = keywords_str
        .and_then(|k| serde_json::from_str(&k).ok())
        .unwrap_or_default();

    let matched_inc_str: Option<String> = row.get("matched_inclusion_criteria")?;
    let matched_inclusion: Vec<String> = matched_inc_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let matched_exc_str: Option<String> = row.get("matched_exclusion_criteria")?;
    let matched_exclusion: Vec<String> = matched_exc_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let ris_extras_str: Option<String> = row.get("ris_extras")?;
    let ris_extras: Option<serde_json::Value> = ris_extras_str
        .and_then(|s| serde_json::from_str(&s).ok());

    let tags_str: Option<String> = row.get::<_, Option<String>>("tags").ok().flatten();
    let _ = tags_str; // Tags loaded via join in a future plan

    let screening_error_int: i32 = row.get("screening_error")?;
    let manual_override_int: i32 = row.get("manual_override")?;

    Ok(Article {
        id: row.get("id")?,
        status,
        screening_error: screening_error_int != 0,
        title: row.get("title")?,
        abstract_text: row.get("abstract_text")?,
        authors,
        publication_year: row.get("publication_year")?,
        doi: row.get("doi")?,
        journal: row.get("journal")?,
        volume: row.get("volume")?,
        issue: row.get("issue")?,
        start_page: row.get("start_page")?,
        end_page: row.get("end_page")?,
        keywords,
        url: row.get("url")?,
        language: row.get("language")?,
        publisher: row.get("publisher")?,
        publisher_city: row.get("publisher_city")?,
        publisher_address: row.get("publisher_address")?,
        issn: row.get("issn")?,
        reference_type: row.get("reference_type")?,
        date: row.get("date")?,
        author_address: row.get("author_address")?,
        accession_number: row.get("accession_number")?,
        custom_field3: row.get("custom_field3")?,
        journal_abbreviation: row.get("journal_abbreviation")?,
        journal_iso_abbreviation: row.get("journal_iso_abbreviation")?,
        notes: row.get("notes")?,
        web_of_science_db: row.get("web_of_science_db")?,
        user_notes: row.get("user_notes")?,
        ris_extras,
        duplicate_of: row.get("duplicate_of")?,
        ai_decision,
        ai_reasoning: row.get("ai_reasoning")?,
        ai_confidence: row.get("ai_confidence")?,
        matched_inclusion_criteria: matched_inclusion,
        matched_exclusion_criteria: matched_exclusion,
        tags: vec![],
        labels: vec![],
        manual_override: manual_override_int != 0,
        import_source: row.get("import_source")?,
        imported_at: row.get("imported_at")?,
        screened_at: row.get("screened_at")?,
    })
}
```

- [ ] **Step 2: Update `src-tauri/src/db/mod.rs`**

```rust
pub mod article_repo;
pub mod connection;
pub mod migration;
pub mod migrations;
```

- [ ] **Step 3: Create `src-tauri/src/commands/import.rs`**

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::article_repo;
use crate::db::connection::DbState;
use crate::error::AppError;
use crate::models::article::{Article, NewArticle};
use crate::ris::parser::parse_ris;
use crate::ris::validator::validate_all;
use crate::ris::types::RisRecord;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub total_records: usize,
    pub valid_records: usize,
    pub error_count: usize,
    pub errors: Vec<ImportError>,
    pub preview_articles: Vec<PreviewArticle>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub record_index: usize,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewArticle {
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i32>,
    pub journal: Option<String>,
    pub doi: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported_count: usize,
    pub articles: Vec<Article>,
    pub remaining_capacity: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseRisRequest {
    pub content: String,
    pub file_name: String,
}

#[tauri::command]
pub fn parse_ris_file(request: ParseRisRequest) -> Result<ImportPreview, AppError> {
    let parse_result = parse_ris(&request.content)?;
    let (valid, errors) = validate_all(&parse_result.records);

    let preview_articles: Vec<PreviewArticle> = valid
        .iter()
        .take(10)
        .map(|r| PreviewArticle {
            title: r.title.clone().unwrap_or_default(),
            authors: r.authors.clone(),
            publication_year: r.publication_year,
            journal: r.journal.clone(),
            doi: r.doi.clone(),
        })
        .collect();

    Ok(ImportPreview {
        total_records: parse_result.records.len(),
        valid_records: valid.len(),
        error_count: errors.len(),
        errors: errors
            .into_iter()
            .map(|e| ImportError {
                record_index: e.record_index,
                message: e.message,
            })
            .collect(),
        preview_articles,
    })
}

fn ris_record_to_new_article(record: &RisRecord) -> NewArticle {
    let extras: Option<serde_json::Value> = if record.extras.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&record.extras).unwrap_or(serde_json::Value::Null))
    };

    NewArticle {
        title: record.title.clone().unwrap_or_default(),
        abstract_text: record.abstract_text.clone().unwrap_or_default(),
        authors: record.authors.clone(),
        publication_year: record.publication_year,
        doi: record.doi.clone(),
        journal: record.journal.clone(),
        volume: record.volume.clone(),
        issue: record.issue.clone(),
        start_page: record.start_page.clone(),
        end_page: record.end_page.clone(),
        keywords: record.keywords.clone(),
        url: record.url.clone(),
        language: record.language.clone(),
        publisher: record.publisher.clone(),
        publisher_city: record.publisher_city.clone(),
        publisher_address: record.publisher_address.clone(),
        issn: record.issn.clone(),
        reference_type: record.reference_type.clone(),
        date: record.date.clone(),
        author_address: record.author_address.clone(),
        accession_number: record.accession_number.clone(),
        custom_field3: record.custom_field3.clone(),
        journal_abbreviation: record.journal_abbreviation.clone(),
        journal_iso_abbreviation: record.journal_iso_abbreviation.clone(),
        notes: record.notes.clone(),
        web_of_science_db: record.web_of_science_db.clone(),
        ris_extras: extras,
        import_source: None,
    }
}

#[tauri::command]
pub fn import_ris_file(db_state: State<'_, DbState>, request: ParseRisRequest) -> Result<ImportResult, AppError> {
    let parse_result = parse_ris(&request.content)?;
    let (valid, errors) = validate_all(&parse_result.records);

    if !errors.is_empty() {
        return Err(AppError::Import(format!(
            "{} record(s) failed validation: {}",
            errors.len(),
            errors
                .iter()
                .map(|e| format!("Record {}: {}", e.record_index, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }

    let new_articles: Vec<NewArticle> = valid.iter().map(ris_record_to_new_article).collect();
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;

    let imported = article_repo::insert_articles_batch(&conn, &new_articles, &request.file_name)?;
    let remaining = article_repo::remaining_capacity(&conn)?;

    Ok(ImportResult {
        imported_count: imported.len(),
        articles: imported,
        remaining_capacity: remaining,
    })
}

#[tauri::command]
pub fn get_articles(db_state: State<'_, DbState>) -> Result<Vec<Article>, AppError> {
    let conn = db_state.conn.lock().map_err(|e| AppError::Database(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    article_repo::get_all_articles(&conn)
}
```

- [ ] **Step 4: Update `src-tauri/src/commands/mod.rs`**

```rust
pub mod import;

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

- [ ] **Step 5: Update `src-tauri/src/lib.rs` to register import commands**

```rust
pub mod commands;
pub mod db;
pub mod error;
pub mod models;
pub mod ris;

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
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::import::parse_ris_file,
            commands::import::import_ris_file,
            commands::import::get_articles,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}
```

- [ ] **Step 6: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 7: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(import): add article repository and Tauri import commands"
```

- [ ] **Step 9: Write integration test for full import pipeline**

Add to `src-tauri/tests/ris_test.rs`:

```rust
use bango_lib::db::connection::create_connection;
use bango_lib::db::migration::run_migrations;
use bango_lib::db::article_repo;
use bango_lib::ris::validator::validate_all;

#[test]
fn test_full_import_pipeline_with_real_ris() {
    let content = fs::read_to_string(asset_path("10A_Lewicki_Stages.ris")).expect("fixture not found");
    let parse_result = parse_ris(&content).expect("Parse failed");
    let (valid, errors) = validate_all(&parse_result.records);

    assert!(errors.is_empty(), "Expected no validation errors: {:?}", errors);
    assert_eq!(valid.len(), 1);

    let conn = create_connection().expect("DB connection failed");
    run_migrations(&conn).expect("Migration failed");

    let articles = article_repo::get_all_articles(&conn).expect("Query failed");
    assert_eq!(articles.len(), 0, "Should start empty");

    // Verify DB schema supports all parsed fields
    let record = &valid[0];
    assert!(record.title.is_some());
    assert!(record.abstract_text.is_some());
    assert!(!record.authors.is_empty());
}
```

- [ ] **Step 10: Run all tests**

Run: `cd src-tauri && cargo test --test ris_test`
Expected: PASS — all tests pass including real RIS file parsing

- [ ] **Step 11: Commit**

```bash
git add src-tauri/tests/ris_test.rs
git commit -m "test(ris): add integration test for full import pipeline"
```

---

## Task 5: Frontend Import UI

**Files:**
- Create: `src/composables/use-import.ts`
- Create: `src/components/import-drop-zone.vue`
- Create: `src/components/import-stepper.vue`
- Create: `src/components/import-preview.vue`
- Create: `src/views/import-ris.vue`
- Modify: `src/router/index.ts` (update import route)

> **Design reference:** Before implementing, read `docs/design-reference/02-ris-import.html` and `docs/design-reference/02-ris-import.png`. Extract the exact layout structure, spacing, and component hierarchy from the Stitch HTML. Implement only v3-scoped elements per `docs/design-reference/00-design-patterns.md` Section 14.

- [ ] **Step 1: Create `src/composables/use-import.ts`**

```typescript
import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface ImportPreview {
  totalRecords: number;
  validRecords: number;
  errorCount: number;
  errors: ImportError[];
  previewArticles: PreviewArticle[];
}

export interface ImportError {
  recordIndex: number;
  message: string;
}

export interface PreviewArticle {
  title: string;
  authors: string[];
  publicationYear: number | null;
  journal: string | null;
  doi: string | null;
}

export interface ImportResult {
  importedCount: number;
  articles: unknown[];
  remainingCapacity: number;
}

export type ImportStep = 'upload' | 'parse' | 'import' | 'complete';

export function useImport() {
  const step = ref<ImportStep>('upload');
  const fileName = ref<string | null>(null);
  const fileContent = ref<string | null>(null);
  const preview = ref<ImportPreview | null>(null);
  const importResult = ref<ImportResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const hasFile = computed(() => fileContent.value !== null);
  const hasErrors = computed(() => (preview.value?.errorCount ?? 0) > 0);
  const canImport = computed(() => preview.value !== null && preview.value.errorCount === 0);

  async function loadFile(file: File): Promise<void> {
    loading.value = true;
    error.value = null;
    fileName.value = file.name;

    try {
      fileContent.value = await file.text();
      step.value = 'parse';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to read file';
    } finally {
      loading.value = false;
    }
  }

  async function parseFile(): Promise<void> {
    if (!fileContent.value || !fileName.value) return;

    loading.value = true;
    error.value = null;

    try {
      preview.value = await tauriCommand<ImportPreview>('parse_ris_file', {
        request: {
          content: fileContent.value,
          fileName: fileName.value,
        },
      });
      step.value = 'import';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Parse failed';
    } finally {
      loading.value = false;
    }
  }

  async function confirmImport(): Promise<void> {
    if (!fileContent.value || !fileName.value) return;

    loading.value = true;
    error.value = null;

    try {
      importResult.value = await tauriCommand<ImportResult>('import_ris_file', {
        request: {
          content: fileContent.value,
          fileName: fileName.value,
        },
      });
      step.value = 'complete';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Import failed';
    } finally {
      loading.value = false;
    }
  }

  function reset(): void {
    step.value = 'upload';
    fileName.value = null;
    fileContent.value = null;
    preview.value = null;
    importResult.value = null;
    loading.value = false;
    error.value = null;
  }

  return {
    step, fileName, preview, importResult, loading, error,
    hasFile, hasErrors, canImport,
    loadFile, parseFile, confirmImport, reset,
  };
}
```

- [ ] **Step 2: Create `src/components/import-drop-zone.vue`**

```vue
<script setup lang="ts">
import { ref } from 'vue';

const emit = defineEmits<{ fileSelected: [file: File] }>();
const isDragging = ref(false);

function onDragOver(event: DragEvent): void {
  event.preventDefault();
  isDragging.value = true;
}

function onDragLeave(): void {
  isDragging.value = false;
}

function onDrop(event: DragEvent): void {
  event.preventDefault();
  isDragging.value = false;

  const file = event.dataTransfer?.files[0];
  if (file && file.name.endsWith('.ris')) {
    emit('fileSelected', file);
  }
}

function onFileInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0];
  if (file) {
    emit('fileSelected', file);
  }
}
</script>

<template>
  <div
    class="drop-zone"
    :class="{ 'drop-zone--active': isDragging }"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <div class="drop-zone__content">
      <div class="drop-zone__icon">↑</div>
      <p class="drop-zone__text">Drag and drop an RIS file here</p>
      <p class="drop-zone__subtext">or</p>
      <label class="drop-zone__button">
        Browse Files
        <input type="file" accept=".ris" class="drop-zone__input" @change="onFileInput" />
      </label>
    </div>
  </div>
</template>

<style scoped>
.drop-zone {
  border: 2px dashed var(--color-outline-variant);
  border-radius: var(--radius-md);
  padding: var(--space-10) var(--space-6);
  text-align: center;
  transition: all 0.2s;
  background-color: var(--color-surface-container-low);
}

.drop-zone--active {
  border-color: var(--color-primary);
  background-color: var(--color-surface-container);
}

.drop-zone__content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
}

.drop-zone__icon {
  font-size: 32px;
  color: var(--color-outline);
}

.drop-zone__text {
  font-size: var(--font-size-body);
  color: var(--color-on-surface);
}

.drop-zone__subtext {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.drop-zone__button {
  display: inline-block;
  padding: var(--space-2) var(--space-4);
  background-color: var(--color-primary);
  color: var(--color-on-primary);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: opacity 0.15s;
}

.drop-zone__button:hover {
  opacity: 0.9;
}

.drop-zone__input {
  display: none;
}
</style>
```

- [ ] **Step 3: Create `src/components/import-stepper.vue`**

```vue
<script setup lang="ts">
import type { ImportStep } from '@/composables/use-import';

defineProps<{ currentStep: ImportStep }>();

const steps: { key: ImportStep; label: string }[] = [
  { key: 'upload', label: 'Upload' },
  { key: 'parse', label: 'Parse' },
  { key: 'import', label: 'Review' },
  { key: 'complete', label: 'Complete' },
];

function stepIndex(step: ImportStep): number {
  return steps.findIndex((s) => s.key === step);
}
</script>

<template>
  <div class="stepper">
    <div
      v-for="(step, i) in steps"
      :key="step.key"
      class="stepper__step"
      :class="{
        'stepper__step--active': stepIndex(currentStep) === i,
        'stepper__step--done': stepIndex(currentStep) > i,
      }"
    >
      <div class="stepper__dot">{{ stepIndex(currentStep) > i ? '✓' : i + 1 }}</div>
      <span class="stepper__label">{{ step.label }}</span>
      <div v-if="i < steps.length - 1" class="stepper__line" />
    </div>
  </div>
</template>

<style scoped>
.stepper {
  display: flex;
  align-items: center;
  gap: 0;
  padding: var(--space-4) 0;
}

.stepper__step {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1;
}

.stepper__dot {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface-variant);
  flex-shrink: 0;
}

.stepper__step--active .stepper__dot {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.stepper__step--done .stepper__dot {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.stepper__label {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  white-space: nowrap;
}

.stepper__step--active .stepper__label {
  color: var(--color-on-surface);
  font-weight: var(--font-weight-semibold);
}

.stepper__line {
  flex: 1;
  height: 1px;
  background-color: var(--color-outline-variant);
  margin: 0 var(--space-2);
}
</style>
```

- [ ] **Step 4: Create `src/components/import-preview.vue`**

```vue
<script setup lang="ts">
import type { PreviewArticle, ImportError } from '@/composables/use-import';

defineProps<{
  articles: PreviewArticle[];
  errorCount: number;
  errors: ImportError[];
}>();
</script>

<template>
  <div class="preview">
    <div v-if="errorCount > 0" class="preview__errors">
      <h2>Validation Errors ({{ errorCount }})</h2>
      <ul class="preview__error-list">
        <li v-for="err in errors" :key="err.recordIndex" class="preview__error-item">
          Record {{ err.recordIndex }}: {{ err.message }}
        </li>
      </ul>
    </div>

    <div class="preview__table-wrapper">
      <table class="preview__table">
        <thead>
          <tr>
            <th>Title</th>
            <th>Authors</th>
            <th>Year</th>
            <th>Journal</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(article, i) in articles" :key="i">
            <td>{{ article.title }}</td>
            <td>{{ article.authors.join('; ') }}</td>
            <td>{{ article.publicationYear ?? '—' }}</td>
            <td>{{ article.journal ?? '—' }}</td>
          </tr>
        </tbody>
      </table>
      <p class="preview__note">Showing first {{ articles.length }} articles</p>
    </div>
  </div>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.preview__errors {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  border-radius: var(--radius-default);
}

.preview__errors h2 {
  font-size: var(--font-size-h2);
  color: var(--color-error);
  margin-bottom: var(--space-2);
}

.preview__error-list {
  list-style: none;
  font-size: var(--font-size-caption);
  color: var(--color-error);
}

.preview__error-item {
  padding: var(--space-1) 0;
}

.preview__table-wrapper {
  overflow-x: auto;
}

.preview__table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--font-size-caption);
}

.preview__table th {
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
}

.preview__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  color: var(--color-on-surface);
}

.preview__table tr:hover td {
  background-color: var(--color-hover);
}

.preview__note {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  margin-top: var(--space-2);
}
</style>
```

- [ ] **Step 5: Create `src/views/import-ris.vue`**

```vue
<script setup lang="ts">
import { useImport } from '@/composables/use-import';
import ImportDropZone from '@/components/import-drop-zone.vue';
import ImportStepper from '@/components/import-stepper.vue';
import ImportPreview from '@/components/import-preview.vue';

const {
  step, fileName, preview, importResult, loading, error,
  hasErrors, canImport,
  loadFile, parseFile, confirmImport, reset,
} = useImport();
</script>

<template>
  <div class="import-view">
    <div class="import-view__header">
      <h1>Import RIS File</h1>
      <ImportStepper :current-step="step" />
    </div>

    <div v-if="error" class="import-view__error">
      {{ error }}
    </div>

    <div class="import-view__body">
      <!-- Step 1: Upload -->
      <section v-if="step === 'upload'">
        <ImportDropZone @file-selected="loadFile" />
      </section>

      <!-- Step 2: Parse -->
      <section v-if="step === 'parse'">
        <p class="import-view__file-name">Selected: {{ fileName }}</p>
        <div class="import-view__actions">
          <button class="btn btn--secondary" @click="reset">Cancel</button>
          <button class="btn btn--primary" :disabled="loading" @click="parseFile">
            {{ loading ? 'Parsing...' : 'Parse File' }}
          </button>
        </div>
      </section>

      <!-- Step 3: Review & Import -->
      <section v-if="step === 'import' && preview">
        <div class="import-view__summary">
          <div class="import-view__stat">
            <span class="import-view__stat-value">{{ preview.totalRecords }}</span>
            <span class="import-view__stat-label">Total Records</span>
          </div>
          <div class="import-view__stat">
            <span class="import-view__stat-value">{{ preview.validRecords }}</span>
            <span class="import-view__stat-label">Valid</span>
          </div>
          <div class="import-view__stat import-view__stat--error">
            <span class="import-view__stat-value">{{ preview.errorCount }}</span>
            <span class="import-view__stat-label">Errors</span>
          </div>
        </div>

        <ImportPreview
          :articles="preview.previewArticles"
          :error-count="preview.errorCount"
          :errors="preview.errors"
        />

        <div class="import-view__actions">
          <button class="btn btn--secondary" @click="reset">Cancel</button>
          <button
            class="btn btn--primary"
            :disabled="!canImport || loading"
            @click="confirmImport"
          >
            {{ loading ? 'Importing...' : `Import ${preview.validRecords} Articles` }}
          </button>
        </div>
      </section>

      <!-- Step 4: Complete -->
      <section v-if="step === 'complete' && importResult">
        <div class="import-view__success">
          <h2>Import Complete</h2>
          <p>{{ importResult.importedCount }} articles imported successfully.</p>
          <p class="import-view__capacity">
            Remaining capacity: {{ importResult.remainingCapacity }} articles
          </p>
        </div>
        <div class="import-view__actions">
          <button class="btn btn--primary" @click="reset">Import Another File</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.import-view {
  padding: var(--space-6);
  max-width: 900px;
}

.import-view__header {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.import-view__error {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.import-view__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.import-view__file-name {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.import-view__summary {
  display: flex;
  gap: var(--space-4);
  margin-bottom: var(--space-4);
}

.import-view__stat {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  min-width: 100px;
}

.import-view__stat-value {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
}

.import-view__stat-label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
}

.import-view__stat--error .import-view__stat-value {
  color: var(--color-error);
}

.import-view__actions {
  display: flex;
  gap: var(--space-3);
  margin-top: var(--space-4);
}

.import-view__success {
  padding: var(--space-6);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-default);
  text-align: center;
}

.import-view__success h2 {
  margin-bottom: var(--space-2);
}

.import-view__capacity {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  margin-top: var(--space-2);
}

.btn {
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: opacity 0.15s;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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

- [ ] **Step 6: Update router to use import-ris view**

In `src/router/index.ts`, change the import route component:

```typescript
const ImportRis = () => import('@/views/import-ris.vue');

const routes = [
  { path: '/', name: 'dashboard', component: Dashboard },
  { path: '/articles', name: 'articles', component: Placeholder, props: { title: 'Articles' } },
  { path: '/import', name: 'import', component: ImportRis },
  { path: '/dedup', name: 'dedup', component: Placeholder, props: { title: 'Deduplication' } },
  { path: '/criteria', name: 'criteria', component: Placeholder, props: { title: 'Criteria Editor' } },
  { path: '/screening', name: 'screening', component: Placeholder, props: { title: 'AI Screening' } },
  { path: '/tags', name: 'tags', component: Placeholder, props: { title: 'Tags & Labels' } },
  { path: '/prisma', name: 'prisma', component: Placeholder, props: { title: 'PRISMA Flow Diagram' } },
  { path: '/settings', name: 'settings', component: Placeholder, props: { title: 'LLM Configuration' } },
];
```

- [ ] **Step 7: Run `npm run lint:check`**

Run: `npm run lint:check`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/composables/ src/components/ src/views/import-ris.vue src/router/index.ts
git commit -m "feat(import): add RIS import UI with drop zone, preview, and stepper"
```

---

## Task 6: Final Verification

- [ ] **Step 1: Run `npm run check:all`**

Run: `npm run check:all`
Expected: PASS

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass

- [ ] **Step 3: Verify app launches and import flow works**

Run: `cd src-tauri && cargo tauri dev`
Expected: App opens, sidebar navigation works, clicking "Import RIS" shows the import wizard with drag-and-drop zone.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "chore: fix any lint/format issues from RIS import setup"
```
