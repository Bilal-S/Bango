# Bango - v3 Specification

Third revision of the Bango specification. Incorporates scope reductions, gap-fills, and detail expansions from the v2 (Second Specification). Supersedes all prior specifications.

---

## 1. Product Overview

Bango is a **desktop application** for AI-assisted systematic literature review. Researchers import RIS bibliography files, define inclusion/exclusion criteria, and use LLMs to screen article abstracts - producing a rigorously categorized set of articles with reasoning, tags, and labels.

Built with **Tauri 2.x** for a lightweight, offline-capable experience. All data is stored locally in SQLite. No cloud upload is required.

**v1 is desktop-only.** Mobile support is deferred to a future release.

The app manages a **single project** - there is no project selector or multi-project workspace. The database and all state belong to one active review.

---

## 2. Terminology

| Term | Definition | Source |
|------|-----------|--------|
| **Tag** | A content-category label describing an article's topic, methodology, or relevance. E.g., "machine-learning", "clinical-trial". | AI generates from RIS `KW` field and user criteria. User can add, edit, delete. |
| **Label** | A workflow marker for organizational/process tracking. E.g., "priority-read", "disputed", "needs-full-text". | AI generates an initial set from inclusion and exclusion criteria. User can expand and modify. |
| **Criterion** | A discrete inclusion or exclusion rule with an assigned priority level. | User-defined. |
| **Research Aim** | A discrete statement of research objective. | User-defined. |

---

## 3. Domain Model

```json
{
  "ResearchAim": {
    "id": "uuid",
    "text": "string",
    "createdAt": "ISO-8601 timestamp"
  },
  "Criterion": {
    "id": "uuid",
    "type": "enum[inclusion, exclusion]",
    "text": "string",
    "priority": "enum[critical, high, standard, low, optional]",
    "createdAt": "ISO-8601 timestamp"
  },
  "Tag": {
    "id": "uuid",
    "name": "string",
    "source": "enum[ai_suggested, user_created, ris_keyword]",
    "color": "string | null"
  },
  "Label": {
    "id": "uuid",
    "name": "string",
    "source": "enum[ai_generated, user_created]",
    "color": "string | null"
  },
  "Article": {
    "id": "uuid",
    "sequence_id": "integer (auto-incrementing)",
    "status": "enum[duplicate, working, included, rejected]",
    "screeningError": "boolean (default: false)",
    "title": "string (required)",
    "abstractText": "string (required)",
    "authors": ["string"] (required, min 1)",
    "publicationYear": "integer | null",
    "doi": "string | null",
    "journal": "string | null",
    "volume": "string | null",
    "issue": "string | null",
    "startPage": "string | null",
    "endPage": "string | null",
    "keywords": ["string"],
    "url": "string | null",
    "language": "string | null",
    "publisher": "string | null",
    "publisherCity": "string | null",
    "publisherAddress": "string | null",
    "issn": "string | null",
    "referenceType": "string | null",
    "date": "string | null",
    "authorAddress": "string | null",
    "accessionNumber": "string | null",
    "customField3": "string | null",
    "journalAbbreviation": "string | null",
    "journalIsoAbbreviation": "string | null",
    "notes": "string | null",
    "webOfScienceDb": "string | null",
    "userNotes": "string | null",
    "risExtras": "object (all unrecognized RIS tags preserved here)",
    "duplicateOf": "uuid | null (references surviving article id)",
    "aiDecision": "enum[include, exclude] | null",
    "aiReasoning": "string | null",
    "aiConfidence": "float (0.0–1.0) | null",
    "matchedInclusionCriteria": ["uuid"],
    "matchedExclusionCriteria": ["uuid"],
    "tags": ["uuid"],
    "labels": ["uuid"],
    "manualOverride": "boolean (default: false)",
    "importSource": "string (filename of originating RIS file)",
    "importedAt": "ISO-8601 timestamp",
    "screenedAt": "ISO-8601 timestamp | null",
    "dataLength": "integer | null (total character count for estimation)",
    "tokenEstimate": "integer | null (heuristic tokens)",
    "actualTokens": "integer | null (actual tokens consumed)"
  },
  "LLMConfig": {
    "provider": "enum[openai, anthropic, google, mistral_ai, z_ai, llama_cpp, ollama, lm_studio, custom]",
    "endpointUrl": "string (full URL provided by user, app does not append paths)",
    "apiKey": "string (encrypted)",
    "modelName": "string",
    "temperature": "float (default: 0.2)",
    "maxConcurrentRequests": "integer (default: 3)",
    "requestDelayMs": "integer (default: 500)",
    "contextWindowTokens": "integer (default: 50000)"
  },
  "AuditEntry": {
    "id": "uuid",
    "articleId": "uuid",
    "timestamp": "ISO-8601 timestamp",
    "action": "enum[import, dedup_merge, dedup_flag, status_change, tag_add, tag_remove, label_add, label_remove, criteria_match, ai_screen, manual_override, ai_summary]",
    "fromStatus": "enum | null",
    "toStatus": "enum | null",
    "details": "string",
    "source": "enum[ai, user, system]"
  },
  "ExportMetadata": {
    "specVersion": "string (e.g., '3.0')",
    "exportedAt": "ISO-8601 timestamp",
    "appName": "string ('Bango')",
    "appVersion": "string"
  }
}
```

**Changes from v2:**
- Added `sequence_id`, `dataLength`, `tokenEstimate`, and `actualTokens` to Article for performance and progress tracking.
- Added `color` to Tag and Label models.
- Expanded `LLMProvider` enum to include `anthropic` and `mistral_ai`.
- Added `ExportMetadata` schema for forward-compatible exports.

