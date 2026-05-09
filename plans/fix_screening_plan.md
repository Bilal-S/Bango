# Screening Engine — 5-Problem Fix Plan (Final)

## Background

After thorough code review and feedback incorporation, all root causes are confirmed. The fixes are surgical — no architectural rework needed.

---

## Problem 1 — Too Many LLM Requests

### Root Cause

Two compounding issues:

1. **Wrong sort key**: `get_next_unscreened_working_batch` orders by `imported_at ASC` — a timestamp that can be identical for bulk-imported articles. When a batch fails to parse (e.g., wrong response format), `screened_at` is never set, so the next loop iteration re-fetches the **exact same batch**. With 6 articles and a bad response format, this loops indefinitely.

2. **Response envelope mismatch**: The LLM returns content wrapped in a `message.content` key (or similar envelope). The current `extract_json` only handles bare arrays and code-fenced arrays — it cannot unwrap envelopes. Every batch therefore fails to parse → marks articles as errors → but they're still `screened_at IS NULL` → refetched on next iteration.

3. **Concurrent-start risk**: `start_screening` is currently blocking IPC. If called more than once, multiple engine instances spawn, each making their own requests.

### Fix — `article_repo.rs`

Change ordering to `sequence_id ASC` (unique monotonic integer, set at import time):

```sql
SELECT id, sequence_id, title, abstract_text, authors, publication_year
FROM articles
WHERE status = 'working' AND screened_at IS NULL
ORDER BY sequence_id ASC
LIMIT ?1
```

No OFFSET is needed. Once an article is written (success or error), its `screened_at` is set (or `screening_error = 1`), so subsequent queries naturally advance. `sequence_id` guarantees deterministic, non-repeating paging.

### Fix — `client.rs` (response content extraction)

The LLM client already unwraps the OpenAI `choices[0].message.content` structure. However, some providers (like the one shown in the sample) return a different envelope. Make the content extraction in `send_openai_compatible` and `send_google` more robust by searching for a `content` key at the **first two levels** of the response object if the normal path fails:

```rust
// Pseudocode for robust content extraction
fn extract_content_from_response(value: &serde_json::Value) -> Option<String> {
    // Level 0: is value itself a string?
    if let Some(s) = value.as_str() { return Some(s.to_string()); }
    // Level 1: check standard paths first
    if let Some(s) = value["choices"][0]["message"]["content"].as_str() { return Some(s.to_string()); }
    // Level 1: scan all top-level keys for a "content" key
    if let Some(obj) = value.as_object() {
        for (_, v) in obj {
            if let Some(s) = v["content"].as_str() { return Some(s.to_string()); }
            // Level 2: if content is an array, join text fields
            if let Some(arr) = v["content"].as_array() {
                // collect all string values or objects with a text/reasoning field
            }
        }
    }
    None
}
```

This handles:
- Standard OpenAI: `choices[0].message.content`  
- The observed z.ai format: `message.content` (array or string)
- Any other provider that places `content` one level down from the root

### Fix — `engine.rs` (`extract_json` fallback)

As a second line of defense in the **engine** (after `client.rs` already extracts a string), `extract_json` should also handle cases where the string itself contains an embedded array wrapped in an object:

1. Try bare array (current behavior).
2. Try code-fence stripping (current behavior).
3. **New**: Try to find and extract a JSON array anywhere within the first 2 levels of a JSON object (scan for `[` as the first non-whitespace character after `{...content...}`).

### Fix — `commands/screening.rs` (concurrent guard)

Before spawning a new engine, check if one is already running:

```rust
{
    let guard = screening_state.engine.read().await;
    if let Some(ref existing) = *guard {
        if existing.get_progress().await.is_running {
            return Ok(existing.get_progress().await);
        }
    }
}
```

---

## Problem 2 — No Live Progress + Save-After-Each-Response

### Root Cause

- `start_screening` is a **blocking IPC call** — it holds the channel for the entire screening run. The 2-second `setInterval` poll starts but cannot fire while the IPC is blocked, so the UI never updates mid-run.
- The Dashboard's `screeningProgress` computes from `articlesStore.articles` (stale in-memory snapshot). It has no awareness of a live engine run.
- No Tauri event is emitted after each article is processed, so there is no push signal available.

### Fix — `commands/screening.rs` (non-blocking)

Change `start_screening` to spawn the engine in a background Tokio task and **return immediately**:

```rust
let app_handle = app_handle.clone();
tokio::spawn(async move {
    let _ = engine.run_sync(&db_state.conn, config, criteria, aims, app_handle).await;
    // Engine clears itself from ScreeningState when done
});
Ok(engine_initial_progress)
```

The `DbState.conn` is a `std::sync::Mutex<Connection>` wrapped in a `tauri::State`. Since `State<'_, T>` is `Clone`-able for Arc-backed managed state, we pass a cloned reference into the spawned task.

### Fix — `engine.rs` (Tauri event emission)

Accept `tauri::AppHandle` in `run_sync`. After **each article** is saved to the DB (success or error), emit a `screening:progress` event:

```rust
pub async fn run_sync(
    &self,
    conn_mutex: &std::sync::Mutex<Connection>,
    config: LlmConfig,
    criteria: Vec<Criterion>,
    aims: Vec<ResearchAim>,
    app_handle: tauri::AppHandle,   // NEW
) -> Result<(), AppError> {
    // ... after each article write:
    let snapshot = self.progress.lock().await.clone();
    let _ = app_handle.emit("screening:progress", &snapshot);
}
```

> [!NOTE]
> `ScreeningEngine` gains a Tauri dependency. This is acceptable. In non-Tauri test contexts (E2E tests), a mock `AppHandle` or a no-op emit wrapper can be used.

### Fix — `stores/screening.ts` (replace poll with event listener)

The store registers a listener when a screening run starts and cleans it up on completion or unmount:

```ts
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

let unlistenProgress: UnlistenFn | null = null;

async function startListening(): Promise<void> {
  if (unlistenProgress) return; // already listening
  unlistenProgress = await listen<ScreeningProgress>('screening:progress', (event) => {
    progress.value = event.payload;
    if (!event.payload.isRunning && unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
  });
}
```

### Fix — `screening-progress.vue`

- Remove the `setInterval` poll entirely.
- Call `store.startListening()` after `startScreening()` succeeds.
- Clean up listener in `onUnmounted`.

> [!NOTE]
> If Tauri is unavailable (browser dev mode), display the same "Tauri not available" notice used on other screens, consistent with the rest of the app.

### Fix — `dashboard.vue` + `use-dashboard.ts`

The Dashboard's Screening Progress card should consume the **screening store's live progress** when a run is active, falling back to the article-count-based computation when no run is in progress:

```ts
// In use-dashboard.ts
const screeningProgress = computed<ScreeningProgress>(() => {
  const liveProgress = screeningStore.progress;
  if (liveProgress && liveProgress.isRunning) {
    // Live data from the engine
    return {
      screened: liveProgress.completed,
      total: liveProgress.total,
      percentage: screeningStore.percentage,
    };
  }
  // Fall back to article-count snapshot
  const all = articlesStore.articles;
  const total = all.length;
  const screened = all.filter((a) => a.aiDecision !== null).length;
  return { screened, total, percentage: total > 0 ? Math.round((screened / total) * 100) : 0 };
});
```

---

## Problem 3 — Cannot Cancel Screening

### Root Cause

The `stop_screening` command correctly sets `cancel_token = true`. The engine's loop checks it at each iteration. However, because `start_screening` was blocking IPC, the stop command couldn't get through while the start command held the channel.

### Fix

**No separate fix needed.** Once `start_screening` is non-blocking (Problem 2), `stop_screening` can be called at any time while the background task runs and the cancel token is respected at the next loop iteration boundary.

**Optional enhancement (not in scope now):** Propagate cancellation into in-flight HTTP requests via `tokio_util::CancellationToken` so long LLM calls abort immediately.

---

## Problem 4 — False "Criteria Not Met" Warning on Load

### Root Cause

`blockingReasons` currently evaluates all conditions simultaneously. When prerequisites (aims, LLM) aren't configured, the backend returns `total_unscreened = 0` (correctly — because it short-circuits). But the frontend shows *all* failing conditions at once, including "No unscreened articles", even though the user should fix prerequisites first.

The flash itself happens because `readiness` is `null` during the async fetch, then immediately switches to a state with all zeros, causing the guardrails section to render before the user has had a chance to see what's happening.

### Fix — `screening-progress.vue`

**Reorder `blockingReasons` to cascade**: prerequisites first, articles count only after all prerequisites pass. This means the user sees one actionable group at a time:

