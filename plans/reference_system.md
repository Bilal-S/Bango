# Reference System — Citation & Reference Tracking

> **Status**: Planning  
> **Created**: 2025-06-05  
> **Phases**: Phase 1 (Database) → Phase 2 (Import & UI)

---

## Table of Contents

1. [Overview](#1-overview)
2. [Phase 1: Database Layer](#2-phase-1-database-layer)
   - [1A. Migration v004 — Articles Table Columns](#1a-migration-v004--articles-table-columns)
   - [1B. New `references` Table](#1b-new-references-table)
   - [1C. Rust Model Changes](#1c-rust-model-changes)
   - [1D. N1 Citation Data Parsing](#1d-n1-citation-data-parsing)
   - [1E. Repository Layer Changes](#1e-repository-layer-changes)
   - [1F. TypeScript Type Updates](#1f-typescript-type-updates)
   - [1G. Phase 1 Test Plan](#1g-phase-1-test-plan)
3. [Phase 2: Import & UI](#3-phase-2-import--ui)
   - [2A. N1 Parsing Wired Into Import](#2a-n1-parsing-wired-into-import)
   - [2B. RIS Reference Import Pipeline](#2b-ris-reference-import-pipeline)
   - [2C. CSV Reference Import Pipeline](#2c-csv-reference-import-pipeline)
   - [2D. has_citation_details / has_reference_details Auto-Update](#2d-auto-update-flags)
   - [2E. Full-Text File Association](#2e-full-text-file-association)
   - [2F. UI Changes](#2f-ui-changes)
   - [2G. Phase 2 Test Plan](#2g-phase-2-test-plan)
4. [Files Affected](#4-files-affected)
5. [Implementation Order](#5-implementation-order)

---

## 1. Overview

### Problem

Web of Science (WoS) RIS exports contain structured citation metadata in the `N1` field that is currently stored as plain text in `notes`. Additionally, cited-reference and citing-article detail files (RIS or CSV) provide rich citation network data that has no storage or UI representation.

### Goals

1. **Parse structured citation counts from N1** during import (num_cited, num_references)
2. **Track full-text file associations** per article
3. **Create a `references` table** to store individual citation and reference detail records
4. **Support future import pipelines** for RIS and CSV reference detail files
5. **Support a promotion workflow** where unmatched references can be matched to existing articles or promoted to full articles

### Key Terms

| Term | Definition |
|------|-----------|
| **Citation** | An external article that *cites* the parent article (type=0) |
| **Reference** | An external article *cited by* the parent article (type=1) |
| **Parent article** | An article in the `articles` table that has citation/reference details |
| **Match status** | Whether a reference record has been linked to an article in the main table |

---

## 2. Phase 1: Database Layer

### 1A. Migration v004 — Articles Table Columns

**File**: `src-tauri/src/db/migrations/v004_article_references.rs`

Six new columns on the `articles` table:

```sql
-- Citation counts extracted from N1 during import
ALTER TABLE articles ADD COLUMN num_cited INTEGER;
ALTER TABLE articles ADD COLUMN num_references INTEGER;

-- Flags updated when detail records exist in references table
ALTER TABLE articles ADD COLUMN has_citation_details INTEGER NOT NULL DEFAULT 0;
ALTER TABLE articles ADD COLUMN has_reference_details INTEGER NOT NULL DEFAULT 0;

-- Full-text file tracking
ALTER TABLE articles ADD COLUMN has_full_text INTEGER NOT NULL DEFAULT 0;
ALTER TABLE articles ADD COLUMN full_text_file_name TEXT;
```

| Column | SQLite Type | Rust Type | Default | Description |
|--------|------------|-----------|---------|-------------|
| `num_cited` | `INTEGER` | `Option<i32>` | `NULL` | Total times cited (from N1: `Total Times Cited: NN`) |
| `num_references` | `INTEGER` | `Option<i32>` | `NULL` | Number of references (from N1: `Cited Reference Count: NN`) |
| `has_citation_details` | `INTEGER` | `bool` | `0` | True when citation detail records exist in `references` (type=0) |
| `has_reference_details` | `INTEGER` | `INTEGER` | `0` | True when reference detail records exist in `references` (type=1) |
| `has_full_text` | `INTEGER` | `bool` | `0` | True when a full-text file is associated |
| `full_text_file_name` | `TEXT` | `Option<String>` | `NULL` | Relative path with partial subpath (e.g., `fulltext/smith2023.pdf`) |

**Index**: No new indexes needed on `articles` for these columns — they are lookups by article ID (primary key).

### 1B. New `references` Table

**Included in the same migration v004.**

```sql
CREATE TABLE IF NOT EXISTS references (
    id TEXT PRIMARY KEY,
    type INTEGER NOT NULL CHECK(type IN (0, 1)),
        -- 0 = citation (another article citing the parent)
        -- 1 = reference (a work cited by the parent)
    parent_id TEXT NOT NULL,
    match_status TEXT NOT NULL DEFAULT 'unmatched'
        CHECK(match_status IN ('unmatched', 'matched', 'imported')),
        -- 'unmatched': no link to an article in the main table
        -- 'matched': linked to an existing article via DOI/title match
        -- 'imported': promoted to a full article in the articles table

    -- Metadata fields (mirroring articles, but all nullable for incomplete data)
    title TEXT,
    abstract_text TEXT,
    authors TEXT,                     -- JSON array of strings
    publication_year INTEGER,
    doi TEXT,
    journal TEXT,
    volume TEXT,
    issue TEXT,
    start_page TEXT,
    end_page TEXT,
    keywords TEXT,                    -- JSON array of strings
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
    ris_extras TEXT,                  -- JSON object for unrecognized tags

    -- Citation counts (reference records may also carry these)
    num_cited INTEGER,
    num_references INTEGER,

    -- Full-text tracking
    has_full_text INTEGER NOT NULL DEFAULT 0,
    full_text_file_name TEXT,

    -- Import tracking
    import_source TEXT,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (parent_id) REFERENCES articles(id) ON DELETE CASCADE
);

-- Primary lookup: all references/citations for a given article
CREATE INDEX IF NOT EXISTS idx_references_parent_type
    ON references(parent_id, type);

-- DOI lookup for matching references to existing articles
CREATE INDEX IF NOT EXISTS idx_references_doi
    ON references(doi);

-- Filter by match status for promotion workflow
CREATE INDEX IF NOT EXISTS idx_references_match_status
    ON references(match_status);
```

#### Design Decisions

1. **`type` as INTEGER 0/1** — Efficient for filtering and indexing. 0 = citation (this external paper cites the parent), 1 = reference (the parent cites this external paper).

2. **Composite index `(parent_id, type)`** — This is the most frequent query pattern: "give me all references for article X" or "give me all citations for article X". The composite index covers both without a separate lookup.

3. **`match_status` enum** — Supports the future promotion workflow:
   - `unmatched` → no known link to the main articles table
   - `matched` → DOI or title matched to an existing article (store article ID separately or via DOI join)
   - `imported` → the reference was promoted to a full article record

4. **No AI screening columns** — Reference records are metadata-only, not screened for inclusion/exclusion. No `status`, `ai_decision`, `ai_confidence`, `screened_at`, etc.

5. **No `sequence_id`** — Ordering is by `imported_at` or natural order from the source file.

6. **No `tags`/`labels` junction tables** — Reference records don't have tags or labels. These are tracking metadata only.

7. **All article metadata columns nullable** — Citation/reference exports often contain only partial data (title, authors, year, DOI). Abstracts are rare in these exports.

8. **CASCADE delete** — When a parent article is deleted, all its reference records are automatically removed.

### 1C. Rust Model Changes

#### `src-tauri/src/models/article.rs`

Add 6 new fields to `Article`:

```rust
pub struct Article {
    // ... existing fields ...

    // --- NEW FIELDS ---
    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,
    pub has_citation_details: bool,
    pub has_reference_details: bool,
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,
}
```

Add 6 new fields to `NewArticle`:

```rust
pub struct NewArticle {
    // ... existing fields ...

    // --- NEW FIELDS ---
    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,
    // Note: has_citation_details and has_reference_details default to false
    // and are only set when reference records are inserted, not during article import
}
```

#### `src-tauri/src/models/reference.rs` (NEW FILE)

```rust
use serde::{Deserialize, Serialize};

/// The type of reference record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceType {
    /// Another article that cites the parent article
    Citation,
    /// A work cited by the parent article
    Reference,
}

impl ReferenceType {
    #[must_use]
    pub fn as_int(&self) -> i32 {
        match self {
            Self::Citation => 0,
            Self::Reference => 1,
        }
    }

    #[must_use]
    pub fn from_int(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Citation),
            1 => Some(Self::Reference),
            _ => None,
        }
    }
}

/// Match status for a reference record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    Unmatched,
    Matched,
    Imported,
}

impl MatchStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmatched => "unmatched",
            Self::Matched => "matched",
            Self::Imported => "imported",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unmatched" => Some(Self::Unmatched),
            "matched" => Some(Self::Matched),
            "imported" => Some(Self::Imported),
            _ => None,
        }
    }
}

/// A reference/citation detail record linked to a parent article.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub reference_type: ReferenceType,
    pub parent_id: String,
    pub match_status: MatchStatus,

    // Metadata (all optional — reference exports often have partial data)
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
    pub reference_type_field: Option<String>,
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

    // Citation counts
    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,

    // Full-text tracking
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,

    // Import tracking
    pub import_source: Option<String>,
    pub imported_at: String,
}

/// A new reference record to be inserted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NewReference {
    pub reference_type: ReferenceType,
    pub parent_id: String,
    pub match_status: MatchStatus,

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
    pub reference_type_field: Option<String>,
    pub date: Option<String>,
    pub author_address: Option<String>,
    pub accession_number: Option<String>,
    pub custom_field3: Option<String>,
    pub journal_abbreviation: Option<String>,
    pub journal_iso_abbreviation: Option<String>,
    pub notes: Option<String>,
    pub web_of_science_db: Option<String>,
    pub ris_extras: Option<serde_json::Value>,

    pub num_cited: Option<i32>,
    pub num_references: Option<i32>,
    pub has_full_text: bool,
    pub full_text_file_name: Option<String>,
    pub import_source: Option<String>,
}
```

#### `src-tauri/src/ris/types.rs`

Add two fields to `RisRecord`:

```rust
pub struct RisRecord {
    // ... existing fields ...

    /// Total times cited, extracted from N1 field
    pub num_cited: Option<i32>,
    /// Number of cited references, extracted from N1 field
    pub num_references: Option<i32>,
}
```

### 1D. N1 Citation Data Parsing

#### Specification

The N1 field in WoS RIS exports can contain structured citation data. The parser must extract:
- `Total Times Cited: NN` → `num_cited`
- `Cited Reference Count: NN` → `num_references`

#### Input Formats to Handle

**Format 1: Standard WoS multi-line**
```
Times Cited in Web of Science Core Collection:  44
Total Times Cited:  49
Cited Reference Count:  34
```

**Format 2: Single-line compact**
```
Total Times Cited: 49 Cited Reference Count: 34
```

**Format 3: Mixed with user notes**
```
Important paper for methodology review
Total Times Cited:  49
Cited Reference Count:  34
```

**Format 4: Only one field present**
```
Cited Reference Count: 12
```

**Format 5: No citation data (plain note)**
```
This is a regular note with no citation data
```

**Format 6: With extra whitespace**
```
Total   Times   Cited:    49
```

**Format 7: Zero values**
```
Total Times Cited: 0
Cited Reference Count: 0
```

#### Implementation

**File**: `src-tauri/src/ris/n1_parser.rs` (NEW FILE)

```rust
/// Parses citation data from an N1 (Notes) field value.
///
/// Returns (num_cited, num_references) where each is `Some(count)` if found,
/// or `None` if the field was not present in the N1 value.
///
/// The N1 value is preserved in full in the article's `notes` field regardless
/// of whether citation data was extracted.
pub fn parse_n1_citation_data(n1_value: &str) -> (Option<i32>, Option<i32>) {
    let mut num_cited: Option<i32> = None;
    let mut num_references: Option<i32> = None;

    // Match "Total Times Cited:" followed by optional whitespace and digits
    // Case-insensitive, handles multi-line and inline
    for line in n1_value.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("Total Times Cited:") {
            if let Ok(val) = rest.trim().parse::<i32>() {
                num_cited = Some(val);
            }
        }

        if let Some(rest) = trimmed.strip_prefix("Cited Reference Count:") {
            if let Ok(val) = rest.trim().parse::<i32>() {
                num_references = Some(val);
            }
        }
    }

    (num_cited, num_references)
}
```

**Key behaviors**:
- Case-sensitive matching (WoS exports use exact casing shown above)
- Extracts only the first match for each field
- Handles multi-line N1 values (each line checked independently)
- Returns `(None, None)` if no citation data found — the N1 is still stored in `notes`
- Does not modify the N1 value — full text preserved in `notes`

**Wiring**: Called in `ris_record_to_new_article()` after setting `notes`:

```rust
let (num_cited, num_references) = record.notes
    .as_deref()
    .map(|n| parse_n1_citation_data(n))
    .unwrap_or((None, None));
```

#### N1 Parser Unit Tests

**File**: `src-tauri/src/ris/n1_parser.rs` (inline `#[cfg(test)]` module)

| Test Case | Input | Expected Output |
|-----------|-------|----------------|
| `standard_wos_format` | Full 3-line WoS N1 | `(Some(49), Some(34))` |
| `single_line` | Both fields on one line | `(Some(49), Some(34))` |
| `mixed_with_notes` | Notes before citation data | `(Some(49), Some(34))` |
| `only_cited_count` | Only `Total Times Cited` | `(Some(12), None)` |
| `only_reference_count` | Only `Cited Reference Count` | `(None, Some(7))` |
| `no_citation_data` | Plain text note | `(None, None)` |
| `empty_string` | `""` | `(None, None)` |
| `zero_values` | Both counts are 0 | `(Some(0), Some(0))` |
| `extra_whitespace` | Multiple spaces around colon/values | `(Some(49), Some(34))` |
| `times_cited_only_first_line` | Times Cited on first line only | `(Some(44), None)` |
| `large_numbers` | Counts in thousands | `(Some(1234), Some(5678))` |
| `negative_value_treated_as_none` | `Total Times Cited: -1` | `(None, None)` — negative parsed but treated as absent (edge case decision: actually Rust `parse::<i32>()` will parse `-1` successfully, but WoS never outputs negative. We accept the parsed value.) |
| `non_numeric_value` | `Total Times Cited: N/A` | `(None, None)` |
| `duplicate_keys` | Same key twice | First value wins |
| `embedded_in_longer_text` | Citation data mid-sentence | May not match with `strip_prefix` — this is by design. We only match lines that START with the key. |

> **Design note**: We use `strip_prefix` on each trimmed line rather than regex. This is intentional — it's faster, has no external dependency, and WoS N1 values always have the key at the start of a line.

### 1E. Repository Layer Changes

#### `src-tauri/src/db/article_repo.rs`

**Changes to `insert_article()`**:
- Add 6 new params to INSERT statement: `num_cited, num_references, has_citation_details, has_reference_details, has_full_text, full_text_file_name`

**Changes to `insert_articles_batch()`**:
- Same 6 new params in batch INSERT

**Changes to `row_to_article()`**:
- Read 6 new columns from the row

**Changes to `get_next_unscreened_working_batch()`**:
- Add default values for the 6 new fields in the screening batch query (these fields are not needed for screening, so just use `None`/`false`)

**No new query functions needed for Phase 1** — the new columns are read alongside existing columns.

#### `src-tauri/src/db/reference_repo.rs` (NEW FILE)

```rust
// CRUD operations for the references table

/// Insert a single reference record
pub fn insert_reference(conn: &Connection, reference: &NewReference) -> Result<Reference, AppError>

/// Insert multiple reference records in a transaction
pub fn insert_references_batch(
    conn: &Connection,
    references: &[NewReference],
    import_source: &str,
) -> Result<Vec<Reference>, AppError>

/// Get all references for a parent article
pub fn get_references_by_parent(conn: &Connection, parent_id: &str) -> Result<Vec<Reference>, AppError>

/// Get references for a parent article filtered by type
pub fn get_references_by_parent_and_type(
    conn: &Connection,
    parent_id: &str,
    ref_type: i32,  // 0=citation, 1=reference
) -> Result<Vec<Reference>, AppError>

/// Count references for a parent article by type
pub fn count_references_by_parent_and_type(
    conn: &Connection,
    parent_id: &str,
    ref_type: i32,
) -> Result<usize, AppError>

/// Update match status for a reference record
pub fn update_match_status(
    conn: &Connection,
    reference_id: &str,
    new_status: &str,
) -> Result<(), AppError>

/// Find references with a specific DOI (for matching)
pub fn find_references_by_doi(
    conn: &Connection,
    doi: &str,
) -> Result<Vec<Reference>, AppError>

/// Get references by match status
pub fn get_references_by_match_status(
    conn: &Connection,
    status: &str,
) -> Result<Vec<Reference>, AppError>

/// Delete all references for a parent article
pub fn delete_references_by_parent(
    conn: &Connection,
    parent_id: &str,
) -> Result<usize, AppError>

/// Update has_citation_details / has_reference_details flags on parent article
pub fn sync_parent_reference_flags(
    conn: &Connection,
    parent_id: &str,
) -> Result<(), AppError>
```

The `sync_parent_reference_flags` function queries:
```sql
SELECT COUNT(*) FROM references WHERE parent_id = ?1 AND type = 0  -- citations
SELECT COUNT(*) FROM references WHERE parent_id = ?1 AND type = 1  -- references
```
Then updates the parent article's `has_citation_details` and `has_reference_details` accordingly.

#### `src-tauri/src/db/migrations/mod.rs`

Register v004:

```rust
pub mod v004_article_references;

pub fn get_migrations() -> Vec<Migration> {
    vec![
        // ... existing ...
        Migration { version: v004_article_references::VERSION, up_sql: v004_article_references::UP_SQL },
    ]
}
```

### 1F. TypeScript Type Updates

#### `src/types/index.ts`

Add new fields to `Article` interface:

```typescript
export interface Article {
  // ... existing fields ...

  // Citation tracking
  numCited: number | null;
  numReferences: number | null;
  hasCitationDetails: boolean;
  hasReferenceDetails: boolean;

  // Full-text tracking
  hasFullText: boolean;
  fullTextFileName: string | null;
}
```

Add new `Reference` types:

```typescript
export type ReferenceType = 'citation' | 'reference';
export type MatchStatus = 'unmatched' | 'matched' | 'imported';

export interface Reference {
  id: string;
  referenceType: ReferenceType;
  parentId: string;
  matchStatus: MatchStatus;

  // Metadata (all optional)
  title: string | null;
  abstractText: string | null;
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
  referenceTypeField: string | null;
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

  // Citation counts
  numCited: number | null;
  numReferences: number | null;

  // Full-text tracking
  hasFullText: boolean;
  fullTextFileName: string | null;

  // Import tracking
  importSource: string | null;
  importedAt: string;
}
```

### 1G. Phase 1 Test Plan

#### 1G-1. Migration Tests

**File**: `src-tauri/tests/migration_test.rs` (or inline)

| Test | Description |
|------|-------------|
| `v004_adds_article_columns` | Run migration on empty DB, verify all 6 new columns exist on `articles` |
| `v004_creates_references_table` | Verify `references` table exists with correct schema |
| `v004_creates_indexes` | Verify `idx_references_parent_type`, `idx_references_doi`, `idx_references_match_status` exist |
| `v004_idempotent` | Running migration twice does not fail |
| `v004_preserves_existing_data` | Insert article before migration, run migration, verify data intact + new columns have defaults |

#### 1G-2. N1 Parser Tests

**File**: `src-tauri/src/ris/n1_parser.rs` (inline `#[cfg(test)]`)

| Test | Input | Expected `(num_cited, num_references)` |
|------|-------|---------------------------------------|
| `standard_wos_format` | 3-line WoS N1 with all fields | `(Some(49), Some(34))` |
| `only_total_times_cited` | `Total Times Cited: 12` | `(Some(12), None)` |
| `only_cited_ref_count` | `Cited Reference Count: 7` | `(None, Some(7))` |
| `mixed_with_notes` | Notes + citation data | `(Some(49), Some(34))` |
| `no_citation_data` | `Just a plain note` | `(None, None)` |
| `empty_string` | `""` | `(None, None)` |
| `zero_values` | Both zero | `(Some(0), Some(0))` |
| `non_numeric` | `Total Times Cited: N/A` | `(None, None)` |
| `duplicate_keys` | Same key appears twice | First value wins |
| `large_numbers` | Thousands | `(Some(1234), Some(5678))` |
| `extra_whitespace` | Multiple spaces | `(Some(49), Some(34))` |
| `only_core_collection` | `Times Cited in Web of Science Core Collection: 44` without Total line | `(None, None)` — we only match `Total Times Cited:` |

#### 1G-3. Article Repo Tests

**File**: `src-tauri/tests/article_repo_test.rs` (update existing)

| Test | Description |
|------|-------------|
| `insert_article_with_citation_data` | Insert article with `num_cited=5, num_references=20`, verify round-trip |
| `insert_article_without_citation_data` | Insert article with `None` for citation fields, verify defaults |
| `batch_insert_preserves_citation_data` | Batch insert with mixed citation data |
| `query_articles_returns_new_fields` | Query articles and verify new fields are populated |
| `row_to_article_handles_null_citation_fields` | Read article with all NULL citation columns |

#### 1G-4. Reference Repo Tests

**File**: `src-tauri/tests/reference_repo_test.rs` (NEW)

| Test | Description |
|------|-------------|
| `insert_reference_citation` | Insert type=0 (citation), verify round-trip |
| `insert_reference_reference` | Insert type=1 (reference), verify round-trip |
| `insert_references_batch` | Batch insert mixed types, verify all present |
| `get_references_by_parent` | Insert references for 2 parents, query by parent_id |
| `get_references_by_parent_and_type` | Filter by type using composite index |
| `count_references_by_type` | Count citations vs references for a parent |
| `update_match_status` | Insert as unmatched, update to matched, verify |
| `find_by_doi` | Insert with DOI, find by DOI |
| `get_by_match_status` | Insert mixed statuses, filter |
| `delete_by_parent` | Delete all references for a parent, verify gone |
| `cascade_delete_on_article` | Delete parent article, verify references auto-deleted |
| `sync_parent_flags` | Insert references, call sync, verify parent article flags updated |
| `sync_parent_flags_empty` | No references for parent, sync sets flags to false |
| `reference_with_minimal_data` | Insert with only title + parent_id, verify nullable fields work |
| `reference_with_full_data` | Insert with all fields populated |
| `match_status_constraint` | Verify invalid match_status rejected by CHECK |
| `type_constraint` | Verify invalid type (2) rejected by CHECK |

---

## 3. Phase 2: Import & UI (DEFERRED)

> This phase is documented here for planning purposes. Implementation will be tracked separately.

### 2A. N1 Parsing Wired Into Import

**Changes to `src-tauri/src/commands/import.rs`**:

In `ris_record_to_new_article()`, after setting `notes`:
```rust
let (num_cited, num_references) = record.notes
    .as_deref()
    .map(|n| parse_n1_citation_data(n))
    .unwrap_or((None, None));

NewArticle {
    // ... existing fields ...
    num_cited,
    num_references,
    has_full_text: false,
    full_text_file_name: None,
}
```

This applies to both RIS and BibTeX imports (BibTeX is converted to RisRecord first, so N1 parsing applies to both).

### 2B. RIS Reference Import Pipeline

#### Source Format

WoS exports cited references as a separate RIS file where each record represents a paper cited by (or citing) a target article. The parent article is identified by:
- DOI match against existing articles
- Accession number match
- Title + year fuzzy match

#### Import Flow

1. User uploads a "Cited References" or "Citing Articles" RIS file
2. Each record is parsed into a `RisRecord`
3. The system attempts to match each record's parent (the article it cites/is cited by) against the `articles` table
4. Matched records get `parent_id` set and `type` determined by file type
5. Unmatched references are stored with `parent_id` pointing to a special "orphan" marker or rejected
6. After insert, `sync_parent_reference_flags()` is called for each affected parent

#### New Tauri Commands

```rust
#[tauri::command]
pub async fn parse_references_ris_file(request: ParseRisRequest) -> Result<ReferenceImportPreview, AppError>

#[tauri::command]
pub async fn import_references_ris_file(
    app: AppHandle,
    request: ReferenceImportRequest,
) -> Result<ReferenceImportResult, AppError>
```

#### UI: Reference Import View

- Similar to existing RIS import stepper
- Additional step: "Match to Articles" — shows how many references matched existing articles
- Preview table showing: reference title, year, DOI, matched parent article title

### 2C. CSV Reference Import Pipeline

#### Source Format

WoS "Cited References" CSV export with columns:
```
Citing Article DOI, Cited Reference, Cited Author, Cited Year, Cited Work, Volume, Page, DOI
```

#### Implementation

- New CSV parser module: `src-tauri/src/csv/parser.rs`
- Maps CSV rows to `NewReference` structs
- Parent matching via `Citing Article DOI` → `articles.doi`
- `type` determined by file type: "Cited References" CSV → type=1, "Citing Articles" CSV → type=0

### 2D. Auto-Update Flags

After any reference insert/delete operation:

```rust
pub fn sync_parent_reference_flags(
    conn: &Connection,
    parent_id: &str,
) -> Result<(), AppError> {
    let has_citations: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM references WHERE parent_id = ?1 AND type = 0)",
        [parent_id],
        |row| row.get(0),
    )?;

    let has_references: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM references WHERE parent_id = ?1 AND type = 1)",
        [parent_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE articles SET has_citation_details = ?1, has_reference_details = ?2, changed_at = datetime('now') WHERE id = ?3",
        params![has_citations as i32, has_references as i32, parent_id],
    )?;

    Ok(())
}
```

### 2E. Full-Text File Association

#### New Tauri Commands

```rust
#[tauri::command]
pub async fn associate_full_text(
    app: AppHandle,
    article_id: String,
    file_path: String,
) -> Result<(), AppError>

#[tauri::command]
pub async fn remove_full_text(
    app: AppHandle,
    article_id: String,
) -> Result<(), AppError>

#[tauri::command]
pub async fn open_full_text(
    article_id: String,
) -> Result<(), AppError>
```

#### Storage

Full-text files are copied to the app's data directory:
```
~/.local/share/BonCode.Bango/fulltext/{article_id}/{filename}
```

The `full_text_file_name` column stores: `fulltext/{article_id}/{filename}`

### 2F. UI Changes

#### Article Detail Panel

Add a new "Citations & References" section showing:
- **Citation count badge**: `49 citations` (from `num_cited`)
- **Reference count badge**: `34 references` (from `num_references`)
- **Citation details indicator**: icon when `has_citation_details` is true
- **Reference details indicator**: icon when `has_reference_details` is true
- **Full-text indicator**: paper icon when `has_full_text` is true, clickable to open

#### Article Table

Optional new columns (hidden by default):
- "Cited by" column showing `num_cited`
- "References" column showing `num_references`
- "Full Text" icon column

#### Reference List Viewer

New panel/tab in article detail showing all reference/citation records for the selected article:
- Filterable by type (citations / references)
- Shows title, authors, year, DOI for each record
- Click to expand full metadata
- "Match Status" indicator for each record
- "Promote to Article" button for unmatched records

#### Import UI

- New import option: "Import Citations/References"
- Supports RIS and CSV formats
- Shows matching results before import

### 2G. Phase 2 Test Plan

#### Import Tests

| Test | Description |
|------|-------------|
| `n1_parsing_in_ris_import` | Full RIS import with N1 citation data, verify article has num_cited/num_references |
| `n1_parsing_in_bibtex_import` | BibTeX import with citation data |
| `n1_parsing_preserves_notes` | Verify notes field still contains full N1 text |
| `reference_ris_import` | Import RIS reference file, verify records created |
| `reference_csv_import` | Import CSV reference file, verify records created |
| `reference_parent_matching_by_doi` | References matched to correct parent via DOI |
| `reference_parent_no_match` | References with no matching parent handled gracefully |
| `reference_import_updates_flags` | After import, parent article flags are updated |

#### UI Tests (Vitest)

| Test | Description |
|------|-------------|
| `article_detail_shows_citation_counts` | Verify citation counts rendered in detail panel |
| `article_detail_shows_full_text_icon` | Full-text indicator shows when has_full_text=true |
| `reference_list_filters_by_type` | Toggle between citations and references |

#### Integration Tests

| Test | Description |
|------|-------------|
| `full_import_to_reference_workflow` | Import RIS → import references → verify counts and flags |
| `reference_promotion_workflow` | Create unmatched reference → promote to full article |
| `full_text_association_workflow` | Associate PDF → verify flags → remove → verify flags cleared |

---

## 4. Files Affected

### Phase 1 (Database)

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/db/migrations/v004_article_references.rs` | **CREATE** | Migration: new article columns + references table |
| `src-tauri/src/db/migrations/mod.rs` | **MODIFY** | Register v004 |
| `src-tauri/src/models/article.rs` | **MODIFY** | Add 6 new fields to `Article` and `NewArticle` |
| `src-tauri/src/models/reference.rs` | **CREATE** | New `Reference`, `NewReference`, `ReferenceType`, `MatchStatus` structs |
| `src-tauri/src/models/mod.rs` | **MODIFY** | Add `pub mod reference` |
| `src-tauri/src/ris/types.rs` | **MODIFY** | Add `num_cited`, `num_references` to `RisRecord` |
| `src-tauri/src/ris/n1_parser.rs` | **CREATE** | N1 citation data parsing + tests |
| `src-tauri/src/ris/mod.rs` | **MODIFY** | Add `pub mod n1_parser` |
| `src-tauri/src/db/article_repo.rs` | **MODIFY** | Update INSERT/SELECT/row_to_article for 6 new columns |
| `src-tauri/src/db/reference_repo.rs` | **CREATE** | Full CRUD for references table |
| `src-tauri/src/db/mod.rs` | **MODIFY** | Add `pub mod reference_repo` |
| `src/types/index.ts` | **MODIFY** | Add new fields to `Article`, add `Reference` interface |
| `src-tauri/tests/reference_repo_test.rs` | **CREATE** | Reference repo integration tests |
| `src-tauri/tests/article_query_test.rs` | **MODIFY** | Update for new columns |

### Phase 2 (Import & UI) — DEFERRED

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/commands/import.rs` | **MODIFY** | Wire N1 parsing, add reference import commands |
| `src-tauri/src/csv/parser.rs` | **CREATE** | CSV reference parser |
| `src-tauri/src/csv/mod.rs` | **CREATE** | CSV module |
| `src/composables/use-import.ts` | **MODIFY** | Add reference import flow |
| `src/components/article-detail-panel.vue` | **MODIFY** | Show citation counts, reference list, full-text |
| `src/components/article-table.vue` | **MODIFY** | Optional citation/reference columns |
| `src/components/reference-list.vue` | **CREATE** | Reference list viewer component |
| `src/views/import-ris.vue` | **MODIFY** | Add reference import option |

---

## 5. Implementation Order

### Phase 1 (Current)

```
Step 1: Create migration v004
        ├── New columns on articles table
        ├── New references table
        └── Indexes (including composite parent_id + type)

Step 2: Update Rust models
        ├── Article + NewArticle (6 new fields)
        ├── Reference + NewReference (new structs)
        └── RisRecord (2 new fields)

Step 3: Create N1 parser module
        ├── parse_n1_citation_data() function
        └── Unit tests (12+ test cases)

Step 4: Update article_repo.rs
        ├── INSERT statements (6 new params)
        ├── row_to_article() (6 new fields)
        └── get_next_unscreened_working_batch() (defaults for new fields)

Step 5: Create reference_repo.rs
        ├── insert_reference / insert_references_batch
        ├── get_references_by_parent_and_type
        ├── count_references_by_parent_and_type
        ├── update_match_status
        ├── sync_parent_reference_flags
        └── Integration tests (15+ test cases)

Step 6: Update TypeScript types
        ├── Article interface (6 new fields)
        └── Reference interface + types

Step 7: Run full test suite
        ├── cargo test
        ├── cargo clippy
        └── npm run check:all
```

### Phase 2 (Deferred)

```
Step 1: Wire N1 parsing into import commands
Step 2: Create CSV parser module
Step 3: Add reference import Tauri commands
Step 4: Add full-text file association commands
Step 5: Update import composable + views
Step 6: Update article detail panel
Step 7: Create reference list viewer component
Step 8: End-to-end testing