---

## 4. RIS Import Specification

### 4.1 Supported RIS Tags

| RIS Tag | Field | Maps To | Required |
|---------|-------|---------|----------|
| `TY` | Reference Type | `referenceType` | Yes |
| `TI` | Title | `title` | Yes |
| `AB` | Abstract | `abstract` | Yes |
| `AU` | Author | `authors[]` | Yes (min 1) |
| `PY` | Publication Year | `publicationYear` | No |
| `DO` | DOI | `doi` | No |
| `T2` | Journal / Publication | `journal` | No |
| `VL` | Volume | `volume` | No |
| `IS` | Issue | `issue` | No |
| `SP` | Start Page | `startPage` | No |
| `EP` | End Page | `endPage` | No |
| `KW` | Keywords | `keywords[]` | No |
| `UR` | URL | `url` | No |
| `LA` | Language | `language` | No |
| `PB` | Publisher | `publisher` | No |
| `SN` | ISSN / ISBN | `issn` | No |
| `M3` | Type of Work | `referenceType` (fallback) | No |
| `N2` | Abstract (alternate) | `abstract` (fallback if AB missing) | No |
| `JO` | Journal (alternate) | `journal` (fallback) | No |
| `DA` | Date | `date` | No |
| `AD` | Author Address | `authorAddress` | No |
| `AN` | Accession Number | `accessionNumber` | No |
| `C3` | Custom Field 3 | `customField3` | No |
| `ER` | End of Reference | *(parser delimiter - not stored as article data)* | No |
| `J9` | Journal Abbreviation (29-char) | `journalAbbreviation` | No |
| `JI` | Journal ISO Abbreviation | `journalIsoAbbreviation` | No |
| `N1` | Notes | `notes` | No |
| `PA` | Publisher Address | `publisherAddress` | No |
| `PI` | Publisher City | `publisherCity` | No |
| `PU` | Publisher | `publisher` (fallback for PB) | No |
| `WE` | Web of Science Database | `webOfScienceDb` | No |

All unrecognized RIS tags are preserved as key-value pairs in `risExtras`.

### 4.2 Import Validation

- Articles missing **Title**, **Abstract**, or **Authors** are flagged as validation errors with a specific message indicating which required field is missing (e.g., "Missing required field: Title (TI)").
- If `AB` is missing but `N2` is present, `N2` is used as the abstract.
- Multiple `AU` tags for a single article are collected into the `authors[]` array.
- Multiple `KW` tags for a single article are collected into the `keywords[]` array.
- Each import records the source filename in `importSource`.
- **Partial imports are supported**: valid articles are imported even when some records fail validation. Invalid records are skipped.
- Validation errors are **grouped by error message** and displayed as collapsible summaries in the UI (e.g., "7 records - Missing required field: Abstract (AB or N2)"). Users can expand each group to see the affected record indices.
- A **warning banner** is shown when there are validation issues, indicating how many records will be skipped and how many will be imported.
- Users can also **manually exclude** individual valid articles from the preview table before confirming import.

### 4.3 Import Limits

- If a single RIS file contains more valid records than the remaining project capacity (10,000 total article limit minus current article count), the import is **rejected** with an error stating how many articles were in the file and how many slots remain.
- **Partial imports**: Only valid articles count toward the import limit. Invalid records are excluded before the capacity check.
- The import result reports: number imported, number skipped (validation), number excluded (user), and remaining capacity.

### 4.4 Multiple Imports

- Users can import **multiple RIS files** into the project.
- On import, deduplication runs against **all existing articles** (regardless of status).
- **Non-duplicate articles** are automatically promoted to `working` status.
- **Duplicate articles** are placed in `duplicate` status with `duplicateOf` set to the surviving article.
- If a newly imported article is a duplicate of an article already in `working`, `included`, or `rejected`, the **existing article's status is not changed**. The newly imported article is placed in `duplicate` status with `duplicateOf` referencing the accepted article. The UI displays a reference to the accepted article in the duplicate's detail view.
- Previously resolved duplicates are not re-evaluated. Only new articles are compared against the full corpus.

---

## 5. Deduplication Specification

### 5.1 Algorithm

Multi-strategy matching using Levenshtein distance normalized to a 0–100% similarity score:

| Strategy | Match Fields | Threshold | Result |
|----------|-------------|-----------|--------|
| DOI exact | `doi` | Exact match | Exact duplicate (auto-merge) |
| Title + Year | `title` + `publicationYear` | Title similarity >= 95% AND year matches | Exact duplicate (auto-merge) |
| Fuzzy title + Year | `title` + `publicationYear` | Title similarity 70–94% AND year matches | Fuzzy match (flag for manual review) |
| Author + Title partial | First author last name + `title` | Author exact match AND title similarity >= 80% | Fuzzy match (flag for manual review) |

Strategies are evaluated in order. First match wins. Articles without DOI skip strategy 1. Articles with a null `publicationYear` skip strategies 2 and 3 (year match required). These articles fall through to strategy 4 or, if no match is found, proceed directly to the Working list.

### 5.2 Title Normalization

Before comparison, titles are:
1. Converted to lowercase
2. Stripped of all punctuation (`.,;:!?'"-()[]{}`)
3. Collapsed whitespace to single spaces
4. Trimmed

**Short-title guard:** Titles that are fewer than 10 characters after normalization are excluded from title-based matching strategies (strategies 2–4). These articles proceed directly to the Working list without dedup comparison via title.

### 5.3 Merge Behavior

