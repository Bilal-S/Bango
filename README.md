# Bango

**AI-Powered Systematic Literature Review Tool**

Bango is a cross-platform desktop and mobile application that automates and accelerates the screening phase of systematic literature reviews, scoping reviews, and meta-analyses. Researchers import RIS bibliography files, define inclusion/exclusion criteria with weighted priorities, and let AI screen abstracts - producing a rigorously categorized set of articles ready for full-text review.

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

All data stays on your machine in a local SQLite database - no cloud upload required.

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
- Non-duplicate articles are promoted directly to the Working list on import
- Duplicates are placed in a separate Duplicates list (not a working list) with `duplicateOf` references
- Cross-status dedup protection: articles already in Working, Included, or Rejected are never affected by new duplicate imports

### Criteria-Based Screening
- Define research aims as a list of discrete text entries
- Define inclusion and exclusion criteria as discrete entries, each with a priority level
- Priority levels: **Critical**, **High**, **Standard**, **Low**, or **Optional**
- Deterministic conflict resolution: highest-priority matched rule wins; ties favor inclusion; no match defaults to exclude

### AI-Powered Abstract Screening
- Configure connections to hosted LLMs (OpenAI, Google, z.ai) or local setups (llama.cpp, Ollama, LM Studio), plus any OpenAI-compatible endpoint
- User provides the full endpoint URL - the app does not append paths
- AI evaluates each article's abstract in isolation as a separate API call
- Returns structured JSON with decision, reasoning paragraph, matched criteria, suggested tags, and confidence score
- Background batch processing with configurable concurrency (default: 3) and request delay (default: 500ms)
- Exponential backoff on rate limits; malformed responses flagged as screening errors
- Generates a structured AI summary of included articles: key themes, research trends, methodological strengths, common weaknesses, and literature gaps

### Tag & Label Management
- **Tags**: content-category labels (e.g., "machine-learning", "clinical-trial") - AI suggests from RIS keywords and user criteria; user can add, edit, delete
- **Labels**: workflow markers (e.g., "priority-read", "disputed") - AI generates from inclusion/exclusion criteria; user can expand and modify
- Tags and labels generated in a pre-screening pass; user reviews before AI screening begins
- Full manual editing - override any AI decision, adjust tags and labels, move articles between lists

### PRISMA 2020 Flow Diagram
- Standard four-phase PRISMA 2020 flow diagram with exact record counts
- Optional exclusion reason breakdown (user-controlled toggle)
- Rendered as SVG; exportable as SVG, PNG, and PDF

### Audit Trail
- Every state change, tag/label edit, and AI decision logged with timestamp and source
- Per-article history view with revert capability
- No global undo - corrections are made by moving articles or editing data

### Search, Sort, and Filter
- Full-text search across title and abstract fields
- Sort by title, publication year, date added, or AI confidence score
- Filter by status, tags, labels, matched criteria, publication year range, and manual override status

### Cross-Platform
- Desktop and mobile support via Tauri 2.x with feature parity
- Responsive layouts adapt to smaller screens
- Offline-capable - all data stored locally in SQLite; browse, search, and edit without a network connection
- AI screening requires an active LLM connection (hosted or local)

---

## Workflow

Articles flow through a strict state machine. An article exists in exactly one state at any time:

```
Import → Working (non-duplicate) or Duplicate (flagged)
Duplicate → (resolve) → Working
Working → (AI screening or manual) → Included | Rejected
```

| State | Description | Editable |
|-------|-------------|----------|
| **Duplicate** | Articles flagged as duplicates during import. Read-only until resolved via side-by-side review. | No (until resolved) |
| **Working** | Deduplicated articles awaiting screening. Non-duplicate articles arrive here directly on import. | Yes |
| **Included** | Articles meeting inclusion criteria | Yes |
| **Rejected** | Articles excluded based on criteria | Yes |

On import, deduplication runs against all existing articles. Non-duplicate articles are promoted directly to Working. Duplicate articles remain in the Duplicates list with `duplicateOf` references. If a newly imported article duplicates an article already in Working, Included, or Rejected, the existing article's status is never changed — the new article is placed in Duplicates referencing the accepted article.

