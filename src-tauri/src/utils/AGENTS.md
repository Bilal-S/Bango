# utils/

## Purpose

Pure helper utilities shared across the backend: section-aware text
classification, semantic chunking, the Tier 3 shared tokenizer, the LLM JSON
pre-parser, and PDF text extraction (incl. legacy-CJK mojibake recovery).

## Ownership

- Owns: `sections.rs`, `chunking.rs`, `text_tokens.rs`, `json_repair.rs`,
  `pdf_extract.rs`, `mod.rs`.
- Consumed by: `wiki::fts` + `wiki::ingest` (sections/chunking),
  `screening::chunk_retrieval` + `screening::engine` (text_tokens/chunking),
  `commands::full_text` (sections/chunking/pdf_extract),
  `commands::summary` (sections), `llm::orchestrator::send_json`
  (json_repair), and `wiki::raw_export` (sections/chunking).

## Local Contracts

### `sections.rs` - section-aware text classification (T1.1 + Tier 2 Phase 1)

`classify_sections(text)` splits flat extracted text into `Vec<Section>` by
detecting heading lines (markdown `##`, numbered `2.1 Study Design`, bare
keyword `METHODS`). `SectionKind` enum: `Heading, Abstract, Introduction,
Methods, Results, Discussion, Conclusion, References, Table, Figure, Text`
(Table/Figure added in T2 Phase 1 for caption/table extraction).
`SectionKind::label()` returns the stable display string. Tier 2 Phase 1
added: `extract_captions(text)` (multi-line Figure/Table caption extraction
via `CAPTION_START_RE` with greedy continuation), `detect_markdown_tables(text)`
(pipe + whitespace-aligned table detection returning GFM tables +
`<!-- TABLE:N -->` placeholders), `extract_sections_with_tables(text)`
(composer that keeps `classify_sections` untouched). Constants:
`COLUMN_ALIGN_TOLERANCE=2`, `MIN_TABLE_LINES=2`. `extract_sections(path)` is
the I/O wrapper. Pure functions (`#[must_use]`); consumed by T1.2
`chunking.rs`, T1.3 `summary::prompt`, T2.4 `raw_export::structure_full_text`,
and T3.1 `attach_full_text` chunk storage. Tier 2 proptest invariants live in
`src-tauri/tests/sections_test.rs` (page-spanning break) +
`src-tauri/tests/chunking_test.rs` (word-count bounds + contiguous index).

### `chunking.rs` - semantic chunking (T1.2 + Tier 2 Phase 1)

`chunk_sections(sections, target_words)` walks `Section`s and emits
`Vec<Chunk>` bounded by `DEFAULT_CHUNK_WORDS=512` / `MIN_CHUNK_WORDS=100` /
`MAX_CHUNK_WORDS=1200`. Splits long sections at sentence boundaries; merges
tiny tails (now MAX-guarded so a near-MAX chunk + tiny tail cannot exceed the
hard cap); skips `References` entirely; carries section provenance
(`Some("Methods")`) so FTS5 chunk rows + chat citations can render
`(§Methods)`. **Atomic Table/Figure arm** (T2 Phase 1): `SectionKind::Table` /
`Figure` sections are emitted as a single chunk regardless of
`MAX_CHUNK_WORDS` so GFM tables survive intact into the FTS index. Pure
functions (`#[must_use]`). Consumed by `wiki::fts` (chunk-emission) and T3.1
`attach_full_text` chunk storage. Property-based tests (`proptest`) in
`src-tauri/tests/chunking_test.rs` verify the word-count bound (excluding
atomic Table/Figure) + contiguous `chunk_index` for any input.

### `text_tokens.rs`

Tier 3 shared tokenizer for FTS5 BM25 + screening chunk scoring. Shared
between `screening::chunk_retrieval` (criteria-token TF density) and the FTS5
index. Do not introduce a second tokenizer for these consumers.

### `pdf_extract.rs`

