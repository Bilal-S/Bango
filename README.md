# Bango

**AI-Powered Systematic Literature Review Tool**

Bango is a cross-platform desktop and mobile application that automates and accelerates the screening phase of systematic literature reviews, scoping reviews, and meta-analyses. Researchers import RIS bibliography files, define inclusion/exclusion criteria with weighted priorities, and let AI screen abstracts — producing a rigorously categorized set of articles ready for full-text review.

Built with [Tauri 2.x](https://tauri.app/) for a lightweight, offline-capable experience with no cloud dependency.

---

## Overview

Traditional systematic review workflows rely on spreadsheets and manual screening, which is slow, error-prone, and difficult to collaborate on. Bango brings this process into a modern, local-first application that:

- Imports and parses RIS bibliography files with 30+ supported metadata fields
- Deduplicates records using multi-strategy matching (DOI, title, year, author) with Levenshtein similarity
- Lets researchers define research aims, inclusion criteria, and exclusion criteria with five priority levels
- Connects to local or hosted LLMs to screen article abstracts against those criteria
- Produces tagged, labeled, and reasoned inclusion/exclusion decisions
- Exports results in RIS format compatible with Zotero, Mendeley, and EndNote
- Generates PRISMA 2020 flow diagrams with optional exclusion reason breakdowns
- Supports full project backup and transfer via encrypted `.bango.json` files

All data stays on your machine in a local SQLite database — no cloud upload required.

---

## Key Features

### Data Import & Export
- Import RIS bibliography files with full metadata (title, abstract, authors, year, DOI, journal, keywords, and more)
- Import multiple RIS files into a single project with automatic re-deduplication
- Export the included list in valid RIS format, with AI-generated tags (`KW`), reasoning notes (`N1`), user notes (`NO`), and inclusion/exclusion labels (`C1` as JSON)
- Full project export/import as a single `.bango.json` file with AES-256-GCM encrypted API keys

### Intelligent Deduplication
- Multi-strategy matching: DOI exact match, title+year exact (>=95% similarity), fuzzy title+year (70–94%), author+title partial
- Levenshtein distance-based title comparison with normalization
- Auto-merge exact duplicates; flag fuzzy matches for side-by-side manual review
- Duplicates preserved in the Imported list as an audit trail with `duplicateOf` references

### Criteria-Based Screening
- Define research aims as a list of discrete text entries
- Define inclusion and exclusion criteria as discrete entries, each with a priority level
- Priority levels: **Critical**, **High**, **Standard**, **Low**, or **Optional**
- Deterministic conflict resolution: highest-priority matched rule wins; ties favor inclusion; no match defaults to exclude

### AI-Powered Abstract Screening
- Configure connections to hosted LLMs (OpenAI, Google, z.ai) or local setups (llama.cpp, Ollama, LM Studio), plus any OpenAI-compatible endpoint
- User provides the full endpoint URL — the app does not append paths
- AI evaluates each article's abstract in isolation as a separate API call
- Returns structured JSON with decision, reasoning paragraph, matched criteria, suggested tags, and confidence score
- Background batch processing with configurable concurrency (default: 3) and request delay (default: 500ms)
- Exponential backoff on rate limits; malformed responses flagged as screening errors
- Generates a structured AI summary of included articles: key themes, research trends, methodological strengths, common weaknesses, and literature gaps

### Tag & Label Management
- **Tags**: content-category labels (e.g., "machine-learning", "clinical-trial") — AI suggests from RIS keywords and user criteria; user can add, edit, delete
- **Labels**: workflow markers (e.g., "priority-read", "disputed") — AI generates from inclusion/exclusion criteria; user can expand and modify
- Tags and labels generated in a pre-screening pass; user reviews before AI screening begins
- Full manual editing — override any AI decision, adjust tags and labels, move articles between lists

### PRISMA 2020 Flow Diagram
- Standard four-phase PRISMA 2020 flow diagram with exact record counts
- Optional exclusion reason breakdown (user-controlled toggle)
- Rendered as SVG; exportable as SVG, PNG, and PDF

### Audit Trail
- Every state change, tag/label edit, and AI decision logged with timestamp and source
- Per-article history view with revert capability
- No global undo — corrections are made by moving articles or editing data

### Search, Sort, and Filter
- Full-text search across title and abstract fields
- Sort by title, publication year, date added, or AI confidence score
- Filter by status, tags, labels, matched criteria, publication year range, and manual override status

### Cross-Platform
- Desktop and mobile support via Tauri 2.x with feature parity
- Responsive layouts adapt to smaller screens
- Offline-capable — all data stored locally in SQLite; browse, search, and edit without a network connection
- AI screening requires an active LLM connection (hosted or local)

---

## Workflow

Articles flow through a strict state machine. An article exists in exactly one state at any time:

```
Imported → (deduplication) → Working → (AI screening) → Included | Rejected
```

| State | Description | Editable |
|-------|-------------|----------|
| **Imported** | Raw articles from the original RIS file(s) | Read-only (audit trail) |
| **Working** | Deduplicated articles awaiting screening | Yes |
| **Included** | Articles meeting inclusion criteria | Yes |
| **Rejected** | Articles excluded based on criteria | Yes |

Users can manually override AI decisions and move articles freely between Working, Included, and Rejected lists. The Imported list is read-only to preserve the audit trail.

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| **Framework** | Tauri 2.x — lightweight cross-platform runtime |
| **Frontend** | Vue 3.x with TypeScript |
| **Backend** | Rust — memory-safe, non-blocking background processing |
| **Database** | Local SQLite — portable, offline-first |
| **AI** | REST API client in Rust — async requests to external or local LLM endpoints |
| **Encryption** | AES-256-GCM with PBKDF2 key derivation |

---

## AI Integration

Bango supports a range of LLM providers to fit different budgets and privacy requirements:

| Provider | Type | Endpoint |
|----------|------|----------|
| OpenAI | Hosted | User-provided full URL |
| Google | Hosted | User-provided full URL (Gemini adapter) |
| z.ai | Hosted | User-provided full URL |
| llama.cpp | Local | User-provided full URL |
| Ollama | Local | User-provided full URL |
| LM Studio | Local | User-provided full URL |
| Custom | Either | User-provided full URL |

> **Note:** The system requires an LLM with a context window of 50,000 tokens or larger. A warning is displayed if a local provider is selected on a machine with less than 16 GB VRAM.

API keys are encrypted locally with AES-256-GCM using a machine-derived key. Project exports re-encrypt keys with a user-provided password for secure transfer to collaborators.

---

## Project Status

Bango is currently in the **requirements and planning** phase. The specification is complete and development has not yet started.

See the [requirements documents](./Requirment/) for detailed functional and non-functional specifications.

---

## Getting Started

> Setup instructions will be added once development begins.

---

## License

TBD
