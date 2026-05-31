# Screening Screen: Stale-Data Window - Analysis & Plan

## Problem Statement

When the user navigates to the Screening screen the Vue component mounts and
immediately calls `fetchReadiness()`, which fires the IPC command
`get_screening_readiness` to the Rust backend.  
Until that IPC call resolves the screen renders **stale / default state**:
counts of 0, no guardrails, incorrect "Ready" signal - all of which can confuse
or mislead the user.

---

## Root-Cause Analysis

### 1. What actually happens on mount

```
User clicks Screening nav
  → Vue router lazy-loads screening-progress.vue   (~instant, pre-fetched)
  → onMounted() fires
      → fetchReadiness()                            ← IPC round-trip begins
            → store.loading = true  (only on first load)
            → tauriCommand('get_screening_readiness')
                 ↳ Rust: acquires std::sync::Mutex<Connection>
                 ↳ Rust: runs 4–7 COUNT / EXISTS queries (criteria, aims, config,
                          count_working, count_unscreened, max_article_char_len)
                 ↳ Rust: optionally computes token estimation
            → store.readiness = data                ← screen finally correct
```

### 2. What the template renders *before* `readiness` arrives

| Reactive value | State during IPC gap | What user sees |
|---|---|---|
| `readinessLoading` | `true` | Small spinner in top-right corner - **only if `readiness` is already populated** (second+ visits) |
| `readiness` | `null` | The whole `<template v-else-if="readiness">` block is **hidden** |
| First-visit (`!initialized`) | `loading = true` | A centered spinner + "Loading screening data…" message ✓ |
| Subsequent visits (`initialized`) | `loading = false` immediately | **The full old content renders from stale `readiness` data**, only a tiny corner spinner appears |

### 3. The real pain point - second+ visits

On **first visit** the fullscreen spinner works correctly because `initialized`
starts `false` and `loading` is set to `true` before the IPC call.

On **every subsequent navigation** to the screen (most common case):

- `initialized` is already `true` → `loading` stays `false`
- `readiness` still holds the previous data (potentially stale by minutes)
- The view renders immediately with stale counts and stale guardrail state
- Only a tiny 16 px corner spinner signals that data is refreshing
- The user may read incorrect article counts or hit "Start Screening" based on
  stale readiness before the data corrects itself

### 4. How long is the IPC gap?

`get_screening_readiness` holds the single `std::sync::Mutex<Connection>` for
the entire call and runs at minimum 4 COUNT/EXISTS queries, plus optionally
`max_article_char_len` (a table scan if the index on `data_length` is missing).
On a project with thousands of articles this can take **100–500 ms**, which is
long enough to be clearly perceptible.

### 5. What is already in place

- **First-visit fullscreen spinner** - works correctly.
- **Optimistic progress on `startScreening()`** - already sets `progress`
  before the IPC call returns; this part is fine.
- **Small corner refresh hint** - present but easy to miss (16 px spinner,
  absolute positioned top-right). Users don't associate it with stale data
  below.

---

## Options

### Option A - Loading Overlay on Re-Navigation (Recommended)

**Idea:** When the component mounts and `readiness` is stale (i.e.,
`initialized` is `true` but we are re-navigating), immediately set
`loading = true` in the store *before* the IPC call, so the fullscreen spinner
appears for both first and subsequent visits.

**Where to change:**

#### [MODIFY] [screening.ts store](file:///home/user/code/bango/src/stores/screening.ts)

In `fetchReadiness()`, remove the `isFirstLoad` guard so `loading` is always
set to `true` at entry and `false` on exit:

```diff
  async function fetchReadiness(): Promise<void> {
-   const isFirstLoad = !initialized.value;
-   if (isFirstLoad) {
-     loading.value = true;
-   }
+   loading.value = true;
    error.value = null;
    try {
      const data = await tauriCommand<ScreeningReadiness>('get_screening_readiness');
      readiness.value = data;
      if (data.progress) {
        progress.value = data.progress;
      }
      initialized.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
-     if (isFirstLoad) {
-       loading.value = false;
-     }
+     loading.value = false;
    }
  }
```

