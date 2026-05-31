# Security Audit Report - Bango

**Scope:** Full codebase (Rust backend, TypeScript/Vue frontend, database, LLM client, RIS parser)
**Date:** 2026-05-10
**Method:** Manual code review across all source files. No code changes made.

---

## Critical / High Severity

### H1. SQL Injection via `sort_dir` parameter
- **File:** `src-tauri/src/db/article_repo.rs:485-496`
- **Details:** `sort_dir` is a user-supplied string interpolated directly into SQL with zero validation. While `sort_by` is whitelisted via a match statement, `sort_dir` is not:
  ```rust
  let sort_dir = query.sort_dir.as_deref().unwrap_or("DESC");
  // ...
  "title" => format!(" ORDER BY title COLLATE NOCASE {sort_dir}"),
  ```
  A malicious frontend call could inject arbitrary SQL (e.g., `DESC; DROP TABLE articles; --`).

### H2. Decrypted API key sent to frontend via IPC
- **Files:** `src-tauri/src/db/llm_config_repo.rs:50-53`, `src-tauri/src/commands/llm_config.rs:11-17`, `src/stores/llm-config.ts:14`
- **Details:** The `get_llm_config` Tauri command decrypts the API key and returns the full `LlmConfig` (including plaintext key) to the WebView. The struct also derives `Debug`, meaning any debug logging would print the key. The plaintext key lives in the Pinia store and is accessible via Vue DevTools.

### H3. Content Security Policy disabled
- **File:** `src-tauri/tauri.conf.json:23-25`
- **Details:** `"csp": null` disables all CSP protections in the WebView. Combined with Google Fonts loaded from CDN without SRI hashes (`index.html:7-14`), this provides no defense against script injection from compromised external resources.

### H4. Path traversal in file import/export commands
- **Files:**
  - `src-tauri/src/commands/import.rs:67-69` (read)
  - `src-tauri/src/commands/import.rs:155-157` (read)
  - `src-tauri/src/commands/export_cmd.rs:63-66` (write)
  - `src-tauri/src/commands/export_cmd.rs:82-85` (write)
  - `src-tauri/src/commands/prisma.rs:37` (write)
  - `src-tauri/src/commands/prisma.rs:77` (write)
- **Details:** User-supplied path strings are passed directly to `std::fs::read_to_string` and `std::fs::write` with no canonicalization, sandboxing, or directory restriction. The app can read/write arbitrary files on the filesystem (e.g., `../../../etc/shadow`).

### H5. LLM responses logged to stderr in production
- **File:** `src-tauri/src/screening/engine.rs:491-526`
- **Details:** The first 300-500 characters of every LLM response are printed via `eprintln!` in production code (not gated behind `#[cfg(debug_assertions)]`). This could contain PII from article abstracts, author names, and institutional affiliations. Capturable by system log tools or debuggers.

---

## Medium Severity

### M1. Weak API key encryption scheme
- **File:** `src-tauri/src/crypto/aes_gcm.rs:10`
- **Details:** PBKDF2 uses a hardcoded static salt (`b"bango-app-salt16"`) and derives the encryption key from `hostname:username` - both are easily guessable. Anyone who knows the target's hostname can reconstruct the AES-256-GCM key and decrypt stored API keys from the SQLite DB. The 600k PBKDF2 iterations are strong, but the key material is not secret.

### M2. No HTTPS enforcement on custom LLM endpoints
- **File:** `src-tauri/src/llm/client.rs:195-477`
- **Details:** `list_models` and `send_chat_completion` accept arbitrary `endpoint_url` values without validating the URL scheme. A user-configured `http://` endpoint transmits the API key as a plaintext bearer token.

### M3. Prompt injection via article content
- **File:** `src-tauri/src/screening/prompt.rs:140-155`
- **Details:** Article title, authors, and abstract text from imported RIS files are inserted directly into LLM prompts. The `escape_json_str` function only handles basic JSON escaping - it doesn't mitigate prompt injection. A maliciously crafted RIS file could manipulate screening decisions.

### M4. No cumulative token budget for screening runs
- **File:** `src-tauri/src/screening/engine.rs`
- **Details:** Per-article token counts are recorded but no running total is checked against a maximum. Large screening runs consume tokens proportionally with no cap - a cost control risk for expensive API providers.

### M5. API error response bodies leaked to frontend
- **File:** `src-tauri/src/llm/client.rs:362-363, 442-443`
- **Details:** Full HTTP response bodies from failed LLM API calls are included in error messages returned to the frontend. These can contain API key prefixes, organization IDs, and internal error details.

