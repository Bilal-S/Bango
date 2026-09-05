# Bango - v5 Specification

Product specification for Bango, a desktop application for AI-assisted systematic literature review. Covers the references/citations system, BibTeX import, App Settings, Journal Index, translation pipeline, embedding-based search, and the OpenAlex/Citation Finder integrations.

---

> **Extra features**: Optional supplementary features may be documented in `docs/extra-features.md`. If that document is present, treat its contents as part of this specification; if it is absent, disregard this reference and treat this specification is complete on its own.

## 1. Product Overview

Bango is a **desktop application** for AI-assisted systematic literature review. Researchers import RIS or BibTeX bibliography files, define inclusion/exclusion criteria, and use LLMs to screen article abstracts—producing a rigorously categorized set of articles with reasoning, tags, and labels.

Built with **Tauri 2.x** and **Vue 3 + TypeScript + Tailwind CSS v4** for a lightweight, offline-capable experience. All project data is stored locally in SQLite. The app manages a **single project**—the database and all state belong to one active review. Mobile support is deferred.

---

## 2. Terminology & Data Models

### 2.1 Key Terminology

* **Tag**: Content-category label describing article topics (e.g., `"machine-learning"`). Suggested by AI and user-configurable. Names are ≤ 35 chars, lowercase, hyphenated; `inclusion:`/`exclusion:` prefixes are stripped; overlong names truncate at the last word boundary (never mid-word).
* **Label**: Workflow marker for organizational tracking (e.g., `"priority-read"`, `"disputed"`). Same naming constraints as tags. Auto-generated criterion labels (`"Inclusion: {text}"`) have the prefix stripped and remainder hyphenated.
* **Criterion**: Discrete inclusion or exclusion rule with a priority level, defined by the user.
* **Research Aim**: Statement of research objective, serving as AI screening context.
* **Citation**: An external article that *cites* a parent article (`article_reference_links.type = 0`).
* **Reference**: An external article *cited by* a parent article (`article_reference_links.type = 1`).
* **Parent Article**: An article in the library containing citation/reference details.
* **Match Status**: Connection state between a reference paper and a library article (`'unmatched'`, `'matched'`, `'imported'`, `'not_in_library'`).
* **DOI Canonical Form**: DOIs are stored trimmed, lowercased, with `https://doi.org/` / `http://doi.org/` / `https://dx.doi.org/` / `http://dx.doi.org/` / `doi:` prefixes stripped and placeholder values (`NA`, `N/A`, `NULL`, `NONE`, `-`) treated as absent. Every DOI identity decision (dedup, matching, library checks, backup restore, filename match) compares this canonical form case-insensitively.

### 2.2 Core Database Schema

The SQLite schema consists of the following primary tables. All IDs are UUID strings unless noted otherwise.

#### Core Tables
* **`research_aims`**: `id` (PK), `text`, `created_at`
* **`criteria`**: `id` (PK), `text`, `type` (`'inclusion'`, `'exclusion'`), `priority` (`'critical'`, `'high'`, `'standard'`, `'low'`, `'optional'`), `created_at`
* **`tags`**: `id` (PK), `name` (unique), `color`, `source` (`'ai_suggested'`, `'user_created'`, `'ris_keyword'`)
* **`labels`**: `id` (PK), `name` (unique), `color`, `source` (`'ai_generated'`, `'user_created'`)
* **`app_settings`**: `key` (PK), `value` (e.g., `'storage_root'`)

#### Articles & Audit Tables
* **`articles`**:
    * `id` (PK), `sequence_id` (auto-incrementing int), `status` (`'duplicate'`, `'working'`, `'included'`, `'rejected'`)
    * `title`, `abstract_text`, `authors` (comma/JSON array), `publication_year`, `doi` (nullable), `journal`, `volume`, `issue`, `start_page`, `end_page`, `keywords`, `url`, `language`, `publisher`, `publisher_city`, `publisher_address`, `issn`, `eissn`, `date`, `affiliation`, `author_address`, `accession_number`, `custom_field3`, `journal_abbreviation`, `journal_iso_abbreviation`, `web_of_science_db`, `notes`, `user_notes`
    * `duplicate_of` (FK to `articles.id`, nullable)
    * `ai_decision` (`'include'`, `'exclude'`, nullable), `ai_reasoning`, `ai_confidence` (0.0–1.0)
    * `matched_inclusion_criteria` (JSON array of UUIDs, satisfied criteria), `matched_exclusion_criteria` (JSON array of UUIDs: violated exclusion criteria plus failed inclusion criteria recorded as rejection reasons; an inclusion-type UUID here means a required criterion was not met)
    * `manual_override` (int/bool, default 0), `screening_error` (int/bool, default 0), `screened_at` (timestamp)
    * `data_length` (character count), `token_estimate` (heuristic tokens), `actual_tokens`
    * `full_text` (extracted text, nullable), `full_text_ai_summary` (nullable), `full_text_file_name` (nullable)
    * `num_cited` (nullable), `num_references` (nullable)
    * `has_citation_details` (bool, default 0), `has_figures_or_tables` (bool, default 0; computed once at full-text attach time), `has_full_text` (bool, default 0), `has_reference_details` (bool, default 0)
    * `is_translated` (bool, default 0), `translation_status` (`'none'`, `'queued'`, `'running'`, `'succeeded'`, `'failed'`; default `'none'`), `translation_error` (text, nullable), `translated_at` (timestamp, nullable)
    * `import_source` (originating filename), `imported_at`
* **`article_tags`**: `article_id` (FK), `tag_id` (FK) [PK: composite]
* **`article_labels`**: `article_id` (FK), `label_id` (FK) [PK: composite]
* **`audit_entries`**: `id` (PK), `article_id` (FK), `action` (e.g., `'import'`, `'status_change'`, `'ai_screen'`, `'translation'`, `'translation_error'`, `'reference_import'`, `'search_strategy'`, `'error'`), `from_status`, `to_status`, `details`, `source` (`'ai'`, `'user'`, `'system'`), `timestamp`

#### Translation Archive Tables
* **`article_original_content`**:
    * `article_id` (PK, FK to `articles.id`), `original_title`, `original_abstract_text`, `original_full_text`, `source_language`, `stored_at`
    * Populated once at translation time before the working `articles` row is rewritten. Preserves the original-language title, abstract, and full text.