#### [MODIFY] [screening-progress.vue](file:///home/user/code/bango/src/views/screening-progress.vue)

Change the loading template condition so the spinner shows whenever loading is
true, **not** only when `readiness` is absent. Keep the "live-update" tiny
corner hint for background refreshes that happen while the screen is already
fully rendered (e.g., after a run completes):

```diff
- <!-- Initial Loading State (only if no data at all) -->
- <div v-if="readinessLoading && !readiness" class="screening-view__loading">
+ <!-- Loading overlay - shown any time readiness is being fetched (first or repeat visit) -->
+ <div v-if="readinessLoading" class="screening-view__loading">
    <div class="screening-view__spinner" />
    <p>Loading screening data&hellip;</p>
  </div>
```

And remove (or repurpose) the corner hint since it would only show during
background refreshes that users don't need to be aware of:

```diff
- <!-- Non-blocking Loading Indicator (shown if refreshing in background) -->
- <div
-   v-if="readinessLoading && readiness"
-   class="screening-view__refreshing-hint"
-   ...
- >
-   <div class="screening-view__spinner-sm" />
- </div>
```

**Pros:**
- Minimal change (2 files, ~8 lines).
- Zero stale data visible - user always sees a clear loading state.
- Consistent UX with first-visit behavior (same spinner).
- No backend changes needed.

**Cons:**
- A brief (100–500 ms) fullscreen spinner on every navigation - may feel
  slightly slower than Option B.

---

### Option B - Speed Up the Backend Query (Complementary)

**Idea:** Make `get_screening_readiness` faster so the window is imperceptible.

**Analysis of the queries:**

| Query | Speed | Notes |
|---|---|---|
| `has_any_aims` / `has_inclusion` / `has_exclusion` / `has_llm_config` | Very fast (EXISTS on small tables) | Not a concern |
| `count_working` | Fast (status index exists) | Not a concern |
| `count_unscreened_working` | Fast (status + screened_at index) | Not a concern |
| `max_article_char_len` | **Potentially slow** | Full scan unless `data_length` is indexed |

**Fix:** Add a DB index on `articles(data_length)` in a migration. This brings
`max_article_char_len` from O(n) to O(1) using the B-tree max.

Additionally, the token-estimation computation runs inside the DB mutex lock,
which blocks any concurrent DB access. It could be moved **outside** the lock:

```diff
  // In get_screening_readiness (screening.rs command)
  // 1. Release the lock before token estimation
  let max_chars = {
      let conn = db_state.conn.lock()...?;
      article_repo::max_article_char_len(&conn)?
      // lock released here
  };
  
  // 2. Token estimation (CPU-only, no DB needed)
  let token_warning = if total_unscreened > 0 {
      let config = ...;
      ...compute warning outside of lock scope...
  } else { None };
```

**Pros:**
- Faster navigation overall; may bring the gap below ~50 ms.
- Better for the DB generally (the index helps other queries too).

**Cons:**
- Requires a new DB migration (`v005_data_length_index.rs`).
- Does not eliminate the stale-data window - just makes it shorter.
- Does not help if the DB is under write pressure (e.g., during import).

---

## Recommendation

**Do both, in order:**

1. **Option A first** (the overlay) - it's tiny, safe, and immediately fixes
   the UX problem with zero risk of regression.
2. **Option B after** (the index + lock scope) - it makes the overlay disappear
   faster and is a general performance improvement.

Together they give: *instant visual feedback* + *fast data retrieval*.

---

## Verification Plan

### Manual
1. Navigate away from the Screening screen (e.g., to Articles) and back.
2. Confirm a spinner/overlay appears immediately on arrival.
3. Confirm the spinner disappears and correct data is shown once the IPC call
   returns.
4. Confirm a running screening job still shows live progress (not a blank
   spinner) on re-navigation.

### Automated (existing test infrastructure)
- The store change is pure logic; the existing Vitest store tests should
  continue to pass without modification.
- No new Rust tests needed for Option A; Option B would require a migration
  test.