- **Exact duplicates**: The article with the most complete metadata (highest non-null field count) is retained as the surviving article and placed in `working` status. The other is marked with `duplicateOf: <surviving_id>` and placed in `duplicate` status.
- **Fuzzy matches**: Both articles are placed in `duplicate` status. The user is presented a side-by-side comparison view and chooses: keep left, keep right, or keep both (not duplicates). The rejected article remains in `duplicate` with `duplicateOf` set. The surviving article moves to `working`.
- **Cross-status dedup protection**: If a newly imported article duplicates an article already in `working`, `included`, or `rejected`, the existing article's status is **never changed**. The newly imported article is placed in `duplicate` with `duplicateOf` referencing the accepted article.
- When a duplicate is resolved, only the surviving representative article moves to `working`.

---

## 6. Criteria and Priority System

### 6.1 Research Aims

- Research aims are a **list of discrete text entries** (not a single free-text block).
- Each aim is entered individually by the user.
- No priority level is assigned to aims - they serve as context for the AI screening.

### 6.2 Inclusion and Exclusion Criteria

- Both inclusion and exclusion criteria are discrete text entries.
- **Both types** are assigned a priority level: `critical`, `high`, `standard`, `low`, or `optional`.
- Priority is assigned by the user at creation time and can be changed later.
- The AI does not adjust priorities.

### 6.3 Priority Conflict Resolution

When the AI screens an article and it matches multiple criteria:

1. Find the **single highest-priority** inclusion criterion that matches. Note its level.
2. Find the **single highest-priority** exclusion criterion that matches. Note its level.
3. Compare the two levels using the hierarchy: `critical > high > standard > low > optional`.
4. **The higher-priority side wins.**
5. **If tied → include** (ties favor inclusion).
6. **If no criteria match at all → exclude** (no basis for inclusion).

This is deterministic logic applied by the app after the AI reports which criteria matched. The AI does not resolve priority conflicts - it only identifies matches.

---

## 7. Article State Machine

### 7.1 States

| State | Description |
|-------|-------------|
| **Duplicate** | Article flagged as a duplicate during import. Read-only until resolved. Not a working list — only duplicates reside here. |
| **Working** | Deduplicated article awaiting or pending screening. Non-duplicate articles are promoted here directly on import. |
| **Included** | Article meeting inclusion criteria. |
| **Rejected** | Article excluded based on criteria. |

Articles in any state may have a `screeningError` flag set to `true`, which is displayed in the UI. The flag does not change the article's status - errored articles remain in their current state (typically Working).

### 7.2 State Transition Diagram

```
  Import ──(non-dup)──► Working
     │                      ▲  ▲
     │                      │  │
     ▼                  Manual moves:
  Duplicate ──(resolve)──► Working
     │
     │               Working ↔ Included ↔ Rejected
     │
     Duplicate: READ-ONLY until resolved
```

**Flow:**
1. On import, non-duplicate articles go directly to `working`.
2. Duplicate articles go to `duplicate` (read-only until resolved).
3. When a duplicate is resolved, the surviving article moves to `working`.
4. From `working`, articles move to `included` or `rejected` via AI screening or manual action.
5. Manual moves are freely allowed between `working`, `included`, and `rejected`.

### 7.3 Allowed Transitions

| From | To | Trigger | Notes |
|------|----|---------|-------|
| *(new import)* | Working | Import (non-duplicate) | Non-duplicate articles are promoted directly to Working on import. |
| *(new import)* | Duplicate | Import (duplicate detected) | Duplicate articles placed in Duplicate with `duplicateOf` set. Existing accepted articles (in Working, Included, or Rejected) are never affected. |
| Duplicate | Working | Duplicate resolution | User resolves duplicate pair; surviving article moves to Working. |
| Working | Included | AI screening or manual | AI sets `aiDecision`, `aiReasoning`, `aiConfidence`, matched criteria. |
| Working | Rejected | AI screening or manual | Same as above. |
| Included | Working | Manual override | Sets `manualOverride: true`. |
| Included | Rejected | Manual override | Sets `manualOverride: true`. |
| Rejected | Working | Manual override | Sets `manualOverride: true`. |
| Rejected | Included | Manual override | Sets `manualOverride: true`. |

All state changes create an `AuditEntry` record.

### 7.4 Screening Error Handling

When the LLM returns a malformed/invalid response for an article:
- Article stays in the **Working** list with `screeningError: true`.
- The raw LLM response is logged in the audit trail.
- The article displays a visible error indicator in the UI.
- User can retry the article individually or re-run screening for all errored articles.
- Retrying clears the `screeningError` flag before resubmitting.
- **Explicit Error Decisions**: If the LLM returns `decision: "error"`, the app treats this as a screening error and logs the reasoning provided by the AI.

---

## 8. Tag and Label Generation

### 8.1 Timing

Tag and label generation is an **optional, user-triggered** step. It is not required before screening.

**Workflow sequence:**
1. Import RIS → non-duplicate articles go directly to Working list; duplicate articles go to Duplicates list
2. User resolves duplicates in Duplicates list → surviving articles move to Working list
3. User defines research aims, inclusion criteria, exclusion criteria (with priorities)
4. **[Optional]** User clicks "Suggest Tags" and/or "Suggest Labels" → AI generates initial Tags and Labels (two separate buttons, two separate calls)
5. User reviews and edits tags and labels
6. User triggers AI screening → articles move to Included or Rejected (screening also suggests per-article tags)
7. User can manually move articles, adjust tags/labels at any time
8. User triggers AI summary of included articles
9. User exports RIS / PRISMA / project backup

### 8.2 Tag Generation (Optional Pre-Screening Pass)

