# Bango

**AI-Powered Systematic Literature Review Tool**

Bango is a cross-platform desktop and mobile application that automates and accelerates the screening phase of systematic literature reviews, scoping reviews, and meta-analyses. Researchers import bibliography files, define inclusion/exclusion criteria, and let AI screen abstracts — producing a rigorously categorized set of articles ready for full-text review.

Built with [Tauri 2.x](https://tauri.app/) for a lightweight, offline-capable experience with no cloud dependency.

---

## Overview

Traditional systematic review workflows rely on spreadsheets and manual screening, which is slow, error-prone, and difficult to collaborate on. Bango brings this process into a modern, local-first application that:

- Imports and parses standard RIS bibliography files
- Deduplicates records using exact and fuzzy matching
- Lets researchers define research aims, inclusion criteria, and exclusion criteria with weighted priorities
- Connects to local or hosted LLMs to screen article abstracts against those criteria
- Produces tagged, reasoned inclusion/exclusion decisions
- Exports results in RIS format compatible with Zotero, Mendeley, and EndNote
- Generates PRISMA 2020 flow diagrams for publication-ready reporting

All data stays on your machine in a local SQLite database — no cloud upload required.

---

## Key Features

### Data Import & Export
- Import RIS bibliography files with full metadata (title, abstract, authors, year, DOI)
- Export the included list in valid RIS format, with AI-generated tags and reasoning notes attached
- Full project export/import in JSON format for backup and collaboration

### Intelligent Deduplication
- Match duplicate records by title, publication year, and authors
- Auto-merge exact duplicates
- Flag fuzzy matches for side-by-side manual review

### Criteria-Based Screening
- Define research aims, inclusion criteria, and exclusion criteria as discrete text entries
- Assign priority levels: **Critical**, **High**, **Moderate**, **Low**, or **Optional**
- Higher-priority rules always outweigh lower-priority rules; ties favor inclusion

### AI-Powered Abstract Screening
- Configure connections to hosted LLMs (OpenAI, Google, z.ai) or local setups (llama.cpp, Ollama, LM Studio)
- AI evaluates each article's abstract in isolation against your criteria
- Returns structured JSON with inclusion/exclusion decision, reasoning paragraph, matching criteria tags, and labels
- Processes articles in background batches to respect API rate limits
- Generates an overall summary of included articles identifying trends, methodological strengths, and weaknesses

### Tag & Label Management
- AI suggests tags based on RIS metadata and user-defined criteria
- AI generates meta-tags for accepted/rejected articles
- Full manual editing — override any AI decision, adjust tags, move articles between lists

### PRISMA 2020 Flow Diagram
- Automatically generates a visual PRISMA flow diagram with exact record counts at each phase
- Exportable as an image file for manuscript submission

### Cross-Platform
- Desktop and mobile support via Tauri 2.x
- Offline-capable — all data stored locally in SQLite

---

## Workflow

Articles flow through a strict state machine. An article exists in exactly one state at any time:

```
Imported → Deduplicated → Working → AI Review → Included | Rejected
```

| State        | Description |
|--------------|-------------|
| **Imported** | Raw articles from the original RIS file |
| **Working**  | Deduplicated articles awaiting screening |
| **Included** | Articles meeting inclusion criteria |
| **Rejected** | Articles excluded based on criteria |

Users can manually override AI decisions and move articles back to the working list at any time.

---

## Tech Stack

| Layer       | Technology |
|-------------|------------|
| **Framework** | Tauri 2.x — lightweight cross-platform runtime |
| **Frontend**  | Vue 3.x with TypeScript |
| **Backend**   | Rust — memory-safe, non-blocking background processing |
| **Database**  | Local SQLite — portable, offline-first |
| **AI**        | REST API client in Rust — async requests to external or local LLM endpoints |

---

## AI Integration

Bango supports a range of LLM providers to fit different budgets and privacy requirements:

| Provider | Type | Notes |
|----------|------|-------|
| OpenAI | Hosted | API key required |
| Google | Hosted | API key required |
| z.ai | Hosted | API key required |
| llama.cpp | Local | Runs on your hardware |
| Ollama | Local | Runs on your hardware |
| LM Studio | Local | Runs on your hardware |

> **Note:** The system requires an LLM with a context window of 50,000 tokens or larger. A warning is displayed if a local provider is selected on a machine with less than 16 GB VRAM.

API keys and endpoint URLs are encrypted in local storage.

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