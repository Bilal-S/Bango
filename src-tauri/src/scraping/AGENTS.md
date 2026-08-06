# scraping/

## Purpose

Citation Chaser headless-Chrome scraper. Given a DOI, drives
`https://estech.shinyapps.io/citationchaser/?dois=<doi>` to download RIS files of
the article's references and/or citations into `{storage_root}/ris/` as
`{clean_doi}_references.ris` / `{clean_doi}_citations.ris`.

## Ownership

- Owns: `citation_chaser.rs` (scrape driver + pure helpers), `browser.rs`
  (headless-Chrome session management).
- Consumed by `commands/scraping.rs::scrape_citation_chaser_cmd` (async,
  `spawn_blocking`) which stores the cancel token in the managed `ScrapingState`.
- Frontend consumer: `composables/use-references.ts` (batch scrape orchestration
  + cancel).

## Local Contracts

Three coupled contracts (design + validated signals in
`.worktrees/scrapefix2.md`):

### (1) Cancellation

Every poll loop (`click_xpath_with_retry`, `wait_for_element`,
`wait_for_download_enabled`, the empty-result poll, and the pre/post-download
hooks) routes its `thread::sleep` through `sleep_or_cancel(cancel, dur)`, which
checks the `CancelToken` (`Arc<AtomicBool>`) before and after sleeping. The
Tauri command layer (`commands/scraping.rs::scrape_citation_chaser_cmd`) creates
a fresh token per call, stores it in the managed `ScrapingState` for the
duration of the `spawn_blocking` scrape, and clears it on return. The
`cancel_scraping` command signals the active token; the in-flight scrape returns
`ScrapeError::Cancelled` within one `POLL_INTERVAL_MS` (1s) tick, closes the
browser, and removes any partial RIS so the existence-shortcut does not cache a
truncated file. Matches the screening v8.3–v8.5 cancel philosophy but uses
Option 1 (AtomicBool on the blocking pool) because `headless_chrome` is a
synchronous API and the scrape genuinely belongs on `spawn_blocking`. The
frontend (`composables/use-references.ts`) `cancelBatchScraping()` sets the
between-articles `batchCancelled` flag AND fire-and-forgets `cancel_scraping`
so the current in-flight article aborts promptly instead of waiting up to 120s.

### (2) Empty-result detection

A Shiny session that resolves to "0 references" or "no recorded citations"
either disconnects the websocket (references: clicking Search on a 0-ref DOI
tears down the session within ~8s, leaving `#refs_ris` disabled forever) or
serves a 0-byte RIS file with a valid session href (citations: `#cits_ris`
becomes enabled but `fetch(href)` returns HTTP 200 / 0 bytes). The post-Search
`detect_empty_or_disconnect(body_text, kind)` poll (pure `#[must_use]`) watches
`document.body.innerText` for three stable signatures - `had 0 references`
(refs), `no recorded citations in the Lens.org` (cits), `Disconnected from the
server` (either) - and returns `ScrapeError::NoData(reason)` within
`EMPTY_RESULT_TIMEOUT_SECS = 20s` instead of the old 120s
`wait_for_download_enabled` hang. The `validate_ris_nonempty(path)` guard is
defense-in-depth after the download: a 0-byte file or one lacking `TY  -` is
removed and also returns `NoData`, so the existence-shortcut never caches an
empty file. `NoData` and `Cancelled` are routed by the frontend as **skips**
(info toast + `skipped` counter), not errors; the backend's `is_skip_message`
and the frontend's `isScrapeSkipMessage` mirror each other (prefix `"No data:"`
or exact `"Cancelled"`).

### (3) Robust download

`download_file` tries `download_with_reqwest` first (`reqwest::blocking`,
browser-like headers, 10s connect / 30s overall timeout, 5-redirect policy; safe
because the scrape runs on `spawn_blocking`, NOT a tokio async worker) and falls
back to `download_with_curl` if reqwest fails. The error variant is
`ScrapeError::Download` (renamed from `DownloadTimeout` because curl exit 35 is
a TLS handshake error, not a timeout, and the old name produced misleading
"Download timeout" messages for HTTP 403 / connection-refused / real TLS
errors). `reqwest::blocking` requires the `blocking` feature on the `reqwest`
dep in `src-tauri/Cargo.toml`. The scrape is invoked by
`scrape_citation_chaser_cmd` (async, `spawn_blocking`) which has an
existence-shortcut (if both expected RIS files exist, return them immediately
without launching the browser) and logs every outcome to the audit table via
`log_error_best_effort` (skip outcomes are labeled "skipped" so they read
correctly in the Audit Timeline / Diagnostics).

## Work Guidance

- Files keyed on `clean_doi_filename(normalized_doi)`, consistent with Citation
  Chaser RIS naming (`{clean_doi}_references.ris`).
- When modifying cancel/timeout behavior, preserve the `sleep_or_cancel` pattern
  in EVERY poll loop.

## Verification

Tested inline (15 pure tests in `citation_chaser.rs::tests`:
`detect_empty_or_disconnect` branches, `validate_ris_nonempty`, `CancelToken`
semantics, `sleep_or_cancel`, `clean_doi_filename`, `ScrapeKind` helpers) +
`tests/citation_chaser_test.rs` (1 pure `Validation` check + 6 `#[ignore]`d live
tests: refs-only, cits-only, both, zero-refs-returns-NoData-promptly,
zero-cits-returns-NoData-no-cached-file, cancel-returns-Cancelled-promptly).

## Child DOX Index

No child `AGENTS.md` files. This module owns two files (`citation_chaser.rs`,
`browser.rs`) with no further durable boundaries.