- **Trigger**: User clicks "Suggest Tags" button in Tag Management view.
- **Input**: Single AI call containing the aggregated `keywords` from all Working-list articles, plus all user-defined inclusion and exclusion criteria.
- **Output**: A set of suggested Tags representing content categories relevant to the review.
- Each Tag records its `source` as `ai_suggested`, `user_created`, or `ris_keyword`.
- User can add, rename, merge, or delete any Tag.

**Prompt template:**

**System prompt:**
> You are a systematic literature review assistant. Generate a set of content-category tags for organizing articles in a literature review.

**User prompt:**
```
## Task
Generate a concise set of content-category tags for organizing articles in a systematic literature review. Tags should represent meaningful topic, methodology, or relevance categories.

## Article Keywords
{aggregated unique keywords from all Working-list articles, de-duplicated, comma-separated}

## Inclusion Criteria
{numbered list of inclusion criteria}

## Exclusion Criteria
{numbered list of exclusion criteria}

## Response Format
Return JSON exactly matching this schema:
{
  "tags": ["tag-name-1", "tag-name-2", ...]
}

Rules:
- Generate 10–30 tags.
- Each tag should be a short, lowercase, hyphenated string (e.g., "machine-learning", "clinical-trial").
- Tags should be derived from the keywords and criteria provided.
- Do not duplicate or overlap concepts.
```

### 8.3 Label Generation (Optional Pre-Screening Pass)

- **Trigger**: User clicks "Suggest Labels" button in Label Management view.
- **Input**: Single AI call containing all user-defined inclusion and exclusion criteria.
- **Output**: A set of suggested Labels representing workflow/process categories derived from the screening criteria.
- Each Label records its `source` as `ai_generated` or `user_created`.
- User can add, rename, merge, or delete any Label.

**Prompt template:**

**System prompt:**
> You are a systematic literature review assistant. Generate a set of workflow labels for tracking the screening process.

**User prompt:**
```
## Task
Generate a set of workflow labels for tracking articles through the screening process. Labels should represent organizational or process categories (e.g., "priority-read", "disputed", "needs-full-text", "strong-methodology").

## Inclusion Criteria
{numbered list of inclusion criteria}

## Exclusion Criteria
{numbered list of exclusion criteria}

## Response Format
Return JSON exactly matching this schema:
{
  "labels": ["label-name-1", "label-name-2", ...]
}

Rules:
- Generate 5–15 labels.
- Each label should be a short, descriptive string.
- Labels should help categorize articles by their screening status or quality indicators.
- Do not duplicate or overlap concepts.
```

---

## 9. AI Screening Process

### 9.1 Batch Screening Process

Articles are sent to the LLM in **batches** (configurable size, default 1-5 articles) to optimize throughput. The prompt includes:

**System prompt:**
> Act as a systematic literature review screening assistant. Critically evaluate article abstracts against research aims and criteria. Cite specific sentences from the text to justify your decision. Follow priority rules when criteria overlap or conflict. Return only a JSON array matching the required schema, one object per article, in the same order as submitted.

**User prompt:**
```
## Research Aims
{numbered list of aim entries}

## Inclusion Criteria (in order of priority)
{numbered list with [id] and text}

## Exclusion Criteria (in order of priority)
{numbered list with [id] and text}

## Priority Rules
- Higher priority rules always outweigh lower priority rules.
- If inclusion and exclusion criteria of equal priority both match, favor inclusion.

## Articles
[
  {"title": "{title}", "authors": "{authors}", "year": {year}, "abstract": "{abstract_text}"},
  ...
]
```

**Response schema:**
```json
[
  {
    "decision": "include" | "exclude" | "error",
    "reasoning": "A paragraph citing specific sentences from the abstract to justify the decision.",
    "matched_inclusion_criteria": ["criteria-id-1", ...],
    "matched_exclusion_criteria": ["criteria-id-3", ...],
    "suggested_tags": ["tag-name-1", ...],
    "confidence": 0.0-1.0
  }
]
```

### 9.2 Response Processing

1. App parses the JSON response.
2. App applies **deterministic priority conflict resolution** (Section 6.3) based on matched criteria and their priorities. The AI's `decision` field is advisory - the app computes the final decision.
3. If the app's computed decision differs from the AI's `decision`, the app's decision takes precedence and a note is appended to `aiReasoning` (e.g., "[App override: inclusion favored due to equal-priority tie]").
4. The article moves to Included or Rejected.
5. AI-suggested tags are matched to existing tags (case-insensitive name match) or created as new `ai_suggested` tags.

### 9.3 Batch Processing

- **Concurrency**: Configurable, default 3 concurrent requests.
- **Batch Size**: Configurable, default 1-5 articles per request.
- **Delay**: Configurable, default 500ms between requests (applied between starting each new request, not between batches).
- **On HTTP 429 (rate limit)**: Exponential backoff with max 3 retries (delay: 1s, 2s, 4s).
- **On malformed JSON**: Article stays in Working with `screeningError: true`. Raw response logged in audit trail.
- **On connection failure**: Screening job pauses. Completed articles are saved. User can resume or cancel.

### 9.4 Progress Tracking

- Progress bar: `X / Y articles screened (Z%)`
- Estimated time remaining based on rolling average per-article processing time.
- **Pause** button: stops after the current article completes.
- **Cancel** button: stops immediately. Partially screened articles remain in Working.

### 9.5 Resume Screening and Readiness

When resuming or starting a screening job, the app performs a **Readiness Check**:
1. Checks for at least one research aim, one inclusion criterion, and one exclusion criterion.
2. Verifies LLM configuration is complete.
3. Counts unscreened Working articles (`screenedAt IS NULL`).
4. Performs token estimation to warn if any article might exceed context window limits.