* **`article_original_chunks`**:
    * `id` (PK), `article_id` (FK), `chunk_index` (int), `section` (text), `content` (text), `word_count` (int)
    * Holds the pre-translation chunk coordinate space. After translation the re-chunked English content lives in `article_chunks` with its own indices. The two spaces must not be compared or joined directly.

#### References & Citations Tables
* **`reference_papers`**:
    * `id` (PK), `title`, `abstract_text`, `authors` (JSON array), `publication_year`, `doi`, `journal`, `volume`, `issue`, `start_page`, `end_page`, `keywords` (JSON array), `url`, `language`, `publisher`, `publisher_address`, `publisher_city`, `issn`, `eissn`, `date`, `notes`, `reference_type`
    * `journal_index_id` (FK to `journal_index.id`, nullable)
    * `match_status` (`'unmatched'`, `'matched'`, `'imported'`, `'not_in_library'`)
    * `matched_article_id` (FK to `articles.id`, nullable)
    * `citation_count` (int, default 0), `reference_count` (int, default 0)
    * `import_source`, `created_at`, `updated_at`
* **`article_reference_links`**: `id` (PK), `parent_article_id` (FK), `reference_paper_id` (FK), `type` (int: `0` = citation, `1` = reference) [Unique: `(parent_article_id, reference_paper_id, type)`]

#### Bibliometrics Tables
* **`biblio_authors`**: `id` (PK), `display_name`, `normalized_name` (unique), `article_count`, `first_author_count`, `total_citations`, `avg_year`, `estimated_h_index`
* **`biblio_article_authors`**: `id` (PK), `article_id` (FK), `author_id` (FK), `author_order` (0-based), `raw_name`, `raw_affiliation`
* **`biblio_institutions`**: `id` (PK), `normalized_name` (unique), `city`, `country`
* **`biblio_author_affiliations`**: `id` (PK), `article_id` (FK), `author_id` (FK), `institution_id` (FK)
* **`biblio_terms`**: `id` (PK), `raw_term`, `normalized_term`, `term_type` (`'keyword'`, `'noun_phrase'`), `source` (`'metadata'`, `'ai_extracted'`, `'user_added'`), `article_count`
* **`biblio_article_terms`**: `id` (PK), `article_id` (FK), `term_id` (FK), `frequency` (int)
* **`biblio_network_meta`**: `id` (PK), `label`, `network_type` (`'co_authorship'`, `'co_occurrence'`, `'citation'`, `'biblio_coupling'`, `'co_citation'`), `node_count`, `edge_count`, `article_filter`, `params_json`, `created_at`
* **`biblio_network_nodes`**: `id` (PK), `network_id` (FK), `entity_id`, `label`, `weight`, `cluster`, `x`, `y`
* **`biblio_network_edges`**: `id` (PK), `network_id` (FK), `source_id`, `target_id`, `weight`

---

## 3. Bibliographic Import & Deduplication

### 3.1 Import Pipeline
* **Supported Formats**: **RIS** and **BibTeX** (converted internally to `RisRecord` schemas).
* **Validation Rules**: Articles missing `title`, `abstractText` (fallback to `N2` if `AB` empty), or `authors` are flagged and skipped. Collapsible, grouped error summaries in the UI.
* **Exclusion**: Users can review parsed metadata and manually deselect individual articles before confirming import.
* **Early duplicate signal (all formats)**: every preview (RIS, BibTeX, Zotero) checks the valid records' DOIs against the current library (canonical, case-insensitive) and the review step shows a "Duplicates" stat beside Valid/Skipped when any match, carrying a "Skip" checkbox (default on). When enabled, exactly those DOI-identified library duplicates are dropped before insert (reported as a skippedDuplicates count on the complete step); unchecking imports them as before. The confirm button's "Import n Articles" count subtracts exactly the duplicates Skip will drop (restoring them when unchecked), never double-subtracting manually removed rows. Everything else - within-file duplicates and every other dedup strategy - always still flows to the classify phase unchanged.
* **Capacity Guard**: Prevents import exceeding the 10,000 total article project limit.