### M6. No transaction in `classify_imported_articles`
- **File:** `src-tauri/src/commands/dedup.rs:34-121`
- **Details:** Multiple database writes (mark_as_duplicate, move_articles, audit inserts) are performed without a transaction. A crash mid-classification leaves articles in an inconsistent state. Contrast with `merge_exact_duplicates` in the same file which correctly uses `unchecked_transaction()`.

### M7. No input size limit before RIS parsing
- **File:** `src-tauri/src/ris/parser.rs:6-8`, `src-tauri/src/commands/import.rs:64-74`
- **Details:** `parse_ris` accepts a `&str` with no size check. A multi-GB file is read entirely into memory and parsed before the 10,000 article count limit (enforced at insert time) is checked.

### M8. Frontend IPC data trusted without runtime validation
- **Files:** All stores and composables
- **Details:** Data from Rust backend via `tauriCommand<T>()` is directly assigned to reactive state. TypeScript generics provide compile-time assertions only - no runtime validation (e.g., Zod) protects against version skew or corrupted DB state.

### M9. LLM decision values not strictly validated
- **File:** `src-tauri/src/screening/engine.rs:314-432`
- **Details:** The `LlmScreeningResponse` struct allows any string for `decision`, `reasoning`, and `suggested_tags`. Prompt-injected content could produce unexpected decision values stored in the database and audit trail.

---

## Low Severity

| # | Finding | File(s) |
|---|---------|---------|
| L1 | `derive_key_from_password` exposed but unused (dead code) | `src-tauri/src/crypto/aes_gcm.rs:34-38` |
| L2 | No key rotation or expiry mechanism | `src-tauri/src/db/llm_config_repo.rs` |
| L3 | String-based error detection for 429 retries (`e.to_string().contains("429")`) | `src-tauri/src/screening/engine.rs:245` |
| L4 | LLM-generated tags stored without length limits | `src-tauri/src/screening/engine.rs:414-422` |
| L5 | API key sent over Tauri IPC (internal protocol, not encrypted) | `src/composables/use-llm-config.ts:99-108` |
| L6 | External Google Fonts without SRI hashes | `index.html:7-14` |
| L7 | `ImportResult.articles` typed as `unknown[]` | `src/composables/use-import.ts:37` |
| L8 | Minimal client-side input length validation | Various views |
| L9 | LLM audit reasoning sent back to LLM in summary feature | `src-tauri/src/summary/prompt.rs:37-49` |

---

## Clean Areas

| Area | Status |
|------|--------|
| XSS protection | **Clean** - zero `v-html`/`innerHTML` usage, all rendering via Vue text interpolation |
| Code injection | **Clean** - zero `eval()`/`new Function()` usage |
| TypeScript `any` types | **Clean** - zero instances, enforced by ESLint |
| `unwrap()`/`expect()` in non-test code | **Clean** - none found |
| `unsafe {}` blocks | **Clean** - none found |
| npm dependency vulnerabilities | **Clean** - 0 vulnerabilities across 376 packages |
| Console logging of secrets | **Clean** - only one `console.error` for non-sensitive data |
| API keys in localStorage | **Clean** - only UI preferences stored |
| API keys in exports/backups | **Clean** - explicitly excluded |
| Data minimization for LLM | **Clean** - only title/authors/year/abstract sent, not addresses/emails/URLs |
| TLS certificate validation | **Clean** - reqwest uses default TLS with no bypass flags |
| Parameterized SQL (general) | **Clean** - all other queries use parameterized statements |
| `serde_json::Value` without validation | **Clean** - all deserialization uses typed structs |

---

## Recommended Remediation Priority

1. **H1** - Whitelist-validate `sort_dir` to `"ASC"` or `"DESC"` (one-line fix)
2. **H2** - Restructure `get_llm_config` to return the key as masked/redacted to the frontend
3. **H4** - Use Tauri's scoped filesystem API instead of raw `std::fs` calls, or at minimum canonicalize and restrict paths
4. **H3** - Enable CSP in `tauri.conf.json` (may require adjusting inline styles)
5. **H5** - Gate `eprintln!` behind `#[cfg(debug_assertions)]` or a proper logging framework
6. **M1** - Generate a random per-installation secret on first launch; use per-encryption random salts
7. **M2** - Validate URL scheme; reject `http://` for non-localhost endpoints
8. **M6** - Wrap `classify_imported_articles` in a transaction
9. **M7** - Add file size check before reading/parsing RIS files
10. **M3** - Add prompt boundaries (XML tags) and length limits for article fields