The UI displays "Resuming: X articles remaining" or "Ready: X articles to screen" before starting. Continuing a job skips any articles that already have a non-null `screenedAt`.

### 9.6 Token Estimation

Before starting screening, the app estimates total token usage:
1. **Estimation method**: Count characters in the combined system prompt + user prompt template (excluding article-specific fields), divide by 4 to get approximate tokens for the template overhead.
2. For each article, estimate token count for article-specific fields (title, abstract, authors) using the same characters/4 ratio.
3. Find the article with the largest estimated token count. Sum template tokens + that article's tokens to get a worst-case per-article estimate.
4. Compare per-article estimate against `contextWindowTokens`.
5. **Warn if estimated per-article tokens exceed 80%** of the configured context window.
6. No hard block - user can override and proceed.

---

## 10. LLM Configuration

### 10.1 Supported Providers

| Provider | Config | Default Endpoint (suggestion only) |
|----------|--------|-----------------------------------|
| OpenAI | API key required | `https://api.openai.com/v1/chat/completions` |
| Anthropic | API key required | `https://api.anthropic.com/v1/messages` |
| Google | API key required | Gemini API endpoint |
| Mistral AI | API key required | `https://api.mistral.ai/v1/chat/completions` |
| z.ai | API key + full URL | User provides full URL (e.g., `https://api.z.ai/api/coding/paas/v4`) |
| Ollama | No auth | `http://localhost:11434/v1/chat/completions` |
| llama.cpp | No auth | `http://localhost:8080/v1/chat/completions` |
| LM Studio | No auth | `http://localhost:1234/v1/chat/completions` |
| Custom | API key (optional) + full URL | User provides |

All providers use the **OpenAI-compatible chat completions format**. Google Gemini requires a provider-specific adapter. The user provides the **full endpoint URL** - the app does not append `/v1/chat/completions` or any other path.

### 10.2 Connection Testing

- "Test Connection" button sends a minimal chat completion request to the configured endpoint.
- Success: saves configuration and displays success message.
- Failure: displays specific error (connection refused, auth failed, timeout, etc.).

### 10.3 Hardware Warning

- When a local provider (llama.cpp, Ollama, LM Studio) is selected, display a **static warning note**: "Local LLM providers typically require 16 GB or more of VRAM for models supporting 50K+ token context windows. Performance may be limited on systems with less VRAM."
- No GPU detection is performed. The warning is informational only and does not block configuration.

### 10.4 Context Window

- The app requires an LLM with a context window of **50,000 tokens or larger**.
- Token estimation is performed before starting screening (Section 9.6).
- If the estimated count approaches the configured context window, the user is warned.
- No hard block - user can override.

---

## 11. AI Summary

- Triggered manually by the user (not automatic).
- Processes all articles in the Included list.
- **Structured output** with the following sections:
  1. **Key Themes**: Main topics and findings across included studies.
  2. **Research Trends**: Patterns and directions in the literature vis-a-vis the research aims.
  3. **Methodological Strengths**: Common robust methodologies observed.
  4. **Common Weaknesses**: Limitations frequently cited across studies.
  5. **Gaps in Literature**: Under-explored areas relative to the research aims.
- Default target length: ~1000 words (configurable).
- Can be regenerated at any time.
- The summary references the research aims to maintain focus.

**Prompt template:**

**System prompt:**
> You are a systematic literature review assistant. Generate a structured summary of the included articles in a systematic review.

**User prompt:**
```
## Task
Generate a structured summary of the included articles in a systematic literature review. Focus on the research aims provided.

## Research Aims
{numbered list of aim entries}

## Target Length
Approximately {configurableLength} words.

## Included Articles
{for each included article:}
---
Title: {title}
Authors: {authors}
Year: {publicationYear}
Abstract: {abstract}
AI Reasoning: {aiReasoning}
---
{end for}

## Response Format
Return JSON exactly matching this schema:
{
  "key_themes": "A paragraph describing the main topics and findings across included studies.",
  "research_trends": "A paragraph describing patterns and directions in the literature vis-a-vis the research aims.",
  "methodological_strengths": "A paragraph describing common robust methodologies observed.",
  "common_weaknesses": "A paragraph describing limitations frequently cited across studies.",
  "gaps_in_literature": "A paragraph describing under-explored areas relative to the research aims."
}
```

**Batch handling:** If the combined text of all included articles exceeds 80% of the configured context window, the app splits articles into batches, generates a summary for each batch, then sends a synthesis prompt combining the batch summaries into a final summary.

---

## 12. PRISMA 2020 Flow Diagram

### 12.1 Diagram Specification

- **Standard PRISMA 2020 four-phase flow**:
  1. **Identification**: Records identified through database searching → Records after duplicates removed.
  2. **Screening**: Records screened → Records excluded (with count).
  3. **Eligibility**: Records assessed for eligibility → Articles excluded (with count). Since Bango screens abstracts only (no separate full-text review), Eligibility and Screening show the same counts.
  4. **Included**: Final number of studies included in the review.

- **Data mapping:**

| PRISMA Box | Data Source |
|------------|-------------|
| Records identified | Total count of all articles ever imported (all statuses combined) |
| Duplicates removed | Count of articles in `duplicate` status (where `duplicateOf IS NOT NULL`) |
| Records screened | Count of articles in Working, Included, or Rejected status (total articles minus duplicates) |
| Records excluded (Screening) | Count of Rejected articles |
| Studies included | Count of Included articles |

