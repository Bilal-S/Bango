# Bango — Second Specification

Comprehensive specification incorporating all resolved ambiguities from the initial requirements. Supersedes `initial reqs.md`.

---

## 1. Product Overview

Bango is a cross-platform desktop and mobile application for AI-assisted systematic literature review. Researchers import RIS bibliography files, define inclusion/exclusion criteria, and use LLMs to screen article abstracts — producing a rigorously categorized set of articles with reasoning, tags, and labels.

Built with **Tauri 2.x** for a lightweight, offline-capable experience. All data is stored locally in SQLite. No cloud upload is required.

---

## 2. Terminology

| Term | Definition | Source |
|------|-----------|--------|
| **Tag** | A content-category label describing an article's topic, methodology, or relevance. E.g., "machine-learning", "clinical-trial". | AI generates from RIS `KW` (Keywords/Topics) field and user criteria. User can add, edit, delete. |
| **Label** | A workflow marker for organizational/process tracking. E.g., "priority-read", "disputed", "needs-full-text". | AI generates an initial set from inclusion and exclusion criteria. User can expand and modify. |
| **Criterion** | A discrete inclusion or exclusion rule with an assigned priority level. | User-defined. |
| **Research Aim** | A discrete statement of research objective. | User-defined. |
| ~~Meta-tag~~ | Term removed from specification. References to "meta-tag" in prior documents are understood to mean **Tag**. | — |

---

## 3. Domain Model

```json
{
  "Project": {
    "id": "uuid",
    "name": "string",
    "createdAt": "ISO-8601 timestamp",
    "lastModified": "ISO-8601 timestamp",
    "researchAims": ["ResearchAim"],
    "criteria": ["Criterion"],
    "tags": ["Tag"],
    "labels": ["Label"],
    "articles": ["Article"],
    "llmConfig": "LLMConfig",
    "auditLog": ["AuditEntry"]
  },
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
    "source": "enum[ai_suggested, user_created, ris_keyword]"
  },
  "Label": {
    "id": "uuid",
    "name": "string",
    "source": "enum[ai_generated, user_created]"
  },
  "Article": {
    "id": "uuid",
    "status": "enum[imported, working, included, rejected, screening_error]",
    "title": "string (required)",
    "abstract": "string (required)",
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
    "inclusionExclusionLabels": "object | null (JSON with 'inc' and 'exc' arrays of label names)",
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
    "screenedAt": "ISO-8601 timestamp | null"
  },
  "LLMConfig": {
    "provider": "enum[openai, google, z_ai, llama_cpp, ollama, lm_studio, custom]",
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
  }
}
```

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
| `ER` | End of Reference | *(parser delimiter — not stored as article data)* | No |
| `J9` | Journal Abbreviation (29-char) | `journalAbbreviation` | No |
| `JI` | Journal ISO Abbreviation | `journalIsoAbbreviation` | No |
| `N1` | Notes | `notes` | No |
| `PA` | Publisher Address | `publisherAddress` | No |
| `PI` | Publisher City | `publisherCity` | No |
| `PU` | Publisher | `publisher` (fallback for PB) | No |
| `WE` | Web of Science Database | `webOfScienceDb` | No |

All unrecognized RIS tags are preserved as key-value pairs in `risExtras`.

### 4.2 Import Validation

- Articles missing **Title**, **Abstract**, or **Authors** are **rejected** at import with a specific parse error indicating which required field is missing.
- If `AB` is missing but `N2` is present, `N2` is used as the abstract.
- Multiple `AU` tags for a single article are collected into the `authors[]` array.
- Multiple `KW` tags for a single article are collected into the `keywords[]` array.
- Each import records the source filename in `importSource`.

### 4.3 Multiple Imports

- Users can import **multiple RIS files** into a single project.
- Each import adds articles to the Imported list.
- Deduplication re-runs across the **entire** Imported list after each new import.
- Already-screened articles (in Working/Included/Rejected) are **not** affected by subsequent imports.

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

Strategies are evaluated in order. First match wins. Articles without DOI skip strategy 1.

### 5.2 Title Normalization

Before comparison, titles are:
1. Converted to lowercase
2. Stripped of all punctuation (`.,;:!?'"-()[]{}`)
3. Collapsed whitespace to single spaces
4. Trimmed

