# openalex/

## Purpose

OpenAlex catalog search integration (§8.5 of the spec). Provides search,
Boolean Smart Search, single/batch import, DOI-library checks, and optional
reference/citation harvesting.

## Ownership

- Owns: `mod.rs` (types + `get_api_key`/`set_api_key`/`get_mailto` helpers),
  `client.rs` (HTTP client), `mapping.rs` (pure helpers), `search.rs`,
  `smart_search.rs`, `reference_harvest.rs`.
- Commands live in `commands/openalex.rs`.
- Frontend consumers: `openalex-search.vue`, `openalex-result-item.vue`,
  `openalex-detail-panel.vue`, `settings-openalex.vue`, `stores/openalex.ts`,
  `types/openalex.ts`.

## Local Contracts

### Types (`mod.rs`)

`OpenAlexWork` carries `#[serde(default)]` on `cited_by_count`, `keywords`, and
`authorships` so the struct deserializes from any API response subset - the
harvest `select` omits `cited_by_count`/`keywords`, and without the default
serde fails with "missing field" and silently drops the entire
reference/citation harvest.

### HTTP client (`client.rs`)

HTTP client with 429 retry + `mailto`/`api_key` injection + 100ms batch pause +
`download_pdf` + `fetch_citing_works` for cited_by direction. `download_pdf`
sends browser-like headers (`User-Agent`, `Accept`, `Accept-Language`,
`Referer`, `Sec-Fetch-*`) so publishers (MDPI, Elsevier, Springer) that 403 on
the minimal `Bango/2.0` UA serve the PDF instead of a block page; error
messages include the URL + HTTP status for diagnostics.

### Pure mapping helpers (`mapping.rs`)

`reconstruct_abstract`, `truncate_snippet`, `map_work_to_new_article`,
`map_works_to_new_articles`, `map_work_to_reference_paper`.
DOI normalization delegates to the canonical `ris::doi::normalize_doi`
(trim, ASCII case-insensitive prefix strip, placeholder filter, lowercase) -
the module has no DOI normalizer of its own.

### Smart Search (`smart_search.rs`)

LLM-generated Boolean query from aims + criteria via
`LlmRequestType::OpenAlexSmartSearch`.

**1500-character limit + truncation guardrail**: OpenAlex rejects `search=`
queries longer than 1500 characters without an API key. The system + user
prompts instruct the LLM that `searchQuery` MUST be 1500 chars or fewer and to
leverage OpenAlex's native stemming/synonym matching (no redundant
synonyms/stems/plurals, no single-word wildcards - wildcards only inside quoted
multi-word phrases for adjacency/proximity, nested `(...)` grouping, trimmed
redundant exclusions). A deterministic backstop enforces the ceiling regardless
of LLM drift: `MAX_SEARCH_QUERY_LEN = 1500` + `truncate_search_query`
(balance-aware, pure) is applied inside `parse_smart_search_response` so every
parsed query is guaranteed <= 1500 bytes without splitting a quoted phrase,
leaving parentheses unbalanced, or cutting mid-word. The cap is universal
(never conditional on whether a key is configured) so a query stays valid even
if the key is cleared after generation.

### Reference + Citation Harvest (`reference_harvest.rs`)

Batch-fetch both outgoing `referenced_works` and incoming `cites:` citations
when `openalex_retrieve_references` is enabled; inserts as `reference_papers` +
`article_reference_links` with `ReferenceType::Reference` /
`ReferenceType::Citation`. Harvest errors logged to the **article's** audit
trail via `log_harvest_error` helper which writes `action = "error"` with the
article_id so failures surface in the Audit Timeline, not just the generic
Diagnostics feed.

### Commands (`commands/openalex.rs`)

`search_openalex`, `import_openalex_articles` (3-phase: sync DB insert + async
ref/citation harvest + async PDF download with auto AI summary; accepts
`auto_summarize` + `include_section_summaries` params so the frontend can pass
the `bango-full-text-summaries` / `bango-section-summaries` localStorage flags -
the backend cannot read localStorage; PDF download + attach + extraction errors
are logged to the article's audit trail via `log_article_error` helper, NOT
`log_error_best_effort` which writes `article_id = NULL` and hides them from
the Audit Timeline), `check_dois_in_library`, `smart_search_openalex`,
`get_openalex_settings` / `set_openalex_settings`,
`download_and_attach_openalex_pdf`. Import reuses the existing
`insert_articles_batch` -> `classify_imported_articles` ->
`resolve_journal_links` pipeline (parity with RIS/BibTeX). No migration needed
(`'import'` already in `audit_entries.action` CHECK).

### Capabilities

`src-tauri/capabilities/default.json` allows `https://**` + `http://**` for
`opener:allow-open-url` so DOI/PDF/OA links open from the Search detail panel
(publisher domains are not predictable, so the allow-list cannot be a fixed
domain set).

## Verification

- `tests/openalex_mapping_test.rs` (11 tests incl.
  `deserialize_harvest_response_missing_fields`)
- `tests/openalex_search_test.rs` (5 tests)
- `tests/openalex_import_test.rs` (5 tests + 1 ignored Tier 2 stub)
- `tests/openalex_smart_search_test.rs` (15 tests: prompt char-limit/stemming/
  wildcard, `truncate_search_query` balance + word-boundary, parser over-long
  truncation)

## Child DOX Index

No child `AGENTS.md` files. This module owns six files (`mod.rs`, `client.rs`,
`mapping.rs`, `search.rs`, `smart_search.rs`, `reference_harvest.rs`) with no
further durable boundaries.