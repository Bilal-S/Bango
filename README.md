<div align="center">

<img src="design/logo.png" alt="Bango Logo" width="120" />

# Bango

**AI-accelerated systematic literature review screening**

[![Version](https://img.shields.io/badge/version-0.4.0-blue.svg)](https://github.com/Bilal-S/Bango)
[![License](https://img.shields.io/badge/license-Apache%202.0-green.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-orange.svg)](https://tauri.app/)
[![Status](https://img.shields.io/badge/status-active%20development-yellow.svg)]()

Bango is a cross-platform desktop application that automates and accelerates the screening phase of systematic literature reviews, scoping reviews, and meta-analyses. Researchers import RIS bibliography files, define inclusion/exclusion criteria, and let AI screen abstracts — producing a rigorously categorized set of articles ready for full-text review.

Built with [Tauri 2.x](https://tauri.app/) · All data stays on your machine · No cloud dependency

**Author:** [Bilal Soylu (BonCode)](https://github.com/Bilal-S)

</div>

---

## ✨ Highlights

| | Feature | |
|---|---|---|
| 📥 | **RIS Import & Export** | 30+ metadata fields, multi-file import, valid RIS export with AI-generated annotations |
| 🔍 | **Intelligent Deduplication** | DOI, title, year, author matching with Levenshtein similarity and manual review |
| 🤖 | **AI-Powered Screening** | Batch abstract evaluation against your criteria via hosted or local LLMs |
| 🏷️ | **Tags & Labels** | AI-suggested content tags and workflow labels with full manual override |
| 📊 | **PRISMA 2020 Diagrams** | Auto-generated four-phase flow diagrams with exclusion reason breakdowns |
| 🔒 | **Offline & Private** | Local SQLite database, AES-256-GCM encrypted API keys, no cloud upload |
| 📝 | **Audit Trail** | Every state change, tag edit, and AI decision logged with timestamp |

> **Note:** v1 is desktop-only. Mobile support is deferred to a future release.

---

## 📸 Screenshots

<!-- Uncomment when ready to add screenshots:
<table>
  <tr>
    <td><img src="screenshots/dashboard.png" alt="Dashboard" width="400" /></td>
    <td><img src="screenshots/screening.png" alt="Screening Progress" width="400" /></td>
  </tr>
  <tr>
    <td align="center"><em>Dashboard</em></td>
    <td align="center"><em>Screening Progress</em></td>
  </tr>
</table>
-->

*Screenshots coming soon.*

---

## 📋 Table of Contents

- [Workflow](#workflow)
- [Key Features](#key-features)
- [Tech Stack](#tech-stack)
- [AI Integration](#ai-integration)
- [Getting Started](#getting-started)
- [Testing](#testing)
- [Project Structure](#project-structure)
- [Design System](#design-system)
- [License](#license)

---

## 🔄 Workflow

Articles flow through a strict state machine. An article exists in exactly one state at any time:

```
Import → Working (non-duplicate) or Duplicate (flagged)
  Duplicate → (resolve) → Working
  Working → (AI screening or manual) → Included | Rejected
```

| State | Description | Editable |
|-------|-------------|----------|
| **Duplicate** | Flagged as duplicates during import. Read-only until resolved via side-by-side review. | No (until resolved) |
| **Working** | Deduplicated articles awaiting screening. Non-duplicates arrive here directly on import. | Yes |
| **Included** | Articles meeting inclusion criteria. | Yes |
| **Rejected** | Articles excluded based on criteria. | Yes |

On import, deduplication runs against all existing articles. Non-duplicates are promoted directly to Working. If a newly imported article duplicates one already in Working, Included, or Rejected, the existing article's status is never changed — the new article is placed in Duplicates referencing the accepted article.

Users can manually override AI decisions and move articles freely between Working, Included, and Rejected.

---

## 📦 Key Features

<details>
<summary><strong>📥 Data Import & Export</strong></summary>

- Import RIS bibliography files with full metadata (title, abstract, authors, year, DOI, journal, keywords, and more)
- Import multiple RIS files into a single project with automatic re-deduplication
- Export the included list in valid RIS format, with AI-generated tags (`KW`), reasoning notes (`N1`), user notes (`NO`), and inclusion/exclusion labels (`C1` as JSON)
- Full project export/import as a single `.bango.json` file with AES-256-GCM encrypted API keys

</details>

<details>
<summary><strong>🔍 Intelligent Deduplication</strong></summary>

- Multi-strategy matching: DOI exact match, title+year exact (≥95% similarity), fuzzy title+year (70–94%), author+title partial
- Levenshtein distance-based title comparison with normalization
- Auto-merge exact duplicates; flag fuzzy matches for side-by-side manual review
- Non-duplicate articles are promoted directly to the Working list on import
- Duplicates are placed in a separate Duplicates list with `duplicateOf` references
- Cross-status dedup protection: articles already in Working, Included, or Rejected are never affected by new duplicate imports

</details>

<details>
<summary><strong>📋 Criteria-Based Screening</strong></summary>

- Define research aims as a list of discrete text entries
- Define inclusion and exclusion criteria as discrete entries, each with a priority level
- Priority levels: **Critical**, **High**, **Standard**, **Low**, or **Optional**
- Deterministic conflict resolution: highest-priority matched rule wins; ties favor inclusion; no match defaults to exclude

</details>

<details>
<summary><strong>🤖 AI-Powered Abstract Screening</strong></summary>

- Configure connections to hosted LLMs (OpenAI, Anthropic, Google, Mistral AI, z.ai) or local setups (llama.cpp, Ollama, LM Studio), plus any OpenAI-compatible endpoint
- AI evaluates article abstracts in **batches** (1–5 articles per call) for high throughput
- Returns structured JSON with decision, reasoning paragraph, matched criteria, suggested tags, and confidence score
- Background batch processing with configurable concurrency (default: 3) and request delay (default: 500ms)
- Exponential backoff on rate limits; malformed responses flagged as screening errors
- Generates a structured AI summary of included articles: key themes, research trends, methodological strengths, common weaknesses, and literature gaps

</details>

<details>
<summary><strong>🏷️ Tag & Label Management</strong></summary>

- **Tags**: content-category labels (e.g., "machine-learning", "clinical-trial") — AI suggests from RIS keywords and user criteria; user can add, edit, delete
- **Labels**: workflow markers (e.g., "priority-read", "disputed") — AI generates from inclusion/exclusion criteria; user can expand and modify
- Tags and labels generated in a pre-screening pass; user reviews before AI screening begins
- Full manual editing — override any AI decision, adjust tags and labels, move articles between lists

</details>

<details>
<summary><strong>📊 PRISMA 2020 Flow Diagram</strong></summary>

- Standard four-phase PRISMA 2020 flow diagram with exact record counts
- Optional exclusion reason breakdown (user-controlled toggle)
- Rendered as SVG; exportable as SVG and PNG

</details>

<details>
<summary><strong>📝 Audit Trail & Search</strong></summary>

- Every state change, tag/label edit, and AI decision logged with timestamp and source
- Per-article history view
- Full-text search across title and abstract fields
- Sort by title, publication year, date added, or AI confidence score
- Filter by status, tags, labels, matched criteria, publication year range, and manual override status

</details>

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|------------|
| **Framework** | [Tauri 2.x](https://tauri.app/) — lightweight cross-platform runtime |
| **Frontend** | [Vue 3](https://vuejs.org/) + TypeScript |
| **Styling** | [Tailwind CSS v4](https://tailwindcss.com/) (`@theme` design tokens) + CSS custom properties |
| **Backend** | [Rust](https://www.rust-lang.org/) — memory-safe, non-blocking background processing |
| **Database** | Local [SQLite](https://www.sqlite.org/) — portable, offline-first |
| **AI** | REST API client in Rust — async requests to external or local LLM endpoints |
| **Encryption** | AES-256-GCM with PBKDF2 key derivation |

---

## 🤖 AI Integration

Bango supports a range of LLM providers to fit different budgets and privacy requirements.

All providers use a **user-provided full endpoint URL** — the app does not append paths. API keys are encrypted locally with AES-256-GCM using a machine-derived key. Project exports do not include keys; collaborators must provide their own.

**Hosted Providers:** OpenAI · Anthropic · Google (Gemini) · Mistral AI · z.ai

**Local Providers:** llama.cpp · Ollama · LM Studio

**Custom:** Any OpenAI-compatible endpoint

> ⚠️ The system requires an LLM with a context window of **50,000 tokens or larger**. A warning is displayed if a local provider is selected.

---

## 🚀 Getting Started

### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Node.js** | 18+ | [nodejs.org](https://nodejs.org/) |
| **Rust** | Latest stable | [rustup.rs](https://rustup.rs/) |
| **Tauri CLI** | v2.x | Included via `@tauri-apps/cli` in devDependencies |

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

> **Note:** Tauri commands (`invoke`) will not work in browser-only mode. Use the Tauri dev command for full functionality.

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

## 🧪 Testing

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

| Command | What it does |
|---------|-------------|
| `npm run lint:check` | ESLint on `.ts` and `.vue` files |
| `npm run format:check` | Prettier formatting check |
| `npm run lint:rust` | Cargo clippy with `-D warnings` |
| `npm run format:rust` | Cargo `fmt --check` |

---

## 📁 Project Structure

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

## 🎨 Design System

The UI follows the **Scholarly Precision** design system defined in [`DESIGN.md`](DESIGN.md). Reference designs for all 10 screens are in [`docs/design-reference/`](docs/design-reference/).

- **Colors**: Material Design 3 inspired palette (Indigo primary, Slate sidebar, cool gray surfaces)
- **Typography**: Inter font family, sizes from 11px (label-caps) to 24px (display)
- **Icons**: Material Symbols Outlined via Google Fonts
- **CSS approach**: Tailwind CSS v4 (`@theme` tokens) + CSS custom properties (`tokens.css`). Tailwind preflight is disabled to support views using scoped CSS.

---

## 📄 License

This project is licensed under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).

```
Copyright 2024-2026 Bango Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