- **Exclusion reason breakdown**: User-controlled toggle. When enabled, shows the number of articles excluded per matched exclusion criterion. Data derived from `matchedExclusionCriteria` fields on Rejected articles.

### 12.2 Rendering

- Rendered as **SVG** in the UI.
- **Export formats**: SVG, PNG.
- Diagram styling follows the standard PRISMA 2020 layout conventions.

---

## 13. Search, Sort, and Filter

### 13.1 Search

- Full-text search across `title` and `abstract` fields.
- Case-insensitive.
- Searches within the currently viewed list.

### 13.2 Sort

Available sort options for all lists:
- Title (A–Z, Z–A)
- Publication Year (newest first, oldest first)
- Date Added (newest first, oldest first)
- AI Confidence Score (highest first, lowest first)

### 13.3 Filter

Available filter options:
- Status (multi-select)
- Tags (multi-select)
- Labels (multi-select)
- Matched criteria (multi-select)
- Publication year range (from–to)
- Manual override (yes/no)
- Screening error (yes/no)

---

## 14. Audit Trail

- Every state change, tag/label change, criteria match, and AI screening decision creates an `AuditEntry`.
- Audit entries are visible per-article in a detail panel.
- User can view the full history of any article.
- **No revert functionality.** Users correct state by manually moving articles or editing tags/labels.
- No global undo stack.

---

## 15. Export and Import

### 15.1 RIS Export

- Exports the Included list in valid RIS format.
- Includes all original metadata fields.
- Appends AI-generated tags as `KW` entries.
- Appends AI reasoning as an `N1` (Notes) field.
- User-entered notes on an article are exported using the `NO` RIS tag.
- Inclusion and exclusion labels are exported using the `C1` RIS tag. Labels are grouped into `inc` and `exc` arrays based on the article's `aiDecision`: labels on included articles go into `inc`, labels on rejected articles go into `exc`. Example:
  ```
  C1 - {"inc":["journal","blue"],"exc":["morbid","large"]}
  ```
- Compatible with Zotero, Mendeley, and EndNote.

### 15.2 Project Export

- Single `.bango.json` file.
- Top-level structure includes `ExportMetadata` (specVersion, exportedAt, appName, appVersion) for forward compatibility.
- Contains: research aims, criteria, all articles (all states), tags, labels, audit log, LLM config (provider name + endpoint URL + model name).
- **API keys are included**, encrypted with a **user-provided password** using AES-256-GCM.
- The recipient must enter the password to import the project and access the LLM configuration.

### 15.3 Project Import

- Accepts a `.bango.json` file.
- Checks `specVersion` to determine if the file format is supported. If the version is newer than the app supports, display a warning that some data may not be imported correctly.
- User enters the password to decrypt API keys.
- If password is incorrect, project imports without API keys (user must re-enter).
- Imported data **replaces** the current project state (no merge).

---

## 16. Non-Functional Requirements

### 16.1 Performance

| Metric | Target |
|--------|--------|
| Max RIS file size | 50 MB |
| Max articles per project | 10,000 |
| Max RIS file imports per project | Unlimited (within article limit) |
| UI list operations (filter, sort, search) | < 200ms |
| App cold start | < 3 seconds |
| SQLite warning threshold | 80% of practical limits |

### 16.2 Hardware

| Requirement | Details |
|-------------|---------|
| Context window | LLM must support >= 50,000 tokens |
| Local AI VRAM | Static warning displayed for local providers (informational, non-blocking) |

### 16.3 Security

| Mechanism | Details |
|-----------|---------|
| API key storage (local) | AES-256-GCM encryption. Key derived via PBKDF2 from machine hostname + username + app salt. |
| API key storage (export) | Re-encrypted with user-provided password using AES-256-GCM. |
| SQLite database | Not encrypted (local app, OS-level security). |
| App access control | None (Tauri desktop app). |

### 16.4 Offline Capability

- All data is stored locally in SQLite - fully browsable offline.
- AI screening requires an active LLM connection (hosted or local).
- If connection is lost mid-screening, completed articles are saved; pending articles remain in Working.
- User can browse, search, sort, filter, and manually move articles while offline.

---

## 17. Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Framework | Tauri 2.x | Lightweight cross-platform desktop runtime |
| Frontend | Vue 3.x + TypeScript | Strict type-checking for complex UI state |
| Backend | Rust | Memory-safe, non-blocking background processing |
| Database | SQLite (local) | Portable, offline-first |
| AI Integration | REST API client in Rust | Async requests to external or local LLM endpoints |
| Encryption | AES-256-GCM + PBKDF2 | Industry-standard symmetric encryption |

---

## 18. UI Design System

### 18.1 Design Tool