Users can manually override AI decisions and move articles freely between Working, Included, and Rejected lists. The Duplicates list is read-only until individual duplicates are resolved.

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| **Framework** | Tauri 2.x - lightweight cross-platform runtime |
| **Frontend** | Vue 3.x with TypeScript, Tailwind CSS v4 |
| **Styling** | Tailwind CSS v4 (`@theme` design tokens) + CSS custom properties |
| **Backend** | Rust - memory-safe, non-blocking background processing |
| **Database** | Local SQLite - portable, offline-first |
| **AI** | REST API client in Rust - async requests to external or local LLM endpoints |
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

Bango v3 is in **active development**. The core backend (Rust) and all frontend views (Vue) are implemented. See the [v3 specification](docs/superpowers/specs/bango-v3-spec.md) for the full feature spec and [implementation gaps](docs/superpowers/plans/implementation-gaps.md) for remaining work.

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Node.js** | 18+ | [nodejs.org](https://nodejs.org/) |
| **Rust** | Latest stable | [rustup.rs](https://rustup.rs/) |
| **Tauri CLI** | v2.x | Included via `@tauri-apps/cli` in devDependencies |

---

## Getting Started

### Install dependencies

```bash
npm install
```

This also runs `simple-git-hooks` to set up pre-commit linting.

### Development (frontend only)

Starts the Vite dev server on `http://localhost:1420`:

```bash
npm run dev
```

Note: Tauri commands (`invoke`) will not work in browser-only mode. Use the Tauri dev command for full functionality.

### Development (full Tauri app)

Starts the Vite dev server + Rust backend in a native window:

```bash
npm run tauri dev
```

### Build for production

Compiles TypeScript, builds the Vite frontend, and bundles the Tauri app:

```bash
npm run tauri build
```

---

## Testing

### Frontend tests (Vitest)

```bash
npm test            # Run once
npm run test:watch  # Watch mode
```

### Backend tests (Rust)

```bash
cd src-tauri && cargo test
```

### Run all checks

Runs ESLint, Prettier, Rust formatting, and Clippy:

```bash
npm run check:all
```

Individual commands:

| Command | What it does |
|---------|-------------|
| `npm run lint:check` | ESLint on `.ts` and `.vue` files |
| `npm run format:check` | Prettier formatting check |
| `npm run lint:rust` | Cargo clippy with `-D warnings` |
| `npm run format:rust` | Cargo `fmt --check` |

---

## Project Structure

```
├── src/                        # Vue 3 frontend
│   ├── components/             # Reusable Vue components
│   ├── composables/            # Vue composables (shared reactive logic)
│   ├── stores/                 # Pinia stores for global state
│   ├── views/                  # Page-level components (one per route)
│   ├── types/                  # TypeScript interfaces
│   ├── styles/                 # CSS: base.css (Tailwind + tokens), tokens.css (CSS variables)
│   └── main.ts                 # App entry point
├── src-tauri/                  # Rust backend
│   ├── src/
│   │   ├── commands/           # Tauri command handlers (one per domain)
│   │   ├── db/                 # SQLite repos, migrations, connection
│   │   ├── models/             # Domain models (Article, Tag, Label, etc.)
│   │   ├── ris/                # RIS parser, validator, types
│   │   ├── screening/          # AI screening engine, prompt builder
│   │   ├── dedup/              # Deduplication engine, similarity scoring
│   │   ├── export/             # RIS writer, project backup
│   │   ├── llm/                # LLM HTTP client
│   │   ├── prisma/             # PRISMA diagram data + SVG generation
│   │   └── crypto/             # AES-256-GCM encryption for API keys
│   └── tests/                  # Rust integration tests
├── docs/
│   ├── design-reference/       # 10 Stitch HTML reference screens + patterns doc
│   └── superpowers/
│       ├── specs/              # v3 specification
│       └── plans/              # Implementation plans + gaps doc
├── DESIGN.md                   # Scholarly Precision design system tokens
└── CLAUDE.md                   # Coding rules for Claude Code
```

---

## Design System

The UI follows the **Scholarly Precision** design system defined in `DESIGN.md`. Reference designs for all 10 screens are in `docs/design-reference/`.

- **Colors**: Material Design 3 inspired palette (Indigo primary, Slate sidebar, cool gray surfaces)
- **Typography**: Inter font family, sizes from 11px (label-caps) to 24px (display)
- **Icons**: Material Symbols Outlined via Google Fonts
- **CSS approach**: Tailwind CSS v4 (`@theme` tokens) + CSS custom properties (`tokens.css`). Tailwind preflight is disabled to support views using scoped CSS.

---

## License

TBD