#### 3.1.1 Zotero Import (local API)
Direct import from a locally running Zotero desktop client via its local HTTP API (`http://localhost:23119/api/`, API version 3, localhost only, read-only until Confirm).
* **Connection states**: the wizard's "Import from Zotero" button probes `GET /api/` and renders one of four states - `ok`, `not_running` (connection refused/timeout), `api_disabled` (`403`, or `404` "No endpoint found" - the connector server answering while the local API is not reachable, e.g. a startup race or the preference not yet active; both show actionable guidance naming Zotero Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero"), or `error` (status + body snippet), each with a Retry.
* **Collection picker**: the flat `/users/0/collections` list rendered as an indented tree; subcollection items are included automatically (the API's `/items` is not recursive, so subcollections are walked explicitly).
* **Validation parity**: items map to `RisRecord` through a scholarly itemType -> RIS type table; unsupported item types surface as an "Unsupported Zotero item type" group and records missing title/abstract/authors as "Missing required fields" (same strict rules as RIS/BibTeX, standard review step).
* **Tag mapping**: Zotero tags become Bango tags with source `ris_keyword` (spec 2.1 naming rules applied); `articles.keywords` stays empty - one representation.
* **Note import**: child notes (`itemType = note`) are discovered in one bulk `?itemType=note` request (with a per-item `/children` fallback), converted from note HTML to plain text, and merged into the article's editable user notes, ordered by `dateAdded` (oldest first). Each note contributes one block - its first line (the title), a `---` separator line, then the remaining body text - and blocks are joined by one blank line. The review step shows a "N with notes" count alongside the attachment count; `notes_merged_count` reports how many imported articles received notes.
* **Key-based exclusion + version guard**: review-step row removal maps to Zotero item keys (never positional); a `Last-Modified-Version` mismatch between preview and confirm aborts with nothing written.
* **Attachments**: pdf/txt child attachments are discovered in one bulk request, resolved via the file endpoint's 302 `file://` Location, and attached per-article through the standard full-text attach (direct attach, OpenAlex pattern); failures are non-fatal (per-article audit errors + counts), URL-only locations and non-pdf/txt children count as skipped, duplicates are skipped; the DB mutex is never held across the file copy (split extract/commit pipeline).

### 3.2 Multi-Strategy Deduplication
Upon import, new articles run sequentially through a prioritized strategy pipeline against the entire project database:
1. **DOI Exact**: Match `doi` case-insensitively on the canonical form (§2.1) → auto-merge as duplicate.
2. **Title + Year**: Normalised title similarity ≥ 95% (Levenshtein) AND exact `publicationYear`.
3. **Fuzzy Title + Year**: Normalised title similarity 70–94% AND exact `publicationYear` → flags for manual review.
4. **Author + Title Partial**: Exact first-author last name AND normalised title similarity ≥ 80% → flags for manual review.

* *Title Normalisation*: Trimmed lowercase, punctuation stripped (`.,;:!?'"-()[]{}`), whitespace collapsed. Normalized titles shorter than 10 chars bypass strategies 2–4.
* *Merge Behavior*: Surviving articles (highest non-null field count) enter `working` status. Duplicates enter `duplicate` status with `duplicateOf` set. Existing resolved database entries are never modified or re-screened by incoming imports.

---

## 4. Criteria, State Machine, and AI Screening

### 4.1 Criteria & Priority Conflict Resolution
* **Aims**: Discrete text items defining systematic review boundaries.
* **Criteria**: Inclusion/exclusion rules with priorities: `critical` > `high` > `standard` > `low` > `optional`.
* **Inline Editing**: Double-click any aim or criterion text to edit in place. `Enter` or click-outside commits; `Shift+Enter` inserts a newline; `Escape` cancels. Committing an empty/whitespace draft deletes the item.
* **Conflict Resolver** (applies only when no Custom Screening Instructions are present):
    1. Find the highest-priority matched inclusion criterion.
    2. Find the highest-priority matched exclusion criterion.
    3. If they differ, higher priority wins. If tied, inclusion wins.
    4. If no criteria match, the article is excluded.
* **Custom Screening Instructions governance**: When the user has authored non-empty Custom Screening Instructions (Section 4 of the Criteria screen, persisted in `app_settings.screening_custom_logic`), those combinatorial rules are the **supreme decision authority** and the generic priority resolver above is **not** applied. The LLM applies the custom rules strictly and its `decision` is recorded verbatim—the deterministic resolver cannot understand AND/OR gates, hard exclusions, or conditional inclusion, so it must not second-guess the LLM. Projects without custom rules get byte-identical behavior to the historical resolver.

### 4.2 Article State Machine

```
  Import ──(non-dup)──► Working ◄──(resolve)── Duplicate (Read-only)
                          │
                   Working ↔ Included ↔ Rejected
```

* **Duplicate**: Awaiting resolution. Read-only.
* **Working**: Deduplicated, unscreened article.
* **Included / Rejected**: Moved manually or via AI screening.
* **Screening Errors**: Malformed responses or explicit `"error"` decisions leave the article in `working` with `screeningError = true` and the raw error saved in the audit log.
* **Re-screening**: Moving an article back to `working` from any other status always resets screening flags (`screened_at = NULL`, `screening_error = 0`) so the article becomes eligible for re-screening on the next run.

### 4.3 AI Screening Process

* **Execution**: Multi-threaded async background worker with user-configured batch size (1–15), concurrency, and delay. `start_screening` accepts an optional `max_articles` cap limiting a run to `min(max_articles, unscreened_count)`.
* **Immediate Stop**: Clicking Stop cancels any in-flight LLM call within milliseconds. The response is **dropped**—no DB write, no error marking. Both stage-1 and stage-2 calls are cancellable. Inter-batch throttle delays are also cancellable.
* **Per-call timeout**: Each screening LLM call has a **2-minute wall-clock cap** (tighter than the 10-minute default for other LLM features). Transient 429/408/5xx errors are retried inside that window (3 attempts, exponential backoff). Sustained slowness should be mitigated by lowering `batch_size`; sustained rate limiting by raising `request_delay_ms`.
* **Transient-error handling**: Transient errors (429, 5xx, timeout, transport) leave articles **unscreened** (no `screening_error`, no `screened_at`) so the next run picks them up naturally. The run advances past failed batches via a sequence-id cursor. Non-transient errors (malformed JSON, parse mismatch) still mark the batch as errors.
* **Auto-stop**: Auth failures (401/403) stop the run immediately. Other transient errors stop after 3 consecutive failures or 3 total timeouts. A fatal-error banner surfaces in the UI; a non-fatal warning banner appears after the first timeout and clears on the next successful batch.
* **Deferred counter**: Transient-deferred articles are tracked in a separate `deferred` progress counter (not counted as completed or errors); the UI renders a muted "N article(s) deferred" notice.
* **Readiness Check**: Requires ≥ 1 aim, ≥ 1 inclusion, ≥ 1 exclusion criterion, valid LLM config, and worst-case per-article token estimation. Warns if any estimated footprint exceeds 80% of `contextWindowTokens` (minimum: 50,000). The footprint is mode-aware (see §4.3.1).
* **Advisory Prompts**: Batch prompt supplies aims, criteria, and articles, requesting JSON with `decision`, `reasoning`, `matched_inclusion_criteria`, `matched_exclusion_criteria`, `suggested_tags`, `extracted_terms`, and `confidence`. The deterministic resolution runs locally **unless** Custom Screening Instructions are present (§4.1), in which case the LLM's `decision` is final. Full-text evidence (Enhanced / Two-stage stage 2) is used only to verify criteria matches; the primary decision rests on the abstract.
* **Tag & Label Guidelines**: `suggested_tags` must be concise descriptors (≤ 35 chars, lowercase, hyphenated, no `inclusion:`/`exclusion:` prefixes). A backend sanitizer enforces this as defense-in-depth. The standalone `suggest_tags` / `suggest_labels` commands additionally surface curated standard taxonomies (20 study-type tags, 12 workflow-state labels), instructing the LLM to include up to 4 from each when relevant.
* **Screening-Time Abstract Translation**: When `auto_translate` is enabled (§8.1), a pre-screening step translates non-English `working` articles' title + abstract before the main loop. The progress bar shows "Translating N/M articles..." during this sub-stage. See §4.4 for the full translation pipeline.

### 4.3.1 Screening Modes

The `screening_mode` setting (`abstract` | `enhanced` | `two_stage`, default `abstract`) selects how the engine treats full text of articles that have one attached.

| Mode | Behavior | Token cost per article (worst case) |
|------|----------|--------------------------------------|
| `abstract` (default) | Screens on the abstract alone. Preserves today's behavior and cost exactly. | ~63 tokens (1x) |
| `enhanced` | Abstract plus top-K (`enhanced_top_k`, default 2) criteria-matched chunks from Methods/Results. Chunks retrieved at attach time and ranked by TF-density + Methods boost + per-article word budget. | ~320 tokens (~5x) |
| `two_stage` | Stage 1 screens abstract; only borderline articles (confidence in `[two_stage_low, two_stage_high)`, default `[0.4, 0.7)`) with full text get a second full-text-aware pass overriding stage 1. Both passes flow through the priority layer; both recorded in the audit trail (`ai_screen` / `ai_screen_enhanced`). | ~63 clear-cut, ~320 borderline (~1.5x effective) |

A per-article chunk budget (`chunk_budget_per_article`, default 2400 words) guarantees no single article can blow the context window. All three modes are always selectable in Settings regardless of attachments/articles. Enhanced/Two-stage evidence is applied per article only when `has_full_text = 1`; articles and runs without full text fall back to abstract-only screening. At screening start, previously-attached PDFs without chunks are backfilled transparently.

### 4.4 Translation Pipeline

Non-English articles are translated to English before AI workflows consume them. This is a **Plan-A permanent rewrite**: the working `articles` and `article_chunks` rows hold English text after translation; originals are preserved in `article_original_content` and `article_original_chunks`. Opt-in via `auto_translate` (default `false`, §8.1).

#### 4.4.1 Translation State Machine

```
none  ──(enqueue)──► queued  ──(worker picks up)──► running
                                                        │
                               ┌────────────────────────┼────────────────────┐
                               ▼                         ▼                    ▼
                          succeeded                  failed               (crash)
                     (is_translated=1)        (translation_error set)          │
                                                                         ▼
                                                                reenqueue_stranded
                                                                on next startup
```

- **`none`**: Initial state. Never translated.
- **`queued`**: Job sent to the in-memory worker channel.
- **`running`**: Worker is actively translating (LLM call in flight).
- **`succeeded`**: `is_translated = 1`, `translated_at` set, `translation` audit entry written.
- **`failed`**: `translation_error` set, `translation_error` audit entry written. The manual translate button accepts `failed` for retry.

#### 4.4.2 Job Kinds

Two kinds, selected automatically from the article's `has_full_text` flag:

| Kind | Translates | Trigger |
|------|-----------|---------|
| `MetadataOnly` | Title + abstract | Import (no full text), screening pre-step |
| `FullText` | Title + abstract + all chunks, then re-chunks the English result | Full-text attach, manual translate button, batch import Phase 3 |

The `language` column records the original language and is **immutable**—never overwritten by translation. `is_translated = 1` with `language = 'French'` means "originally French, now translated to English; originals in `article_original_content`." The manual translate button on the article detail header always works regardless of `auto_translate`.

#### 4.4.3 Queue Worker

A single background worker processes jobs sequentially via an mpsc channel (capacity 64):

- **Enqueue gate**: `none`/`failed` → `queued`; already-translated or in-flight → skip.
- **Batch enqueue**: single filtered `SELECT` + `UPDATE` in one transaction, then sends jobs.
- **Execution**: 3-burst lock pattern (lock to read article + mark `running` → release for LLM call → lock to write-back translation + audit) so the DB lock is never held across an `.await`.
- **Config caching**: LLM config cached; invalidated on save.
- **Events**: emits `translation:complete` per article for frontend toast feedback.
- **Dedicated connection**: the worker holds its own SQLite connection (separate from the main `DbState`) so translation never blocks UI handlers. All connections set `PRAGMA busy_timeout = 5000`.

#### 4.4.4 Crash Recovery

On app startup, stranded articles (`translation_status IN ('queued','running') AND is_translated = 0`) are **not** re-enqueued automatically (`STARTUP_STRANDED_CAP = 0`). Every stranded row is reset to `failed` with a `translation_error` audit note. The user selectively retranslates via the manual translate button. Set `STARTUP_STRANDED_CAP` to a positive `N` to re-enable bounded re-enqueueing of the first `N` stranded jobs.

#### 4.4.5 Batch Import Integration

The batch import pipeline runs in four phases: Full Text (Phase 1) → Citations (Phase 2) → **Translations (Phase 3)** → AI Summaries (Phase 4). Phase 3 is gated on `auto_translate = true`. It enqueues `FullText` jobs for newly-attached non-English articles and waits per-article for completion. Phase 4 runs after Phase 3 so AI summaries read English text.

#### 4.4.6 Multilingual Section Classification

Section classification supports 10 languages beyond English: French, Spanish, Japanese, Chinese, German, Russian, Portuguese, Italian, Arabic, and Turkish. Academic section keywords (Abstract, Introduction, Methods, Results, Discussion, Conclusion, References) are mapped per language. A Unicode-aware numbered-heading regex detects headings in non-Latin scripts.

---

## 5. References and Citations Tracking

### 5.1 N1 Citation Count Extraction
During standard article imports, the `N1` (Notes) field is parsed for structured citation statistics:
* `Total Times Cited: NN` → `num_cited`
* `Cited Reference Count: NN` → `num_references`
* Supports WoS multi-line, single-line inline, or user-note-surrounded formats. The raw notes field is preserved.

### 5.2 Reference Detail Importing
* **Manual File Import**: Users can import references (backward citations) or citations (forward cited-by) via RIS or BibTeX.
* **Automatic Parsing**: Parsed records are written to `reference_papers`; junction records in `article_reference_links` mapped to the parent article.
* **Promotion Workflow**: Unmatched reference papers with abstracts can be promoted to full articles. The app first checks for existing DB matches (DOI or title + authors + year). If found, links the reference; if not, creates a new article in `working` status.

---

## 6. Bibliometrics Data Layer

### 6.1 Normalization Engine
Users trigger `biblio_normalize` to populate analytical data structures:
* Runs in a single SQLite transaction.
* Clears stale data, extracts/deduplicates authors, parses institutions/countries, strips/identifies keywords and noun-phrases, maps relations, and determines co-authorship strengths + term frequencies.

### 6.2 Analytical Graphs & KPIs
* **Network Construction**: Builds nodes and edges for co-authorship, term co-occurrence, citation, bibliographic coupling, and co-citation networks.
* **Louvain Community Detection**: Partitions networks into clusters locally in Rust for layout visualizations.
* **KPI Metrics**: Computes average publication years, first-author frequencies, H-index approximations, and citation densities.

---

## 7. System Reference Data: Journal Index

The `journal_index` table hosts system-distributed reference metadata mapping journal titles to standard ISSN and publisher information.

### 7.1 Lifecycle Rules
* **Exclusion**: System-level data. **Never** included in project backups; survives project resets.
* **Persistence**: Bundled with the Tauri installation as a pre-populated SQLite database.
* **Updates**: Applied via migrations containing `DELETE FROM journal_index;`.
* **Startup Verification**: After migrations, if the table is empty, the app copies all records from the bundled database.

---

## 8. App Settings & Full-Text Storage

### 8.1 Configuration Settings

Application configurations are managed in the `app_settings` key-value table:

* **`storage_root`**: Bango documents root. All on-disk artifacts derive from it (`fulltext/`, `ris/`, `wiki-root/`). Defaults to `~/Documents/Bango/`. Legacy `fulltext_storage_dir` values are lazy-migrated (trailing `fulltext` segment stripped).
* **`screening_mode`**: Tier 3 screening mode (§4.3.1).
* **`enhanced_top_k`**: Criteria-matched chunks per article in Enhanced mode (default `2`).
* **`enhanced_screening_sections`**: Comma-separated section allow-list for Enhanced evidence (default `"Methods,Results"`).
* **`two_stage_low` / `two_stage_high`**: Borderline confidence band `[low, high)` triggering Two-stage's second pass (defaults `0.4` / `0.7`). User-configurable in Settings -> Screening Preferences as integer percent (defaults 40% / 70%); stored internally as `f64` fractions. The IPC commands `get_two_stage_thresholds` / `set_two_stage_thresholds` convert percent to/from the f64 band at the boundary.
* **`chunk_budget_per_article`**: Per-article word budget for evidence chunks (default `2400`, ~600 tokens).
* **`auto_translate`**: Opt-in toggle for translating non-English articles to English before AI processing (default `false`). See §4.4 for the full pipeline contract. The manual translate button works regardless of this setting. Persisted in the DB so backend stages can read it directly.
* **`project_name`**: Optional user-editable Dashboard title (up to 50 chars). Double-click the title or click the pencil icon to edit inline; empty commit reverts to "Project Dashboard" fallback. **Portable**: travels with project backups. When a backup omits it, the target's existing name is cleared (NULL) so the dashboard reverts to the fallback. Cleared by Delete All Data.
* **`screening_custom_logic`**: Optional combinatorial screening rules (AND/OR gates, hard exclusions). See §4.1 for governance contract.
* **`summary_evidence_mode`**: Project-wide evidence enrichment for literature reviews (`abstract_only` default | `with_summary_facts`).
* **`embedding_status` / `embedding_model` / `embedding_dimensions`**: Triple-state embedding capability flag (see §8.6).
* **`openalex_api_key`**: AES-256-GCM encrypted; raises rate-limit tier. Excluded from backups.
* **`openalex_mailto`**: Polite-pool email. Portable.
* **`openalex_retrieve_references`**: Reference + citation harvest toggle (default `false`). Portable.

**Portability contract**: Machine-local settings (`storage_root`, `*_needs_refresh` flags, `wiki_dir_hash`, `openalex_api_key`, `embedding_*`) are deliberately excluded from backups. Secrets are never exported.

### 8.2 Full-Text Attachments
* **Attachment**: Users can attach `.pdf` or `.txt` files to articles.
* **Text Extraction**: PDF parsed via `pdf_extract`; TXT parsed as text. Stored in `full_text` field, `has_full_text = 1`, copy saved to `{storage_root}/fulltext/`.
* **Inline Display**: PDFs rendered natively in-browser via Blob URLs.
* **AI Summary Schema (v2 superset)**: `full_text_ai_summary` holds a JSON blob. When `include_section_summaries` is enabled AND high-value sections (Methods/Results/Discussion) are detected, the LLM returns a `schema_version: 2` blob with a `section_summaries` array (typed facts: `study_design`, `sample_size`, `effect_size`, `confidence_interval`). Old blobs (no `schema_version` or `1`) lack `section_summaries` and render via existing UI.
* **Figure/Table Descriptions**: `generate_figure_descriptions(article_id)` extracts captions, sends them in one batched LLM call (grounded prompt—no visual hallucination), and merges `{number, caption, description}` objects into the summary blob under `figures`/`tables` keys. One LLM call per article keeps cost bounded.

### 8.3 Research Gap Analysis

The `analyze_research_gaps` command produces a corpus-wide Markdown gap-analysis report over included articles (Thematic Coverage, Identified Gaps, Methodological Landscape, Future Research Directions, References). Surfaced via a "Research Gap Report" button in the AI Summary view header, sharing the same gating and toolbar as "Summarize Findings."

* **Prompt substrate**: Aims + criteria + included articles (with evidence enrichment when `summary_evidence_mode = with_summary_facts`) + a `BiblioContext` block (year range, publications by year, top journals/terms/institutions).
* **Batching**: When the estimated footprint exceeds 80% of the context window, the corpus is split, each half analyzed separately, and the partial reports synthesized into one document.
* **Output**: Markdown-only; no JSON, no UUIDs; citations use the selected style's in-text form. The model is instructed to cite only articles present in the prompt.
* **Persistence**: Single-row `gap_analysis` table. **Regenerable derived artifact** (see §10.2).

### 8.4 Search Strategy Builder

The `suggest_search_strategy` command generates database-ready Boolean search strings for 8 academic databases (PubMed, Scopus, Web of Science, Cochrane Library, EBSCOhost, JSTOR, ScienceDirect, arXiv) from research aims + criteria. Surfaced as a collapsible card in the Criteria Editor.

* **Scope**: Copy-only. Builds text strings the researcher pastes into each database's search interface. Does NOT execute searches or query MeSH/EMTREE APIs.
* **Output**: JSON with PICO concept blocks (3–8 synonyms each), `{oneLine, notes}` per database, and a `warnings` array. Semantic Scholar does not support Boolean operators—a warning is emitted instead.
* **Persistence**: Session-scoped Pinia store (NOT the DB). The audit entry is the only durable record.

### 8.5 OpenAlex Search Integration

OpenAlex catalog search enables discovery and import of scholarly works (270M+) directly into the article library. Free and open; the optional `api_key` only raises the rate-limit tier.

#### 8.5.1 Search & Import
- **Search tab** in the article list status row: non-status surface, session-scoped.
- **Manual mode** (always available): free-text Boolean query with filters (year range, work type, language, OA toggle, show-retracted toggle). `has_abstract:true` always appended; `is_retracted:false` default.
- **Smart Search mode** (LLM-configured only): generates an OpenAlex-optimized Boolean query from aims + criteria. User reviews/edits before execution.
- **Results**: 200-char abstract snippets, DOI-based library check greys out existing articles, split-window detail panel.
- **Import**: Single + bulk "Add to Working" reuses the existing `insert_articles_batch` → `classify_imported_articles` → `resolve_journal_links` pipeline (parity with RIS/BibTeX).

#### 8.5.2 Reference + Citation Harvest
When `openalex_retrieve_references` is enabled (default off), import batch-fetches both citation-graph directions:
- **Outgoing references** (`referenced_works`): stored as `ReferenceType::Reference`.
- **Incoming citations** (`cites:` filter, paginated): stored as `ReferenceType::Citation`.

Both populate `reference_papers` + `article_reference_links`. A 100ms batch pause respects the free-tier rate limit (10 req/s). 429 errors logged to the article's audit trail.

#### 8.5.3 PDF Download + AI Summary
When an imported work has an OA/PDF URL, the import downloads the PDF and attaches it via the existing `attach_full_text` pipeline. If the LLM is configured, an AI summary is auto-generated. Failures (CAPTCHA, paywall, extraction error) are non-fatal and logged.

#### 8.5.4 Criteria Integration
A card in the Criteria Editor provides one-click entry: "Search OpenAlex Now" (navigates to Search tab) and "Smart Search OpenAlex" (LLM-configured only; generates + pre-loads a query).

### 8.6 Embedding-Based Semantic Article Search

Per-article, per-chunk embedding vectors powering bounded cosine-recall semantic search. **Transparent**: no settings card, no audit action, no automatic backfill on upgrade. Feedback is toast-only via the Test Connection probe.

#### 8.6.1 Capability Probe + Triple-State Flag

A triple-state flag records the provider's embedding capability: `embedding_status` (`unknown` default | `enabled` | `disabled`), `embedding_model`, `embedding_dimensions`. The probe runs during **Test Connection** (after the chat test succeeds) and on the first `generate_embeddings` call when status is `unknown`. Resolution order: (1) Anthropic → `disabled`; (2) try the provider-default embedding model; (3) on failure, retry with the configured chat model; (4) both fail → `disabled`. On success, persists model + dimensions. `save_llm_config` resets to `unknown` so a provider/endpoint/model switch re-evaluates.

#### 8.6.2 Generation

`generate_embeddings(article_ids?, status_filter?, force?)` — default corpus is `included`. For each article: a `chunk_index = -1` title+abstract row plus one row per `article_chunks` row when `has_full_text = 1`. Per-row staleness tracked by an `input_hash` (SHA-256 of the embedded text); `force` re-embeds everything. The runner dispatches per-article tasks concurrently (bounded by the orchestrator semaphore); the DB mutex is never held across an `.await`. Triggers: post-AI-summary fire-and-forget, rebuild-text-chunks cascade, batch-import Phase 5, and the standalone command.

#### 8.6.3 Recall

`recall_articles(query, top_k?, status_filter?)` embeds the query, loads all same-dimension rows matching the status filter (default `included`), max-pools cosine similarity per article, returns the top-`top_k` (default 30) `{ articleId, score }` hits. Gates on `embedding_status == enabled`; returns empty vec otherwise.

#### 8.6.4 Storage

The `article_embeddings` table is keyed on `(article_id, chunk_index)` with `-1` as the title+abstract sentinel. Vectors are little-endian `f32` streams. `ON DELETE CASCADE` on article hard-delete. **Regenerable derived artifact** (see §10.2).

### 8.7 Citation Finder

Paste-prose-to-citations matching over the user's article library, accessed as a third toggle in the Chat view alongside Articles and Wiki. Two modes: **whole-block** (one embedding, one result set) and **per-statement** (LLM splits prose into ≤5 claims; each embedded + matched independently).

**Three-layer pipeline**: embedding prefilter (reuses `recall_articles`, extended to multi-status) → token-Jaccard passage extraction → LLM classification (validating/opposing + `misrepresents_source` + cosine confidence). Candidate pool scoped by status filter (Working + Included checked by default; Duplicates excluded).

**One-button flow**: `find_citations` is the single entry point. It runs Phase A (readiness) → Phase B (auto-prepare embeddings if coverage < 100%) → Phase C (the search pipeline). No separate "Prepare Embeddings" button.

**Toggle visibility**: the readiness payload carries the raw `embeddingStatus` triple-state + `embeddingModel`. The toggle is clickable when `'enabled'` or `'unknown'`; **visible-but-disabled** when `'disabled'` (known-unsupported provider—amber banner + "Open Settings" link); `'hidden'` only when readiness hasn't loaded or LLM isn't configured. Reacts live to Settings provider switches.

**Model-mismatch detection**: before each submit, the frontend checks whether stored embeddings were generated with a different model than the current `embedding_model` setting. If so, a confirmation dialog offers: **Regenerate** (scoped delete + re-embed), **Continue anyway** (partial recall), or **Cancel**. Fires once per `storedModel` key per session. The backend's staleness check also flags stale-model rows so Phase B regenerates them on the first run.

**Coverage / first-run notice**: when `coveragePct < 100` and articles are in scope, a notice reads "First run will prepare embeddings for N article(s)—this may take several minutes."

**Cancel + background**: `find_citations` spawns a background task emitting `citation:progress` / `citation:done` / `citation:error` events. One Cancel button covers both Phase B + Phase C.

**Citation style**: reuses the existing 5-style LLM-hint list (APA/MLA/Chicago/IEEE/AMA). The active style is captured at submit time and frozen per-bubble so each bubble renders all its cards with the style selected when the search ran. No `@citation-js` dependency; the Copy button builds plain-text citations via a pure TS helper.

**Data contract**: `misrepresents_source` (`true` = passage taken out of context), `confidence` = cosine normalized from `[-1, 1]` to `[0, 1]`, `section_origin: Option<String>` (None omits the `§…` badge). `ChatMessage` extends with `citations?: CitationResult[]` + `citationStyle?: CitationStyle`.

Commands: `find_citations`, `cancel_citation_search`, `get_citation_finder_readiness`.

### 8.8 Cluster Thematic Analysis

The `biblio_analyze_cluster_themes` command asks the LLM to explain, for one selected Louvain cluster, what its members share, grounded in article titles, author lists, keywords, and abstracts. In scope: the co-authorship (`co_authorship`) and keyword co-occurrence (`co_occurrence`) networks; the citation, co-citation, and bibliographic-coupling variants are rejected with a validation error (extension seams prepared).

* **Trigger**: an "Analyze" button in the `Clusters` legend heading row (between the heading text and the clear-filter icon, matched to its height), visible only when exactly one cluster is selected and `useLlmConfigured()` is true, disabled with a spinner while the selected cluster's analysis is in flight.
* **Member resolution**: the frontend sends the cluster's member entities (`{ id, label }`). Co-authorship member ids are `biblio_authors.id` UUIDs (resolved via `biblio_article_authors`); keyword member ids are `normalize_term(raw_term)` strings resolved by mirroring the keyword network's three-source term collection (`biblio_article_terms` + `article_tags` + `article_labels`), all scoped to included articles and deduped by article id.
* **Prompt size**: single call with a Top-N cap - the 40 most representative articles ranked by citation count (NULLs last) then recency, member list capped at 100, author lists truncated at 300 chars, keyword lists at 200 chars (empty values omit the line), abstracts at 1200 chars, each on a word boundary; the report opens `## Overview` with an italic disclosure line naming the exact capped/total counts whenever truncation occurred.
* **Link protocols**: the LLM may reference only stable ids present in the prompt, wrapped as `[Article Title](article:{id})` and (co-authorship only) `[Author Name](author:{id})`. The panel renders these as clickable spans (author -> focus + locate in the graph; article -> full in-view article detail slide-over, so closing it returns to the exact network state), escapes raw HTML, and renders every other link as plain text.
* **Cache**: session-only Pinia store keyed by `networkType:clusterIndex`, invalidated on every layout/recalculate (Louvain indices are unstable across runs); no persistence and no migration. Analyzing an already-analyzed cluster redisplays the cached markdown without a new LLM call; a duplicate click while the same cluster is in flight is skipped, and an errored entry retries. The panel's re-analyze deletes the entry first and forces a fresh call. Layout-mode switches (fixed <-> dynamic) are positioning-only relayouts: cluster assignments and cached analyses are preserved, and only the explicit Recalculate path re-clusters and invalidates. Client-side visibility filters (min-papers/search) do not invalidate the cache - re-analyze refreshes membership.
* **Output**: Markdown with `# Cluster N - Thematic Analysis`, `## Overview`, `## Main Themes`, `## Representative Articles`; no em dashes; LLM failures surface per-cluster in the panel plus a system diagnostics audit entry.

---

## 9. UI Layout & Design System

The application uses a **"Scholarly Precision"** style: dense, minimalist Notion-like aesthetic.

### 9.1 Theme Configuration (Tailwind CSS v4)
* **Dual Styling**: Scoped custom CSS variables mapped to `--color-*` tokens alongside Tailwind v4 utilities via the `@theme` directive.
* **Reset Suppression**: Tailwind's Preflight is **disabled** (`preflight(false)`) for legacy CSS compatibility.
* **Typography**: Inter fonts + Google Material Symbols Outlined icons. No unicode icon fallbacks.

### 9.2 Key UI Layouts
* **Project Dashboard**: Status KPI cards, active logs, progress stats. The `<h1>` is an **editable project title** (§8.1 `project_name`): double-click or click the pencil icon; empty commit reverts to fallback. A "Start New Project" link opens an informational dialog explaining the single-project export → delete → begin-fresh workflow.
* **Master-Detail Article Viewer**: 3-pane layout with navigation sidebars, filterable/sortable tables, and sliding article detail panels (metadata, AI screening cards, audit timelines).
  The filter panel offers metadata fields (title, author, year, journal, DOI incl. an "Only no DOI" mode), tag/label pills with NOT-negation, and a **Match Criteria** picker (pills + combobox, placed on the same row as DOI) filtering by the criterion UUIDs stored in the article's matched-criteria arrays.
  While any filter is active, the list also fetches the true match total (backend `count_query_articles`, same filters as the list query) so the result-count notice and the pager cover the entire match set - not just the current page-size-capped page.
  Criterion pills show the global criterion number plus the criterion text capped at 20 characters with an ellipsis; three sentinel entries sit at the end of the picker list and render as removable pills (with an "x") when active: `X. No Exclusion Criteria` (matched exclusion array `NULL` or `'[]'`, inclusion array irrelevant - on the Rejected tab this reproduces the PRISMA "Records generally excluded" count exactly), `Y. Unknown Criteria` (articles referencing since-deleted criteria), and `Z. No Criteria` (articles whose matched arrays are both `NULL` or `'[]'`). `Z` is mutually exclusive with the other selections; `X` may AND-combine with specific criteria and `Y`.
* **PRISMA 2020 Flow Diagram**: Four-phase SVG diagram (Identification, Screening, Eligibility, Included), exported through the `Export Diagram` dropdown button (menu items: `Export to PNG`, `Export SVG`).
  The `Export Data` button opens the shared export dialog (§10.2).
  The `Export Report` dropdown exports the Screening Reasons Report (§10.2) as Markdown or PDF.

---

## 10. Audit, Export, and Security

### 10.1 Audit Trail
* **Article-specific Audits**: Every article-related action (import, status change, tag/label edits, AI screening, override, screening errors, metadata edits, note edits, AI reasoning clears) generates an immutable, timestamped `AuditEntry` linked to the target article. Rapid same-type edits from the same source coalesce within a 5-minute window. Visible in article detail panels.
* **System/Generic Audits**: System-wide operations or errors unrelated to a single article are recorded with `article_id = NULL` and `action = 'error'`. Accessible via the Diagnostics screen.
* **CHECK constraint migrations**: Adding a new `audit_entries.action` value requires a rename-create-copy-drop migration (SQLite CHECK constraints can't be ALTERed). The base migration is updated so fresh DBs include all current action values directly.

### 10.2 Export and Security

**Regenerable derived artifacts**: Several tables are dynamically generated and share the same backup/reset contract—**excluded from `ProjectBackup`**, wiped by `reset_project` (in `DROP_TABLES`), and explicitly purged during import (because FK cascade is off during import). These include: `article_chunks`, `article_embeddings`, `gap_analysis`, `summary`, and all `biblio_*` tables.

* **Shared Export Dialog**: Opened from the article-list toolbar `Export` button and the PRISMA view's `Export Data` button.
  Options appear in a fixed order - RIS, Zotero, Project Backup, Wiki Website - all rendered with the same secondary button style.
* **RIS Export**: Exports the Included list, mapping AI tags to `KW`, notes to `NO`, labels to `C8` JSON groupings.
* **PRISMA Screening Reasons Report**: Markdown report (four tables, each with explanatory text) generated from the PRISMA view's `Export Report` dropdown, exported as `Markdown` (save dialog) or `PDF` (rendered to HTML and printed via the webview, where the user chooses "Save as PDF"). Tables 1/2 count each included/rejected article exactly once under its single most significant reason: the highest-priority matched criterion (critical > high > standard > low > optional), ties broken by first-assigned order (earliest UUID in the article's matched-criteria array); articles with no resolvable matched criterion fall into a "General" row so the totals match the included/rejected counts. Tables 3/4 are multi-assignment counts (one row per matched criterion per article, no totals because sums intentionally exceed the article counts). Criterion text is escaped for Markdown table cells; deleted criteria surface as "Deleted criterion" rows in the multi-assignment tables and never win primary attribution. Failed inclusion criteria stored in the matched-exclusion array (the rejection reason for gate-based exclusions) surface as `NOT MET: {text}` rows in Tables 2/4. When a custom project name is set (Dashboard title), the report opens with `# {Project Name}` above an h2 report title and section headings are demoted one level.
* **Project Backup**: Single `.bango.json` containing aims, criteria, articles, tags, labels, audit logs, reference papers/links, and a curated portable subset of `app_settings` (§8.1). Secrets are never exported.
* **Credential Security**: LLM API keys encrypted locally using AES-256-GCM (key derived via PBKDF2 from machine hostname, username, and app salt).
* **Zotero Export (local write API, Zotero 10+)**: The scope-aware "Export Articles (Zotero)" button in the shared export dialog ("Export Included Articles (Zotero)" in the PRISMA/default context, "Export {Tab} Articles (Zotero)" on article-list tabs; hidden when the tab has 0 articles) opens a panel that syncs the scoped articles into a chosen Zotero collection - metadata always, full-text files (`.pdf`/`.txt`) best effort behind an "Include full-text files" checkbox; when the run completes, the panel's action button renames to Close and dismisses the export dialog. Only articles missing from the target collection are exported, matched by canonical DOI (case-insensitive; articles without a DOI are skipped and counted). **Dates are normalized to the most specific ISO form Zotero parses exactly** (`YYYY-MM-DD`, `YYYY-MM`, `YYYY`): month/day are extracted tolerantly from the raw stored date string (`NOV 25`, `APR`, `JUL-AUG` -> first month, `02/2017`, ISO, `April 1957`, ...) and combined with the authoritative `publication_year`; raw strings are never sent because Zotero re-parses them (a 2025 article with raw date `NOV 25` displayed as "Nov 25"). **User notes export as Zotero child-note items**: after the article batches, each created article's user notes are split back into `Title` / `---` / body blocks (the merged import format; free-form text becomes one note) and POSTed as `itemType: note` children in batches of 50 with HTML-escaped, `<br/>`-joined text; failures are non-fatal (system audit errors + `noteExportedCount`/`noteFailedCount` on the result card). The collection dropdown defaults to the collection currently selected in the Zotero UI (connector `getSelectedCollection`, exact-name correlation with ambiguity falling through), then the last collection Bango exported to. Writes go through the local write API only: every request echoes `Zotero-Server-ID`; new items POST in batches of 50 with a fresh `Zotero-Write-Token` per batch (server-assigned keys from the envelope - the local API rejects locally generated keys with `428`); files use the 3-phase upload (md5/mtime/size + `If-None-Match: *` -> bytes -> register); attachment items carry a Title and upload filename of `{Lastname} - {up to 30 title characters, cut at a word boundary}.{ext}` (single-token/institutional first authors use the whole name; authorless articles use the title alone; empty titles fall back to `Untitled`). Write authorization happens at most once per error: the granted key is persisted encrypted (`zotero_api_key`) and reused silently; Bango re-authorizes only when the key is missing, expired (`401 Invalid or expired API key` - the user skipped "Remember", and the run aborts with "tick Remember" guidance while the stale key is cleared), or the live server id differs. Zotero < 10 gates the panel with "requires Zotero 10 or newer" (import still works). The user docs and every communication-error dialog explain how to enable the local API (Zotero Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero").
* **Wiki Static Site Export**: Packages the LLM Wiki as a self-contained static website (HTML + CSS + JS + original Markdown) in a `.zip` via a native save dialog. Article references resolve to synthesis pages or metadata-only stub pages (no full text—copyright safe). DB article full text/PDFs are never included. The frontend renders all HTML and owns the save dialog; the backend handles file I/O + zip.
* **Start-New-Project workflow**: Because Bango is single-project, starting a new review is a guided back-up → delete → begin-fresh flow (Settings → Project Management or Dashboard header link). The Help Guide documents three valid entry paths (Aims-first, Articles-first, Search-via-OpenAlex) that converge on the screening/review pipeline.

### 10.3 App Diagnostics & E2E Testing
* **Tauri Pilot Integration**: The application includes E2E testing hooks via `tauri-pilot` for automated UI inspection, console log retrieval, and action replay. Agents can use the `tauri-pilot` MCP server to inspect views, capture screenshots, and extract runtime logs during diagnostic sessions.

---

## 11. Scope Exclusions

**Still out of scope**: mobile-native builds/layouts; multi-project workspaces; multi-user collaboration or blind review synchronization; naive whole-paper screening; direct PubMed/external API integrations beyond OpenAlex; hosting/deployment integration; interactive PDF text highlighting/annotation; global undo/redo action stacks.

---

## Change History

See git history for per-release changes.