### 5.3 Merge Behavior

- **Exact duplicates**: The article with the most complete metadata (highest non-null field count) is retained as the surviving article. The other is marked with `duplicateOf: <surviving_id>` and remains in the Imported list.
- **Fuzzy matches**: Both articles remain in Imported. The user is presented a side-by-side comparison view and chooses: keep left, keep right, or keep both (not duplicates). The rejected article is marked with `duplicateOf`.
- Surviving articles advance to the Working list. Duplicates remain in Imported (read-only audit trail).

---

## 6. Criteria and Priority System

### 6.1 Research Aims

- Research aims are a **list of discrete text entries** (not a single free-text block).
- Each aim is entered individually by the user.
- No priority level is assigned to aims — they serve as context for the AI screening.

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

This is deterministic logic applied by the app after the AI reports which criteria matched. The AI does not resolve priority conflicts — it only identifies matches.

---

## 7. Article State Machine

### 7.1 States

| State | Description |
|-------|-------------|
| **Imported** | Raw article from RIS file. Read-only. |
| **Working** | Deduplicated article awaiting or pending screening. |
| **Included** | Article meeting inclusion criteria. |
| **Rejected** | Article excluded based on criteria. |
| **Screening Error** | Transient state — article failed AI screening and remains in Working. |

### 7.2 State Transition Diagram

```
                    ┌──────────────────────────────────┐
                    │                                  │
                    ▼                                  │
  Imported ──(dedup)──► Working ◄─────────────────────┤
                              │                       │
                 ┌────────────┼────────────┐          │
                 ▼            ▼            │          │
            Included     Rejected         │          │
                 │            │            │          │
                 └──────► Working ◄────────┘          │
                          │  ▲                        │
                          │  └────────────────────────┘
                          │
                    Manual moves:
              Working ↔ Included ↔ Rejected
              Imported: READ-ONLY
```

### 7.3 Allowed Transitions

| From | To | Trigger | Notes |
|------|----|---------|-------|
| Imported | Working | Deduplication | Surviving articles only. Duplicates stay in Imported with `duplicateOf`. |
| Working | Included | AI screening or manual | AI sets `aiDecision`, `aiReasoning`, `aiConfidence`, matched criteria. |
| Working | Rejected | AI screening or manual | Same as above. |
| Included | Working | Manual override | Sets `manualOverride: true`. |
| Included | Rejected | Manual override | Sets `manualOverride: true`. |
| Rejected | Working | Manual override | Sets `manualOverride: true`. |
| Rejected | Included | Manual override | Sets `manualOverride: true`. |

All state changes create an `AuditEntry` record.

### 7.4 Screening Error Handling

When the LLM returns a malformed/invalid response for an article:
- Article stays in the **Working** list.
- Article is flagged with a visible `screening_error` indicator in the UI.
- The raw LLM response is logged in the audit trail.
- User can retry the article individually or re-run screening for all errored articles.

---

## 8. Tag and Label Generation

### 8.1 Timing

Tags and labels are generated **before AI screening**, after the user has defined criteria.

**Workflow sequence:**
1. Import RIS → articles in Imported list
2. Run deduplication → articles move to Working list
3. User defines research aims, inclusion criteria, exclusion criteria (with priorities)
4. **AI generates initial Tags and Labels** (pre-screening pass)
5. User reviews and edits tags and labels
6. User triggers AI screening → articles move to Included or Rejected
7. User can manually move articles, adjust tags/labels at any time
8. User triggers AI summary of included articles
9. User exports RIS / PRISMA / project backup

### 8.2 Tag Generation

- **Source data**: AI scans the `keywords` (RIS `KW` / Topics) field of all articles in the Working list, plus all user-defined inclusion and exclusion criteria.
- **Output**: A set of suggested Tags representing content categories relevant to the review.
- Each Tag records its `source` as `ai_suggested`, `user_created`, or `ris_keyword`.
- User can add, rename, merge, or delete any Tag.

### 8.3 Label Generation

- **Source data**: AI scans all user-defined inclusion and exclusion criteria.
- **Output**: A set of suggested Labels representing workflow/process categories derived from the screening criteria.
- Each Label records its `source` as `ai_generated` or `user_created`.
- User can add, rename, merge, or delete any Label.