UI designs are created in [Google Stitch](https://stitch.withgoogle.com/) and exported as code via the Stitch MCP server. The design system is captured in `DESIGN.md` at the project root, which serves as the single source of truth for design tokens (colors, typography, spacing, component specs).

### 18.2 Design Tokens

The design system is named **"Scholarly Precision"** - a Minimalist-Corporate aesthetic inspired by "Notion meets Zotero." Full token definitions live in `DESIGN.md`.

**Colors:**
- Primary Indigo: `#4F46E5` - primary actions, active states, Standard priority
- Surface: `#FCF8FF` - main workspace background
- Sidebar Slate: `#1E293B` - navigation panel
- Text Primary: `#1B1B24`
- Text Secondary: `#464555`
- Outline: `#777587`
- Error: `#BA1A1A`

**Priority Colors:**
- Critical: Bright Red (`#EF4444`)
- High: Orange (`#F97316`)
- Standard: Indigo (`#3B82F6`)
- Low: Medium Gray (`#6B7280`)
- Optional: Subtle Gray with dashed borders (`#9CA3AF`)

**Typography:**
- Font family: Inter, system-ui, sans-serif
- Display: 24px/600, -0.02em tracking
- H1: 20px/600, -0.01em tracking
- H2: 16px/600
- Body: 14px/400
- Caption: 13px/400
- Label-caps: 11px/600, 0.05em tracking
- Mono: ui-monospace, SFMono-Regular, 13px/400

**Spacing:**
- Base unit: 4px
- Container padding: 24px
- Sidebar width: 260px
- Gutter: 16px
- Stack gap: 12px

**Border Radius:**
- Primary (containers, buttons, inputs): 8px (0.5rem)
- Small: 4px (0.25rem)
- Medium: 12px (0.75rem)
- Pill badges/chips: 9999px (full round)

### 18.3 Layout Philosophy

- **Fixed-Fluid Hybrid**: Fixed-width sidebar (260px dark slate) + fluid main content area
- **Master-Detail View**: 3-pane layout (Navigation > List > Content)
- **8pt grid system** with 4px increments for dense data components
- Subtle shadows only (`0 4px 12px rgba(0, 0, 0, 0.05)`)
- Pane separation via 1px borders (`#E5E7EB`) rather than shadows

### 18.3.1 Implementation (Tailwind CSS v4)

The design system is implemented via **Tailwind CSS v4** with the `@tailwindcss/vite` plugin. Tailwind's preflight reset is **disabled** (`preflight(false)`) to preserve compatibility with existing custom-CSS views.

**Design token mapping:**

Custom CSS variables in `src/styles/tokens.css` define `--color-*`, `--font-*`, `--space-*`, `--radius-*` tokens consumed by views using scoped CSS. The Tailwind `@theme` block in `src/styles/base.css` maps the same values to Tailwind utilities (`bg-surface-container-lowest`, `text-on-surface`, `font-display`, `text-display`, etc.) for views using Tailwind classes.

**Dual styling approach:**
- **Custom CSS views** (dashboard, import-ris, dedup-review, screening-progress, prisma-diagram, summary-view, llm-config, criteria-editor) use scoped CSS with `var(--color-primary)` etc.
- **Tailwind views** (article-list, tag-label-management, article-table, status-badge, confidence-bar, tag-chip, label-chip) use Tailwind utility classes referencing the `@theme` tokens.

**Font loading:**
- Inter (400, 500, 600, 700, 800) loaded via Google Fonts `<link>` in `index.html`
- Material Symbols Outlined loaded via Google Fonts with `font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 24`

**Icons:**
- All navigation and UI icons use `<span class="material-symbols-outlined">icon_name</span>`
- No Unicode fallback characters in any view

**Design reference files:**
- 10 reference HTML screens in `docs/design-reference/` (01-dashboard.html through 10-llm-config.html)
- Comprehensive design patterns: `docs/design-reference/00-design-patterns.md`
- Implementation gaps documented: `docs/superpowers/plans/implementation-gaps.md`

### 18.4 Component Specifications

| Component | Spec |
|-----------|------|
| **Primary Button** | Solid Indigo background, white text, 8px radius |
| **Secondary Button** | Light gray ghost or subtle outline, 8px radius |
| **Pill Badge** (Status) | Fully rounded (999px), soft tinted background, dark text |
| **Colored Chip** (Tags) | Solid background, 8px radius |
| **Outlined Chip** (Labels) | 1px border, no fill, 8px radius |
| **Priority Indicator** | Solid circle or text-pill in semantic color; Optional uses dashed border |
| **Input Field** | 1px gray border → 2px Indigo on focus, no inner shadow |
| **Data Table** | Row-based, hover state `#F3F4F6`, horizontal rules only, no vertical dividers |

### 18.5 Screen Inventory

The following screens are designed in the Stitch project "Bango AI Literature Reviewer":

| Screen | Description | Key Elements |
|--------|-------------|-------------|
| **Project Dashboard** | Landing screen after opening the app | Project name, article counts by status (pill badges), "Start Screening" CTA, activity feed, quick-action cards (Import RIS, Edit Criteria, View PRISMA) |
| **Article List View** | Core data-heavy screen | Left sidebar with status tabs + counts, filterable/sortable table (checkbox, title, authors, year, journal, status badge, confidence bar, tag chips, label chips), top toolbar (search, sort, filter, bulk actions) |
| **Article Detail Panel** | Right-sliding side panel | Full title, scrollable abstract, metadata (DOI, journal, year, keywords), AI decision card (decision, confidence %, reasoning, matched criteria), editable tags, labels, audit trail timeline |
| **Criteria Editor** | Three-section editor | Research Aims (text entries, add/delete), Inclusion Criteria (text + priority dropdown), Exclusion Criteria (text + priority dropdown). Colored left borders for priority levels |
| **AI Screening Progress** | Batch screening monitor | Progress bar, processed/total count, batch info, live decision feed, pause/resume/stop controls, stats panel (included, rejected, error counts) |
| **RIS Import** | File import flow | Drag-and-drop zone, parsed article preview table (10 rows), import summary card, stepper (Upload → Parse → Dedup → Complete) |
| **Deduplication Review** | Side-by-side comparison | Two-panel view (Record A vs Record B), yellow-highlighted differences, Keep A / Keep B / Keep Both buttons, duplicate pair list with similarity scores |
| **PRISMA 2020 Flow Diagram** | Standard four-phase diagram | Identification → Screening → Eligibility → Included, record counts, exclusion arrows with reasons, export buttons (SVG/PNG), toggle for exclusion reason breakdown |
| **LLM Configuration** | Provider setup form | Provider dropdown, endpoint URL, model name, API key (masked), max tokens slider, concurrency, request delay, Test Connection button, static VRAM warning banner for local providers |
| **Tag & Label Management** | Dual-panel management | Tags (colored chips, article counts, add input, "Suggest Tags" button), Labels (outlined chips, add input, "Suggest Labels" button) |

---

## 19. Design-to-Code Integration (Stitch MCP)

### 19.1 Overview

The project uses [Google Stitch](https://stitch.withgoogle.com/) for UI design and the `@_davideast/stitch-mcp` package as the MCP bridge between Stitch and Claude Code. This enables a direct design-to-code pipeline without manual handoff.

### 19.2 Stitch Project

- **Project Name**: "Bango AI Literature Reviewer"
- **Project ID**: `4799487491058521486`
- **Design System**: "Scholarly Precision" (exported as `DESIGN.md`)
- **Screens**: 10 design screens + 1 design system instance + 2 icon variants

### 19.3 MCP Configuration

The Stitch MCP server is configured in `.claude/settings.local.json`:

```json
{
  "mcpServers": {
    "stitch": {
      "command": "npx",
      "args": ["-y", "@_davideast/stitch-mcp", "proxy"],
      "env": {
        "STITCH_API_KEY": "<stored-in-settings>"
      }
    }
  }
}
```

Authentication uses the `STITCH_API_KEY` environment variable (no browser-based OAuth required in development).

### 19.4 Available MCP Tools

| Tool | Description |
|------|-------------|
| `list_projects` | List all Stitch projects accessible with the configured API key |
| `list_screens` | List all screens within a project |
| `get_screen_code` | Download the HTML/CSS code for a specific screen |
| `get_screen_image` | Get a screenshot of a specific screen as base64 |
| `build_site` | Map screens to routes and generate a deployable site |

### 19.5 Workflow

```
Google Stitch (generate / iterate designs)
    → DESIGN.md (export design system tokens)
    → Claude Code reads DESIGN.md + pulls screen code via MCP
    → Generate Vue 3 components matching design tokens
    → Iterate in Stitch for visual refinements
    → Re-sync via DESIGN.md + MCP
```

**Step-by-step:**
1. Generate or refine designs in Google Stitch using the prompts in the Stitch prompt plan.
2. Export the updated design system as `DESIGN.md` (replaces the file in the project root).
3. Use Claude Code with the Stitch MCP to pull screen HTML/CSS and generate matching Vue components.
4. DESIGN.md provides the design token contract (colors, typography, spacing) - components reference these tokens rather than hardcoding values.
5. All design changes are version-controlled alongside code via `DESIGN.md`.

### 19.6 CLI Commands (Reference)

```bash
# List projects
npx @_davideast/stitch-mcp tool list_projects

# List screens in the Bango project
npx @_davideast/stitch-mcp tool list_screens -d '{"projectId": "4799487491058521486"}'

# Get screen code (HTML/CSS)
npx @_davideast/stitch-mcp tool get_screen_code -d '{"projectId": "4799487491058521486", "screenId": "<screen-id>"}'

# Get screen screenshot
npx @_davideast/stitch-mcp tool get_screen_image -d '{"projectId": "4799487491058521486", "screenId": "<screen-id>"}'

# Run health check
npx @_davideast/stitch-mcp doctor

# Interactive browser (requires terminal with raw mode support)
npx @_davideast/stitch-mcp view --projects
```

---

## 20. Scope Exclusions (v1)

The following features are explicitly **out of scope** for v1:

- **Mobile support** - desktop only for v1.
- **Multi-project workspace** - single project per app instance.
- Multi-user collaboration / real-time sync.
- Blind mode / conflict resolution between reviewers.
- Full-text screening (abstract only).
- PICO framework faceting sidebar.
- Swipe-based mobile screening gestures.
- Machine learning relevance scoring (5-star rating).
- Integration with external reference databases (PubMed API, etc.).
- Automated keyword highlighting within abstracts.
- **Per-article audit revert** - audit log is read-only.
- **PDF PRISMA export** - SVG and PNG only.
- **Dynamic VRAM detection** - static warning text only.

---

## Change Log

| Version | Date | Changes |
|---------|------|---------|
| v3.2 | 2026-05-08 | Article status model refactor: renamed `imported` status to `duplicate`. Non-duplicate articles now promote directly to `working` on import. Only true duplicates remain in `duplicate` status. Added cross-status dedup protection: articles already in `working`, `included`, or `rejected` are never affected by new imports. Updated state machine diagram, transitions, PRISMA data mapping, and workflow sequence. |
| v3.1 | 2026-05-05 | Design implementation update: added Tailwind CSS v4 with @theme tokens, disabled preflight for custom-CSS compatibility, Material Symbols Outlined icons replacing all Unicode fallbacks, Inter font loading, dual styling approach (custom CSS + Tailwind utilities). Documented design reference files and implementation gaps. |
| v3 | 2026-05-04 | Scope reductions: dropped mobile, single-project, simplified PRISMA exports, static VRAM warning, optional tag/label pass, no audit revert. Gap fills: screeningError as boolean flag, PRISMA data mapping, token estimation method, resume screening detail, export specVersion, import limit behavior, short-title dedup guard, multiple-import dedup scoping. Detail expansions: prompt templates for tag/label generation and AI summary, screening override note format, batch summary handling. |
| v2 | Prior | Second Specification (superseded by v3). |
| v1 | Prior | First Specification (archived). |