PDF text extraction incl. **legacy-CJK mojibake recovery** via `encoding_rs` +
`chardetng`: when `unpdf` returns raw Shift-JIS/EUC-JP/CP949/GB18030 bytes as
Latin-1 code points - the common failure mode for CJK PDFs whose fonts lack a
ToUnicode CMap - the `recover_mojibake` pass detects the C1 control-char
signature and re-decodes the bytes to correct Unicode before header/footer
stripping. Tested in `tests/pdf_mojibake_test.rs`.

### `json_repair.rs` - LLM JSON pre-parser

The fix for the recurring "AI summary failed: Invalid JSON response from LLM:
control character (\u0000-\u001F) found while parsing a string" error. LLMs
occasionally place raw control chars (most commonly `0x0A` newlines, also tabs
/ form-feeds / NULs) inside JSON string values instead of escaping them as
`\n` / `\t`. The outer OpenAI envelope is well-formed, so `llm::client` decodes
`choices[0].message.content` into a Rust `String`; but the inner summary/schema
JSON, when re-parsed by `serde_json::from_str`, fails because RFC 8259 forbids
literal control bytes inside JSON string literals.

Three exported helpers:

1. `escape_control_chars_in_json(raw)` (pure, `#[must_use]`) walks the document
   tracking `in_string` + `escape_next` (same pattern as `balance_braces`), and
   re-emits any raw char in `0x00..=0x1F` found *inside* a string literal as
   its JSON escape (`\n`, `\t`, `\r`, `\b`, `\f`, or `\u00XX` for the rest).
   Structural whitespace between tokens is preserved unchanged. **Why escape,
   not strip**: stripping would collapse paragraph breaks in
   `summary_150_250_words`, run together multi-line `reasoning`, and damage
   `key_insights` bullets - silent content corruption in user-facing text.
   Idempotent: a clean JSON document passes through byte-identical.
2. `strip_code_fences(raw)` strips ```` ```json ```` / ```` ``` ```` fences;
   moved here from `summary/prompt.rs` and re-exported from there for backward
   compat.
3. `prepare_llm_json(raw)` chains `strip_code_fences` +
   `escape_control_chars_in_json` in the correct order (fence-strip MUST run
   first).

`prepare_llm_json` is the single pre-parser used by
**`LlmOrchestrator::send_json`** (see `llm/AGENTS.md`) - the recommended entry
point for any caller that feeds the LLM response into `serde_json::from_str`.
`send_json` is a thin wrapper over `send` that runs `prepare_llm_json` on the
result; callers that expect Markdown / plain text (chat, wiki chat, literature
review, wiki ingest, markdown-fallback retry) MUST use `send` instead (the
JSON pre-parser would corrupt quoted spans in prose). The 4 LLM system prompts
(`ai_article_summary_prompt.md`, `ai_article_summary_with_sections_prompt.md`,
`figure_description_prompt.md`, `screening/prompt.rs::SYSTEM_PROMPT`) carry a
one-line defense-in-depth note instructing the model to escape line breaks as
`\n` inside JSON string values. JSON-returning call sites migrated to
`send_json`: `commands/summary.rs` (article summary, monolithic fallback,
section-aware, synthesis, figure descriptions ×2), `commands/openalex.rs`
(smart search), `commands/criteria.rs` (criteria generation). The screening
path keeps its sanitizer as the first step inside
`screening::engine::extract_json` (idempotent; screening-specific shape repair
follows).

## Work Guidance

- All functions in this module are pure (`#[must_use]` where applicable). Do
  not introduce I/O or DB dependencies here.
- The tokenizer in `text_tokens.rs` is the shared Tier 3 tokenizer - do not
  duplicate per-consumer.
- The fence-strip MUST run before control-char escape in `prepare_llm_json`.

## Verification

- `tests/sections_test.rs` (classify_sections + 3 real-PDF e2e + proptest)
- `tests/chunking_test.rs` (9 inline + standalone Tier 2 + proptest)
- `tests/pdf_mojibake_test.rs`
- `tests/json_repair_test.rs` (5 integration tests incl. the exact bug-report
  payload, the screening `reasoning`-with-newline regression, the
  no-op-for-valid-JSON guard, and the `prepare_llm_json` chain + no-op) + 10
  inline tests in `json_repair.rs`.

## Child DOX Index

No child `AGENTS.md` files. This module owns five pure-helper files with no
further durable boundaries.