---

## 9. AI Screening Process

### 9.1 Per-Article Screening

Each article is sent to the LLM as a **separate API call**. The prompt includes:

**System prompt:**
> You are a systematic literature review screening assistant. Evaluate the provided article abstract against the research aims, inclusion criteria, and exclusion criteria. Return your evaluation as structured JSON matching the required schema.

**User prompt:**
```
## Research Aims
{numbered list of aim entries}

## Inclusion Criteria
{numbered list with id, text, priority}

## Exclusion Criteria
{numbered list with id, text, priority}

## Priority Rules
- Higher priority rules always outweigh lower priority rules.
- If inclusion and exclusion criteria of equal priority both match, favor inclusion.

## Article
Title: {title}
Authors: {authors}
Year: {publicationYear}
Abstract: {abstract}

## Response Format
Return JSON exactly matching this schema:
{
  "decision": "include" | "exclude",
  "reasoning": "A paragraph citing specific sentences from the abstract to justify the decision.",
  "matched_inclusion_criteria": ["criteria-id-1", ...],
  "matched_exclusion_criteria": ["criteria-id-3", ...],
  "suggested_tags": ["tag-name-1", ...],
  "confidence": 0.0-1.0
}
```

### 9.2 Response Processing

1. App parses the JSON response.
2. App applies **deterministic priority conflict resolution** (Section 6.3) based on matched criteria and their priorities. The AI's `decision` field is advisory — the app computes the final decision.
3. If the app's computed decision differs from the AI's `decision`, the app's decision takes precedence and a note is added to `aiReasoning`.
4. The article moves to Included or Rejected.
5. AI-suggested tags are matched to existing tags or created as new `ai_suggested` tags.

### 9.3 Batch Processing

- **Concurrency**: Configurable, default 3 concurrent requests.
- **Delay**: Configurable, default 500ms between requests.
- **On HTTP 429 (rate limit)**: Exponential backoff with max 3 retries (delay: 1s, 2s, 4s).
- **On malformed JSON**: Article stays in Working, flagged as `screening_error`.
- **On connection failure**: Screening job pauses. Completed articles are saved. User can resume or cancel.

### 9.4 Progress Tracking

- Progress bar: `X / Y articles screened (Z%)`
- Estimated time remaining based on rolling average per-article processing time.
- **Pause** button: stops after the current article completes.
- **Cancel** button: stops immediately. Partially screened articles remain in Working.

---

## 10. LLM Configuration

### 10.1 Supported Providers

| Provider | Config | Default Endpoint (suggestion only) |
|----------|--------|-----------------------------------|
| OpenAI | API key required | `https://api.openai.com/v1/chat/completions` |
| Google | API key required | Gemini API endpoint |
| z.ai | API key + full URL | User provides full URL (e.g., `https://api.z.ai/api/coding/paas/v4`) |
| Ollama | No auth | `http://localhost:11434/v1/chat/completions` |
| llama.cpp | No auth | `http://localhost:8080/v1/chat/completions` |
| LM Studio | No auth | `http://localhost:1234/v1/chat/completions` |
| Custom | API key (optional) + full URL | User provides |

All providers use the **OpenAI-compatible chat completions format**. Google Gemini requires a provider-specific adapter. The user provides the **full endpoint URL** — the app does not append `/v1/chat/completions` or any other path.

### 10.2 Connection Testing

- "Test Connection" button sends a minimal chat completion request to the configured endpoint.
- Success: saves configuration and displays success message.
- Failure: displays specific error (connection refused, auth failed, timeout, etc.).

### 10.3 Hardware Warning

- App queries local system VRAM.
- If a local provider (llama.cpp, Ollama, LM Studio) is selected and VRAM < 16 GB, display a warning.
- The warning does not block configuration — user can proceed.

### 10.4 Context Window

- The app requires an LLM with a context window of **50,000 tokens or larger**.
- The app estimates the combined prompt token count before starting screening.
- If the estimated count approaches the configured context window, the user is warned.
- No hard block — user can override.

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

---

## 12. PRISMA 2020 Flow Diagram