```ts
const blockingReasons = computed((): string[] => {
  const r = readiness.value;
  if (!r) return []; // still loading — show spinner, not warnings

  const prereqReasons: string[] = [];
  if (!r.hasAims)      prereqReasons.push('No research aims defined. Add aims in the Criteria Editor.');
  if (!r.hasInclusion) prereqReasons.push('No inclusion criteria defined. Add criteria in the Criteria Editor.');
  if (!r.hasExclusion) prereqReasons.push('No exclusion criteria defined. Add criteria in the Criteria Editor.');
  if (!r.hasLlmConfig) prereqReasons.push('LLM is not configured. Set up your LLM in Settings.');

  // Only surface the "no articles" warning once prerequisites are satisfied
  if (prereqReasons.length === 0 && r.totalUnscreened === 0) {
    return ['No unscreened articles in the working list. Import and deduplicate articles first.'];
  }
  return prereqReasons;
});
```

**Loading guard**: The `v-if="readinessLoading && !readiness"` spinner is correct. Add `v-else-if="!readinessLoading && readiness"` (no content while `loading=true AND readiness already set`) to prevent the flash on subsequent background refreshes.

---

## Problem 5 — End-to-End Test With Mock Responses

### Fix

**`LlmClient` trait** — makes the engine testable without a real HTTP server:

```rust
// src-tauri/src/screening/llm_client.rs  [NEW]
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError>;
}

// Real implementation
pub struct HttpLlmClient(pub LlmConfig);

#[async_trait::async_trait]
impl LlmClient for HttpLlmClient {
    async fn send(&self, system: &str, user: &str) -> Result<(String, usize), AppError> {
        client::send_chat_completion(&self.0, system, user).await
    }
}
```

**`engine.run_sync`** accepts `impl LlmClient` instead of calling `client::send_chat_completion` directly.

**Integration test** — `src-tauri/src/screening/tests/e2e_test.rs` [NEW]:

```rust
// Seeds 6 articles, 1 inclusion + 1 exclusion criterion, 1 research aim.
// Mock client returns the sample response envelope (message.content array).
// Asserts: exactly 6 screened, correct include/exclude counts, no re-requests.
```

Test scenarios:
1. **Happy path** — bare array response, batch=2, 6 articles → 3 batches, 6 screened.
2. **Envelope format** — `message.content` wrapper → same result.
3. **Partial error** — one batch returns malformed JSON → those articles get `screening_error=1`, rest succeed.
4. **Cancel mid-run** — cancel after first batch → only first batch's articles screened.
5. **Resume** — after cancellation, re-run → only remaining articles processed (no re-screening).

---

## Spec Update Required

> [!IMPORTANT]
> `docs/superpowers/specs/bango-v3-spec.md` §9.3 currently states: *"Each article is sent to the LLM as a separate API call."* This must be updated to reflect the batch-grouping option (multiple articles per prompt, configurable 1–15). The batch size slider and its behavior should be documented there.

---

## Summary of Files to Change

| File | Change |
|------|--------|
| `src-tauri/src/db/article_repo.rs` | `ORDER BY sequence_id ASC` in batch query |
| `src-tauri/src/llm/client.rs` | Robust `content` key extraction — scan top 2 levels of response object |
| `src-tauri/src/screening/engine.rs` | (1) Improved `extract_json`; (2) Accept `AppHandle` + emit per-article; (3) Accept `impl LlmClient` trait |
| `src-tauri/src/screening/llm_client.rs` *(new)* | `LlmClient` trait + `HttpLlmClient` wrapper |
| `src-tauri/src/commands/screening.rs` | (1) Concurrent-start guard; (2) Non-blocking spawn; (3) Pass `AppHandle` to engine |
| `src/stores/screening.ts` | `startListening()` with `listen('screening:progress')`; cleanup on stop |
| `src/composables/use-screening.ts` | Expose `startListening`; Tauri-unavailable fallback notice |
| `src/views/screening-progress.vue` | Remove `setInterval`; fix `blockingReasons` cascade; add Tauri-unavailable guard |
| `src/views/dashboard.vue` | Use live screening store progress when run is active |
| `src/composables/use-dashboard.ts` | Merge live `screeningStore.progress` into `screeningProgress` computed |
| `src-tauri/src/screening/tests/e2e_test.rs` *(new)* | 5-scenario integration test with mock LLM + in-memory SQLite |
| `docs/superpowers/specs/bango-v3-spec.md` | Update §9.3 to document batch-grouping option (1–15 articles per prompt) |