### 12.1 Diagram Specification

- **Standard PRISMA 2020 four-phase flow**:
  1. **Identification**: Records identified through database searching → Records after duplicates removed.
  2. **Screening**: Records screened → Records excluded (with count).
  3. **Eligibility**: Full-text articles assessed for eligibility → Articles excluded (with count and optional reason breakdown).
  4. **Included**: Final number of studies included in the review.

- Counts at each phase are populated from the actual article states in the database.
- **Exclusion reason breakdown**: Optional, user-controlled toggle. When enabled, shows the number of articles excluded per matched exclusion criterion.

### 12.2 Rendering

- Rendered as **SVG** in the UI.
- **Export formats**: SVG, PNG, PDF.
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

---

## 14. Audit Trail

- Every state change, tag/label change, criteria match, and AI screening decision creates an `AuditEntry`.
- Audit entries are visible per-article in a detail panel.
- User can view the full history of any article and revert to a previous state (per-article revert).
- No global undo stack — changes are corrected by moving articles or editing tags/labels.

---

## 15. Export and Import

### 15.1 RIS Export

- Exports the Included list in valid RIS format.
- Includes all original metadata fields.
- Appends AI-generated tags as `KW` entries.
- Appends AI reasoning as an `N1` (Notes) field.
- User-entered notes on an article are exported using the `NO` RIS tag.
- Inclusion and exclusion labels are exported using the `C1` RIS tag as a JSON object with `inc` and `exc` arrays. Example:
  ```
  C1 - {"exc":["morbid","large"],"inc":["journal","blue"]}
  ```
- Compatible with Zotero, Mendeley, and EndNote.

### 15.2 Project Export

- Single `.bango.json` file.
- Contains: project settings, research aims, criteria, all articles (all states), tags, labels, audit log, LLM config (provider name + endpoint URL + model name).
- **API keys are included**, encrypted with a **user-provided password** using AES-256-GCM.
- The recipient must enter the password to import the project and access the LLM configuration.

### 15.3 Project Import

- Accepts a `.bango.json` file.
- User enters the password to decrypt API keys.
- If password is incorrect, project imports without API keys (user must re-enter).
- Imported project creates a new project (does not merge with existing).

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
| Local AI VRAM | Warning displayed if < 16 GB (non-blocking) |

### 16.3 Security

| Mechanism | Details |
|-----------|---------|
| API key storage (local) | AES-256-GCM encryption. Key derived via PBKDF2 from machine hostname + username + app salt. |
| API key storage (export) | Re-encrypted with user-provided password using AES-256-GCM. |
| SQLite database | Not encrypted (local app, OS-level security). |
| App access control | None (Tauri desktop app). |

### 16.4 Offline Capability

- All data is stored locally in SQLite — fully browsable offline.
- AI screening requires an active LLM connection (hosted or local).
- If connection is lost mid-screening, completed articles are saved; pending articles remain in Working.
- User can browse, search, sort, filter, and manually move articles while offline.

---

## 17. Mobile Support (v1)

- **Feature parity** with desktop: all features available on mobile.
- Responsive UI adapts layouts for smaller screens.
- Standard tap-based interactions (no swipe gestures in v1).
- Local LLM connections supported on mobile (VRAM warning applies).
- SQLite database is shared across platforms.

---

## 18. Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Framework | Tauri 2.x | Lightweight cross-platform runtime (desktop + mobile) |
| Frontend | Vue 3.x + TypeScript | Strict type-checking for complex UI state |
| Backend | Rust | Memory-safe, non-blocking background processing |
| Database | SQLite (local) | Portable, offline-first |
| AI Integration | REST API client in Rust | Async requests to external or local LLM endpoints |
| Encryption | AES-256-GCM + PBKDF2 | Industry-standard symmetric encryption |

---

## 19. Scope Exclusions (v1)

The following features are explicitly **out of scope** for v1:

- Multi-user collaboration / real-time sync
- Blind mode / conflict resolution between reviewers
- Full-text screening (abstract only)
- PICO framework faceting sidebar
- Swipe-based mobile screening gestures
- Machine learning relevance scoring (5-star rating)
- Integration with external reference databases (PubMed API, etc.)
- Automated keyword highlighting within abstracts
