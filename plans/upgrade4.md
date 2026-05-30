# Upgrade 4 — UX Optimization & Enhancement Plan

> Based on analysis of Bango v3 codebase against spec (`docs/superpowers/specs/bango-v3-spec.md`) and development rules (`CLAUDE.md`).
> Covers 8 selected UX improvements + 2 new AI-powered features.

---

## Table of Contents

1. [Feature 3: Batch Operations for Articles](#feature-3-batch-operations)
2. [Feature 4: Advanced Filtering & Sorting Presets](#feature-4-advanced-filtering--sorting-presets)
3. [Feature 7: Undo System](#feature-7-undo-system)
4. [Feature 8: Improved Error Handling & Retry](#feature-8-improved-error-handling--retry)
5. [Feature 9: Responsive Sidebar Navigation Enhancements](#feature-9-responsive-sidebar-navigation)
6. [Feature 11: Enhanced Screening Workflow](#feature-11-enhanced-screening-workflow)
7. [Feature 12: Accessibility (ARIA & Keyboard Navigation)](#feature-12-accessibility)
8. [Feature 13: Dark Mode Support](#feature-13-dark-mode)
9. [Feature A: AI-Generated Criteria from Research Topic](#feature-a-ai-generated-criteria)
10. [Feature B: AI Summary with Academic Referencing in PRISMA](#feature-b-ai-summary-with-academic-referencing)

---

## Feature 3: Batch Operations

### Problem

Users must interact with articles one at a time. There is no way to select multiple articles and perform bulk actions (include, reject, tag, label, delete). This is a critical workflow gap for researchers managing hundreds or thousands of articles.

### Current State

- `article-table.vue` renders a table with click-to-select behavior (single `selectedId`)
- `article-detail-panel.vue` operates on a single article at a time
- `use-article-search.ts` composable manages a single `selectedArticle`
- No checkbox column, no multi-select state, no bulk action bar

### Solution

#### 3.1 Multi-Select in Article Table

**Files to modify:**
- `src/components/article-table.vue` — Add checkbox column, multi-select state
- `src/views/article-list.vue` — Wire bulk selection state
- `src/composables/use-article-search.ts` — Add `selectedIds` ref, bulk methods

**Approach:**

1. Add a `selectedIds: Ref<Set<string>>` to `use-article-search.ts`
2. Add a checkbox column to `article-table.vue` as the first column (before `#`)
3. Support:
   - Individual row checkbox toggle
   - Header "select all" checkbox (selects all on current page)
   - Shift+Click for range selection
4. Emit `toggleSelect(id)` and `toggleSelectAll(ids)` events

```vue
<!-- article-table.vue: new checkbox column -->
<th class="w-10 py-4 px-2">
  <input
    type="checkbox"
    :checked="allSelected"
    :indeterminate="someSelected && !allSelected"
    @change="emit('toggleSelectAll')"
  />
</th>
<!-- per-row -->
<td class="py-5 px-2">
  <input
    type="checkbox"
    :checked="selectedIds.has(article.id)"
    @click.stop="emit('toggleSelect', article.id)"
  />
</td>
```

#### 3.2 Bulk Action Bar

**New file:** `src/components/bulk-action-bar.vue`

A sticky bar that appears when > 0 articles are selected, showing:
- Selection count: "3 articles selected"
- Action buttons: **Include**, **Reject**, **Move to Working**, **Add Tag**, **Add Label**, **Clear Selection**
- Dismiss button (×)

```vue
<script setup lang="ts">
defineProps<{
  selectedCount: number;
}>();

defineEmits<{
  bulkInclude: [];
  bulkReject: [];
  bulkMoveToWorking: [];
  bulkAddTag: [];
  bulkAddLabel: [];
  clearSelection: [];
}>();
</script>
```

**Styling:** Fixed to bottom of the article list area, elevated with shadow, uses the Scholarly Precision design tokens (8px radius, indigo primary actions).

#### 3.3 Bulk Store Operations

**Files to modify:**
- `src/stores/articles.ts` — Add `bulkUpdateStatus(ids, newStatus)`, `bulkAddTag(ids, tagId)`, `bulkAddLabel(ids, labelId)`
- `src-tauri/src/` — New Tauri commands for bulk operations (Rust backend)

**New Tauri commands (Rust):**
- `bulk_update_article_status { ids: Vec<String>, new_status: String }` → updates multiple articles
- `bulk_add_tag_to_articles { article_ids: Vec<String>, tag_name: String }` → adds tag to multiple articles
- `bulk_add_label_to_articles { article_ids: Vec<String>, label_name: String }` → adds label to multiple articles

**Store method pattern:**
```typescript
async function bulkUpdateStatus(ids: string[], newStatus: ArticleStatus): Promise<void> {
  await tauriCommand('bulk_update_article_status', { ids, newStatus });
  invalidate();
  await fetchIfNeeded();
}
```

#### 3.4 Bulk Tag/Label Dialog

**New file:** `src/components/bulk-tag-dialog.vue`

A modal that opens when "Add Tag" or "Add Label" is clicked from the bulk action bar. Contains:
- SuggestInput for tag/label name (reuses existing `suggest-input.vue`)
- Preview of how many articles will be affected
- Confirm/Cancel buttons

---

## Feature 4: Advanced Filtering & Sorting Presets

### Problem

The current filter panel (`article-filter-panel.vue`) provides good per-session filtering but has no way to save, load, or share filter configurations. Users must re-configure filters every time they visit the Articles view.

### Current State

- `article-filter-panel.vue` has: title, author, year range, journal, tags, labels filters
- `use-article-search.ts` has `ArticleFilter` interface and `filter` ref
- Filters reset on navigation (no persistence)
- No sort presets, no URL-synced filter state

### Solution

#### 4.1 Save/Load Filter Presets

**New file:** `src/composables/use-filter-presets.ts`

```typescript
interface FilterPreset {
  id: string;
  name: string;
  filter: ArticleFilter;
  sortColumn: string | null;
  sortDirection: 'asc' | 'desc';
  statusTab: string;
  createdAt: string;
}

export function useFilterPresets() {
  const presets = ref<FilterPreset[]>([]);

  function loadFromStorage(): void { /* localStorage read */ }
  function saveToStorage(): void { /* localStorage write */ }
  function savePreset(name: string, filter: ArticleFilter, ...): void { }
  function deletePreset(id: string): void { }
  function applyPreset(id: string): ArticleFilter { }

  return { presets, loadFromStorage, savePreset, deletePreset, applyPreset };
}
```

#### 4.2 Filter Preset UI

**Files to modify:**
- `src/components/article-toolbar.vue` — Add preset dropdown button next to Filter toggle
- `src/components/article-filter-panel.vue` — Add "Save Current Filters" button and preset list

**UI pattern:**
- A dropdown button labeled "Saved Filters" with a bookmark icon
- Dropdown shows saved presets as a list
- Each preset has: name, apply button, delete button
- "Save Current" button opens a small dialog asking for preset name

#### 4.3 URL-Synced Filter State

**Files to modify:**
- `src/views/article-list.vue` — Read/write filter params from URL query string
- `src/composables/use-article-search.ts` — Add `serializeFilters()` and `deserializeFilters()`

**URL format:**
```
/articles?status=working&yearFrom=2020&yearTo=2024&tags=machine-learning,clinical-trial&sort=title&dir=asc
```

This enables shareable filtered views (e.g., from Dashboard status tiles, which already use `?status=` partially).

---

## Feature 7: Undo System

### Problem

Accidental screening decisions, tag changes, or status moves cannot be undone. The spec explicitly says "no global undo stack" (§14), but per-action undo via toast notification is a lightweight alternative that doesn't conflict with the spec.

### Current State

- `article-detail-panel.vue` emits `moveArticle`, `updateTags`, `updateLabels` — these are one-way
- Stores call Tauri commands directly with no rollback mechanism
- No toast/notification system currently exists in the app

### Solution

#### 7.1 Toast Notification System

**New file:** `src/composables/use-toast.ts`

A global toast system that shows brief notifications at the bottom-right of the screen.

```typescript
interface Toast {
  id: number;
  message: string;
  type: 'success' | 'error' | 'info' | 'warning';
  duration: number; // ms, 0 = persistent
  undoAction?: () => Promise<void>;
}

export function useToast() {
  const toasts = ref<Toast[]>([]);

  function show(message: string, type: Toast['type'] = 'info', undoAction?: () => Promise<void>): void { }
  function dismiss(id: number): void { }

  return { toasts, show, dismiss };
}
```

**New file:** `src/components/toast-container.vue`

Renders toasts as stacked notifications with optional "Undo" button.

#### 7.2 Undoable Actions Pattern

Rather than a full undo stack, use **toast-based undo** for specific reversible actions:

**Approach:**
1. Before executing a mutating action, capture the current state (snapshot)
2. Execute the action
3. Show a toast with "Undo" button for 5 seconds
4. If user clicks "Undo", reverse the action using the snapshot

**Files to modify:**
- `src/stores/articles.ts` — Wrap `moveArticle`-like operations with undo context
- `src/components/article-detail-panel.vue` — Trigger undo-aware actions

**Example — Status change undo:**
```typescript
// In article-list.vue or composable
async function handleMoveArticle(id: string, newStatus: string): Promise<void> {
  const article = articles.value.find(a => a.id === id);
  const previousStatus = article?.status;
  if (!article || !previousStatus) return;

  // Execute the move
  await moveArticle(id, newStatus);

  // Show toast with undo
  toast.show(`Article moved to ${newStatus}`, 'success', async () => {
    await moveArticle(id, previousStatus);
    toast.show('Action undone', 'info');
  });
}
```

#### 7.3 Keyboard Shortcut

- `Ctrl+Z` triggers undo of the most recent toast-based action (if still available)
- Wired through the existing keyboard handling pattern in the app

---

## Feature 8: Improved Error Handling & Retry

### Problem

Error handling exists (`llm-error.ts` has pattern matching, `screening-progress.vue` shows error banners) but there's no:
- Inline retry on individual failed operations
- Network status indicator
- Graceful error boundaries for view-level crashes

### Current State

- `llm-error.ts` — Pattern-matches LLM errors with troubleshooting links ✅
- `screening-progress.vue` — Shows error banner with help link ✅
- Stores set `error` ref on failure ✅
- No retry mechanism on individual operations
- No global error boundary
- No network connectivity indicator

### Solution

#### 8.1 Inline Retry Buttons

**Files to modify:**
- `src/views/dashboard.vue` — Add inline retry on error state (already has retry button ✅)
- `src/views/screening-progress.vue` — Add "Retry" button inline in error banner
- `src/stores/screening.ts` — Add `retryLastScreening()` method

**Pattern:** Every error display should have a contextual retry action:

```vue
<div v-if="error" class="error-banner">
  <p>{{ error }}</p>
  <button @click="retry">Retry</button>
  <button @click="dismiss">Dismiss</button>
</div>
```

#### 8.2 Network Status Indicator

**New file:** `src/composables/use-network-status.ts`

```typescript
export function useNetworkStatus() {
  const isOnline = ref(navigator.onLine);

  function handleOnline(): void { isOnline.value = true; }
  function handleOffline(): void { isOnline.value = false; }

  onMounted(() => {
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);
  });

  onUnmounted(() => {
    window.removeEventListener('online', handleOnline);
    window.removeEventListener('offline', handleOffline);
  });

  return { isOnline };
}
```

**Files to modify:**
- `src/components/app-shell.vue` — Show a small "Offline" banner at the top when `isOnline` is false
- Use the existing `useViewport` pattern as a guide

**UI:** A thin yellow/amber bar below the mobile header (or below the sidebar top) that reads: "You are offline. Some features may be unavailable."

#### 8.3 Error Boundary Component

**New file:** `src/components/error-boundary.vue`

A Vue `onErrorCaptured` wrapper that catches rendering errors in child components and shows a fallback UI:

```vue
<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue';

const hasError = ref(false);
const errorMessage = ref('');

onErrorCaptured((err) => {
  hasError.value = true;
  errorMessage.value = err.message;
  return false; // Prevent propagation
});

function retry(): void {
  hasError.value = false;
  errorMessage.value = '';
}
</script>

<template>
  <slot v-if="!hasError" />
  <div v-else class="error-fallback">
    <span class="material-symbols-outlined">error</span>
    <p>Something went wrong: {{ errorMessage }}</p>
    <button @click="retry">Try Again</button>
  </div>
</template>
```

**Usage in `app-shell.vue`:**
```vue
<ErrorBoundary>
  <router-view />
</ErrorBoundary>
```

---

## Feature 9: Responsive Sidebar Navigation Enhancements

### Problem

The sidebar already has responsive behavior (collapse on desktop, drawer on mobile via `app-shell.vue`). However, it lacks:
- Keyboard navigation support within the sidebar
- Active section grouping (workflow sections)
- Quick status counts in the sidebar
- Smooth animation refinements

### Current State

- `nav-sidebar.vue` — 260px sidebar with 9 nav items + Help ✅
- `app-shell.vue` — Collapse/expand toggle, mobile drawer with backdrop ✅
- `use-viewport.ts` — `isBelowMd` reactive breakpoint ✅
- Active route highlighting ✅
- Collapsed icon-only mode ✅

### Solution

#### 9.1 Workflow Section Grouping

**Files to modify:**
- `src/components/nav-sidebar.vue` — Add section headers between nav groups

Group the nav items into logical workflow sections:

```
── Setup ──────────────
  Dashboard
  Criteria
  Import RIS
── Process ────────────
  Deduplicate
  Screening
  Tags & Labels
── Results ────────────
  Articles
  PRISMA
── System ─────────────
  Settings
  Help Guide
```

**Implementation:**
```typescript
interface NavSection {
  label: string;
  items: NavItem[];
}

const navSections: NavSection[] = [
  { label: 'Setup', items: [dashboard, criteria, importRis] },
  { label: 'Process', items: [dedup, screening, tags] },
  { label: 'Results', items: [articles, prisma] },
];
const systemItems: NavItem[] = [settings, help];
```

Add small section headers in the sidebar (`text-[10px] uppercase tracking-widest text-slate-400`).

#### 9.2 Article Count Badges in Sidebar

**Files to modify:**
- `src/components/nav-sidebar.vue` — Import `useArticlesStore` and show working count

Show a small count badge next to "Articles" when there are unscreened working articles:

```vue
<span v-if="workingCount > 0" class="sidebar__badge-count">
  {{ workingCount }}
</span>
```

Similarly, show a count next to "Deduplicate" if there are unresolved duplicate pairs.

#### 9.3 Keyboard Navigation

- `Tab` / `Shift+Tab` navigates between sidebar links
- `Enter` activates the focused link
- `Escape` closes the mobile drawer
- Arrow keys navigate between items when sidebar has focus

---

## Feature 11: Enhanced Screening Workflow

### Problem

The screening view (`screening-progress.vue`) is a "fire and monitor" experience — users start screening and watch progress. There's no way to review individual screening decisions inline during the process, and no live decision feed.

### Current State

- `screening-progress.vue` — Progress bar, stats, pause/resume/stop ✅
- `screening-progress-bar.vue` — Visual progress bar ✅
- `screening-stats.vue` — Included/rejected/error counts ✅
- `use-screening.ts` — Manages screening lifecycle via Tauri events ✅
- `screening store` — Listens for `screening:progress` events ✅
- No live decision feed (noted as implementation gap in `implementation-gaps.md`)

### Solution

#### 11.1 Live Decision Feed

**New file:** `src/components/screening-decision-feed.vue`

A scrolling feed that shows real-time AI screening decisions as they happen.

```vue
<script setup lang="ts">
import type { ScreeningDecision } from '@/types';

defineProps<{
  decisions: ScreeningDecision[];
  isLive: boolean;
}>();
</script>

<template>
  <div class="decision-feed">
    <div class="decision-feed__header">
      <h3>Live Decisions</h3>
      <span v-if="isLive" class="decision-feed__live">
        <span class="decision-feed__live-dot" />
        Live
      </span>
    </div>
    <div class="decision-feed__list">
      <div v-for="d in decisions" :key="d.articleId" class="decision-feed__item">
        <span class="decision-feed__time">{{ formatTime(d.timestamp) }}</span>
        <span class="decision-feed__title">{{ d.title }}</span>
        <StatusBadge :status="d.decision === 'include' ? 'included' : 'rejected'" />
        <ConfidenceBar :confidence="d.confidence" />
      </div>
    </div>
  </div>
</template>
```

**Styling:** Uses CSS mask for fade effect at top/bottom edges (per design reference):
```css
.decision-feed__list {
  mask-image: linear-gradient(to bottom, transparent, black 10%, black 90%, transparent);
  max-height: 300px;
  overflow-y: auto;
}
```

#### 11.2 Screening Decision Type

**Files to modify:**
- `src/types/index.ts` — Add `ScreeningDecision` interface

```typescript
export interface ScreeningDecision {
  articleId: string;
  title: string;
  decision: 'include' | 'exclude' | 'error';
  confidence: number | null;
  timestamp: string;
}
```

#### 11.3 Event Integration

**Files to modify:**
- `src/stores/screening.ts` — Listen for a new `screening:decision` event from the Rust backend
- `src/views/screening-progress.vue` — Render the `ScreeningDecisionFeed` component

The Rust backend should emit individual decision events alongside the existing `screening:progress` events:

```rust
// Rust: emit per-article decision event
app.emit("screening:decision", ScreeningDecisionPayload { ... })?;
```

#### 11.4 Auto-Advance Article Detail

**Files to modify:**
- `src/components/article-detail-panel.vue` — After a status change (include/reject), optionally advance to the next article automatically

Add a setting/auto-behavior: when the user clicks "Include" or "Reject" in the detail panel footer, automatically navigate to the next article in the list after a brief 300ms delay (to show the status change confirmation).

---

## Feature 12: Accessibility (ARIA & Keyboard Navigation)

### Problem

The application has basic HTML semantics but lacks comprehensive ARIA labels, focus management, and keyboard navigation support required for accessibility compliance (WCAG 2.1 AA).

### Current State

- Semantic HTML used in most components ✅
- `role="switch"` and `aria-checked` on PRISMA toggle ✅
- Missing: ARIA labels on interactive elements, focus traps for modals, skip-to-content link, screen reader announcements

### Solution

#### 12.1 Skip-to-Content Link

**Files to modify:**
- `src/components/app-shell.vue` — Add hidden skip link

```vue
<a href="#main-content" class="skip-link">Skip to main content</a>
<!-- ... -->
<main id="main-content" class="app-shell__main">
```

```css
.skip-link {
  position: absolute;
  top: -40px;
  left: 0;
  background: var(--color-primary);
  color: white;
  padding: 8px 16px;
  z-index: 9999;
  transition: top 0.2s;
}
.skip-link:focus {
  top: 0;
}
```

#### 12.2 ARIA Labels on Interactive Elements

**Files to modify:** All component files

Add `aria-label` to:
- Icon-only buttons (close, edit, delete, navigate)
- Status badge elements
- Confidence bars
- Toggle switches
- Loading spinners (`role="status"`, `aria-label="Loading"`)

**Pattern:**
```vue
<button
  class="material-symbols-outlined"
  aria-label="Close detail panel"
  @click="emit('close')"
>
  close
</button>
```

#### 12.3 Focus Trap for Modals

**New file:** `src/composables/use-focus-trap.ts`

```typescript
export function useFocusTrap(containerRef: Ref<HTMLElement | null>) {
  function activate(): void {
    const container = containerRef.value;
    if (!container) return;
    const focusable = container.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    );
    if (focusable.length > 0) {
      focusable[0].focus();
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key !== 'Tab') return;
    // Trap focus within container
    // ...
  }

  return { activate, handleKeydown };
}
```

**Apply to:** `export-dialog.vue`, `criteria-edit-dialog.vue`, `article-detail-panel.vue` (when open on mobile)

#### 12.4 Live Regions for Dynamic Content

**Files to modify:**
- `src/views/screening-progress.vue` — Add `aria-live="polite"` region for progress updates
- `src/components/screening-stats.vue` — Add `aria-live="polite"` for stat changes

```vue
<div aria-live="polite" aria-atomic="true" class="sr-only">
  {{ percentage }}% complete. {{ progress.completed }} of {{ progress.total }} articles screened.
</div>
```

#### 12.5 Visually Hidden Utility Class

**Add to** `src/styles/base.css`:

```css
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border-width: 0;
}
```

#### 12.6 Keyboard Shortcuts Table

| Shortcut | Context | Action |
|----------|---------|--------|
| `?` | Global | Show keyboard shortcuts overlay |
| `Escape` | Detail panel | Close detail panel |
| `Escape` | Modal | Close modal/dialog |
| `J` / `↓` | Article list | Next article |
| `K` / `↑` | Article list | Previous article |
| `Enter` | Article list | Open detail panel |
| `I` | Article detail | Include article |
| `X` | Article detail | Reject article |
| `W` | Article detail | Move to working |
| `Ctrl+Z` | Global | Undo last action (via toast) |

---

## Feature 13: Dark Mode Support

### Problem

No dark mode exists. The app uses hardcoded light colors throughout. Users working in low-light environments (common for researchers working late) would benefit from a dark theme.

### Current State

- `tokens.css` — All colors defined as CSS custom properties on `:root` (light only)
- `base.css` — Tailwind `@theme` block with light colors
- Components use both CSS custom properties and hardcoded Tailwind classes
- `DESIGN.md` — No dark mode tokens defined

### Solution

#### 13.1 Dark Mode Token Set

**Files to modify:**
- `src/styles/tokens.css` — Add `[data-theme="dark"]` block with dark color values
- `src/styles/base.css` — Add dark mode Tailwind theme variants

**Dark mode color palette** (derived from Scholarly Precision, inverted):

```css
[data-theme="dark"] {
  --color-surface: #121218;
  --color-surface-dim: #0f0f14;
  --color-surface-container-lowest: #0a0a0f;
  --color-surface-container-low: #1a1a24;
  --color-surface-container: #1e1e2a;
  --color-surface-container-high: #282836;
  --color-surface-container-highest: #323242;
  --color-on-surface: #e2e0f0;
  --color-on-surface-variant: #c0bdd4;
  --color-outline: #8b88a0;
  --color-outline-variant: #4a4860;
  --color-sidebar: #0a0a12;
  --color-sidebar-text: #c0bdd4;
  --color-border: #323242;
  --color-hover: #282836;
  --color-error: #ffb4ab;
  --color-error-container: #93000a;
  /* Primary colors remain similar but slightly adjusted */
  --color-primary: #c3c0ff;
  --color-on-primary: #1b0069;
  --color-primary-container: #3f38c9;
}
```

#### 13.2 Theme Composable

**New file:** `src/composables/use-theme.ts`

```typescript
type Theme = 'light' | 'dark' | 'system';

export function useTheme() {
  const theme = ref<Theme>(loadTheme());
  const resolvedTheme = computed(() => {
    if (theme.value === 'system') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return theme.value;
  });

  function setTheme(newTheme: Theme): void {
    theme.value = newTheme;
    localStorage.setItem('bango-theme', newTheme);
    applyTheme(resolvedTheme.value);
  }

  function applyTheme(t: 'light' | 'dark'): void {
    document.documentElement.setAttribute('data-theme', t);
  }

  // System preference listener
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  function handleSystemChange(): void {
    if (theme.value === 'system') applyTheme(resolvedTheme.value);
  }
  mediaQuery.addEventListener('change', handleSystemChange);

  // Initialize
  applyTheme(resolvedTheme.value);

  return { theme, resolvedTheme, setTheme };
}

function loadTheme(): Theme {
  return (localStorage.getItem('bango-theme') as Theme) ?? 'system';
}
```

#### 13.3 Theme Toggle in Settings

**Files to modify:**
- `src/views/llm-config.vue` — Add theme selection (Settings view)
- `src/components/nav-sidebar.vue` — Add quick theme toggle icon in footer (sun/moon icon)

**Settings UI:** Three radio buttons: Light / Dark / System (follows OS)

**Sidebar footer:** A small icon button that cycles through themes:
- `light_mode` → `dark_mode` → `contrast` (system)

#### 13.4 Component Hardcoded Colors Audit

Many components use hardcoded Tailwind colors (e.g., `bg-white`, `text-slate-500`, `bg-indigo-50`). These need to be converted to design token references or conditional classes.

**Pattern:** Replace hardcoded colors with CSS custom property references:

```diff
- class="bg-white border border-slate-200"
+ class="bg-surface-container-lowest border border-outline-variant"
```

**Files needing audit (hardcoded Tailwind colors):**
- `article-toolbar.vue` — `bg-white`, `bg-slate-100`, `border-slate-200`
- `article-filter-panel.vue` — `bg-white`, `bg-slate-50`, `border-slate-200`
- `article-table.vue` — `bg-white`, `bg-slate-50/50`, `border-slate-200`
- `article-detail-panel.vue` — Extensive hardcoded colors
- `criteria-editor.vue` — `#ffffff`, `#e2e8f0` inline
- `tag-label-management.vue` — Uses Tailwind theme tokens (better)
- `dashboard.vue` — Uses CSS custom properties (good) with some hardcoded values

**Approach:** Incremental migration — start with the most-used components and replace hardcoded values with token references.

#### 13.5 Transition

Add a smooth transition when switching themes:

```css
html {
  transition: background-color 0.3s ease, color 0.3s ease;
}
```

---

## Feature A: AI-Generated Criteria from Research Topic

### Problem

Defining inclusion and exclusion criteria is one of the most time-consuming steps in a systematic review. Users often struggle to articulate precise, comprehensive criteria. The AI can help bootstrap this process by generating initial criteria from the research aims and topic.

### Current State

- `criteria-editor.vue` — Manual text entry for aims, inclusion, and exclusion criteria
- `criteria store` — CRUD operations for aims and criteria via Tauri commands
- Tag/label management already has "Suggest from AI" buttons ✅ (pattern to follow)
- No AI-powered criteria generation

### Solution

#### A.1 "Suggest Criteria" Button

**Files to modify:**
- `src/views/criteria-editor.vue` — Add "Suggest Criteria" button to both inclusion and exclusion sections
- `src/stores/criteria.ts` — Add `suggestCriteria(criterionType)` method

**UI:** Following the same pattern as `tag-label-management.vue`'s "Generate from AI" button:

```vue
<div class="add-criterion-row">
  <!-- existing input, priority select, add button -->
  <button
    class="btn-ai-suggest"
    :disabled="suggesting"
    @click="suggestCriteria('inclusion')"
  >
    <span class="material-symbols-outlined" :class="{ 'animate-spin': suggesting }">
      auto_awesome
    </span>
    {{ suggesting ? 'Generating...' : 'Suggest from AI' }}
  </button>
</div>
```

#### A.2 Tauri Command

**New Rust command:** `suggest_criteria`

**Input:**
- Research aims (list of text)
- Existing criteria (to avoid duplicates)
- Criterion type: inclusion or exclusion
- Article keywords (aggregated from working list)

**Prompt template:**

**System prompt:**
> You are a systematic literature review assistant. Generate a set of inclusion/exclusion criteria for a systematic review based on the provided research aims and topic context.

**User prompt:**
```
## Task
Generate a set of {inclusion/exclusion} criteria for a systematic literature review.

## Research Aims
{numbered list of aim entries}

## Existing {Inclusion/Exclusion} Criteria
{numbered list of already-defined criteria — do not duplicate}

## Article Keywords (from working list)
{aggregated unique keywords, comma-separated}

## Response Format
Return JSON exactly matching this schema:
{
  "criteria": [
    { "text": " criterion text here", "priority": "critical|high|standard|low|optional" }
  ]
}

Rules:
- Generate 5–15 criteria.
- Each criterion should be specific, measurable, and unambiguous.
- Assign appropriate priority levels.
- Do not duplicate existing criteria.
- Criteria should be derived from the research aims and topic context.
```

#### A.3 Store Integration

**Files to modify:**
- `src/stores/criteria.ts` — Add:

```typescript
const suggesting = ref(false);

async function suggestCriteria(criterionType: CriterionType): Promise<void> {
  suggesting.value = true;
  try {
    await tauriCommand('suggest_criteria', { criterionType });
    await refresh(); // Re-fetch to get the new criteria
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    suggesting.value = false;
  }
}
```

#### A.4 Review Before Adding

To prevent unwanted criteria from flooding the editor, the AI-suggested criteria should be presented in a review dialog before being committed:

**New file:** `src/components/criteria-suggest-dialog.vue`

Shows a list of suggested criteria with:
- Checkbox to include/exclude each suggestion
- Priority dropdown (pre-filled from AI suggestion, editable)
- Text editing for each suggestion
- "Add Selected" and "Cancel" buttons

Only checked criteria are created via `create_criterion` Tauri commands.

---

## Feature B: AI Summary with Academic Referencing in PRISMA

### Problem

The current AI summary (`summary-view.vue`) generates plain text paragraphs organized by theme. For academic rigor, the summary should include proper citations referencing specific included articles, and this enhanced summary should be accessible from the PRISMA screen as part of the review reporting workflow.

### Current State

- `summary-view.vue` — Standalone view with 5 sections (Key Themes, Research Trends, Methodological Strengths, Common Weaknesses, Gaps in Literature) ✅
- `use-summary.ts` — Calls `generate_summary` Tauri command ✅
- `prisma-diagram.vue` — PRISMA flow diagram with export ✅
- Summary output is plain text — no article citations, no referencing style
- Summary and PRISMA views are separate — no integration

### Solution

#### B.1 Enhanced Summary with Citations

**Files to modify:**
- `src/composables/use-summary.ts` — Add `SummaryCitation` type, update `SummaryOutput`
- `src/views/summary-view.vue` — Render citations inline, add reference list

**Updated types:**

```typescript
export interface SummaryCitation {
  articleId: string;
  title: string;
  authors: string[];
  year: number | null;
  doi: string | null;
}

export interface EnhancedSummarySection {
  content: string;
  citations: SummaryCitation[];
}

export interface EnhancedSummaryOutput {
  keyThemes: EnhancedSummarySection;
  researchTrends: EnhancedSummarySection;
  methodologicalStrengths: EnhancedSummarySection;
  commonWeaknesses: EnhancedSummarySection;
  gapsInLiterature: EnhancedSummarySection;
  references: SummaryCitation[];
}
```

#### B.2 Updated Prompt Template

**Rust backend prompt update for `generate_summary`:**

The prompt should instruct the AI to:
1. Cite specific articles inline using numbered references `[1]`, `[2]`, etc.
2. Provide a references section mapping numbers to article details
3. Use APA-like referencing style

**Updated user prompt:**
```
## Task
Generate a structured summary of the included articles in a systematic literature review.
Cite specific articles inline using numbered references [1], [2], etc.

## Response Format
Return JSON exactly matching this schema:
{
  "key_themes": {
    "content": "A paragraph with inline citations [1], [2] describing key themes...",
    "citations": [1, 3, 5]
  },
  "research_trends": {
    "content": "A paragraph with inline citations describing research trends...",
    "citations": [2, 4, 7]
  },
  "methodological_strengths": { ... },
  "common_weaknesses": { ... },
  "gaps_in_literature": { ... },
  "references": [
    {
      "index": 1,
      "title": "Article Title",
      "authors": "Author A, Author B",
      "year": 2024,
      "doi": "10.1234/..."
    }
  ]
}

Rules:
- Cite at least 3 different articles per section when possible.
- Use [number] inline citation format.
- Include a complete references list at the end.
- Reference format: Author(s) (Year). Title. Journal. DOI.
```

#### B.3 Summary Rendering with Citations

**Files to modify:**
- `src/views/summary-view.vue` — Enhanced rendering

```vue
<section class="summary-section">
  <h2>Key Themes</h2>
  <!-- Render content with inline citation links -->
  <div class="summary-section__content" v-html="renderCitations(section.content)" />
</section>

<!-- References section at the bottom -->
<section class="summary-section">
  <h2>References</h2>
  <ol class="reference-list">
    <li v-for="ref in summary.references" :key="ref.index" class="reference-item">
      <button class="reference-item__link" @click="navigateToArticle(ref.articleId)">
        {{ ref.authors }} ({{ ref.year }}). {{ ref.title }}.
        <span v-if="ref.journal">{{ ref.journal }}.</span>
        <span v-if="ref.doi">DOI: {{ ref.doi }}</span>
      </button>
    </li>
  </ol>
</section>
```

**Citation rendering utility:**
```typescript
function renderCitations(content: string): string {
  // Replace [1], [2], etc. with clickable citation links
  return content.replace(/\[(\d+)\]/g, (match, num) => {
    return `<sup><a href="#ref-${num}" class="citation-link">${num}</a></sup>`;
  });
}
```

#### B.4 Summary Integration in PRISMA View

**Files to modify:**
- `src/views/prisma-diagram.vue` — Add "View AI Summary" button and inline summary panel

Add a collapsible summary section below the PRISMA diagram:

```vue
<!-- In prisma-diagram.vue -->
<div class="prisma-summary">
  <button class="btn btn--secondary" @click="showSummary = !showSummary">
    <span class="material-symbols-outlined">summarize</span>
    {{ showSummary ? 'Hide Summary' : 'View AI Summary' }}
  </button>

  <div v-if="showSummary" class="prisma-summary__content">
    <SummaryView :embedded="true" />
  </div>
</div>
```

Make `summary-view.vue` accept an optional `embedded` prop so it can render without its own header when used inside PRISMA:

```vue
defineProps<{
  embedded?: boolean;
}>();
```

#### B.5 Export Summary with PRISMA

**Files to modify:**
- `src/composables/use-export.ts` — Add `exportSummary()` method
- `src/components/export-dialog.vue` — Add "Export Summary" button

Export the enhanced summary as a formatted HTML document that includes:
- PRISMA flow diagram (as embedded SVG)
- Structured AI summary with citations
- Full reference list
- Research aims

**New Tauri command:** `export_summary_html`

---

## Implementation Phases

| Phase | Features | Estimated Effort | Dependencies |
|-------|----------|-----------------|--------------|
| **Phase 1** | 8 (Error Handling & Retry), 12 (Accessibility basics) | 1–2 weeks | None — foundational |
| **Phase 2** | 7 (Undo/Toast System), 9 (Sidebar Enhancements) | 1–2 weeks | Toast system needed for undo |
| **Phase 3** | 3 (Batch Operations), 4 (Filter Presets) | 2–3 weeks | Batch ops need Rust backend |
| **Phase 4** | 11 (Enhanced Screening), A (AI-Generated Criteria) | 2–3 weeks | Rust backend for new events + commands |
| **Phase 5** | 13 (Dark Mode), B (AI Summary with Referencing) | 2–3 weeks | Dark mode is independent; summary needs Rust backend |

---

## Risks & Considerations

| Risk | Mitigation |
|------|------------|
| **Bundle size increase** | Toast, filter presets, and focus trap are lightweight — no external dependencies |
| **Dark mode hardcoded colors** | Incremental migration; start with token-referencing views |
| **Batch operations API** | Requires new Rust backend commands — coordinate with backend development |
| **AI-generated criteria quality** | Show review dialog before committing; user can edit/delete any suggestion |
| **Summary citations** | AI may hallucinate citations — validate references against actual included articles in Rust backend |
| **Feature A & B require LLM** | Both features need an active LLM connection — show appropriate guidance when not configured |
| **Accessibility regression** | Test with screen reader (NVDA/JAWS) after changes |
| **Scope creep** | Each phase should be independently shippable |

---

## File Inventory

### New Files
| File | Feature | Description |
|------|---------|-------------|
| `src/components/bulk-action-bar.vue` | 3 | Sticky bar for bulk article actions |
| `src/components/bulk-tag-dialog.vue` | 3 | Modal for bulk tag/label assignment |
| `src/composables/use-filter-presets.ts` | 4 | Save/load filter preset logic |
| `src/composables/use-toast.ts` | 7 | Global toast notification system |
| `src/components/toast-container.vue` | 7 | Toast rendering component |
| `src/composables/use-network-status.ts` | 8 | Online/offline detection |
| `src/components/error-boundary.vue` | 8 | Error boundary wrapper |
| `src/composables/use-focus-trap.ts` | 12 | Focus trap for modals |
| `src/composables/use-theme.ts` | 13 | Dark/light/system theme management |
| `src/components/screening-decision-feed.vue` | 11 | Live screening decision stream |
| `src/components/criteria-suggest-dialog.vue` | A | Review AI-suggested criteria before adding |

### Modified Files
| File | Features | Changes |
|------|----------|---------|
| `src/components/article-table.vue` | 3, 12 | Add checkboxes, ARIA labels |
| `src/views/article-list.vue` | 3, 4, 11 | Wire bulk actions, filter presets, auto-advance |
| `src/composables/use-article-search.ts` | 3, 4 | Add multi-select state, filter serialization |
| `src/components/article-toolbar.vue` | 3, 4, 13 | Bulk action trigger, preset dropdown, dark mode |
| `src/components/article-detail-panel.vue` | 7, 11, 12 | Undo on status change, auto-advance, ARIA |
| `src/stores/articles.ts` | 3 | Bulk update methods |
| `src/stores/criteria.ts` | A | `suggestCriteria()` method |
| `src/views/criteria-editor.vue` | A, 13 | "Suggest from AI" button, dark mode |
| `src/views/screening-progress.vue` | 8, 11, 12 | Decision feed, retry, ARIA live regions |
| `src/stores/screening.ts` | 11 | Listen for `screening:decision` events |
| `src/types/index.ts` | 11 | Add `ScreeningDecision` type |
| `src/components/nav-sidebar.vue` | 9, 12, 13 | Section grouping, counts, keyboard nav, theme toggle |
| `src/components/app-shell.vue` | 8, 12, 13 | Network banner, skip link, theme init |
| `src/styles/tokens.css` | 13 | Dark mode color tokens |
| `src/styles/base.css` | 12, 13 | `.sr-only` utility, dark theme integration |
| `src/views/summary-view.vue` | B | Citation rendering, embedded mode |
| `src/composables/use-summary.ts` | B | Enhanced summary types |
| `src/views/prisma-diagram.vue` | B | Summary panel integration |
| `src/composables/use-export.ts` | B | Summary HTML export |
| `src/components/export-dialog.vue` | B | "Export Summary" option |
| `src/utils/llm-error.ts` | 8 | Additional error patterns |

### New Rust Backend Commands
| Command | Feature | Description |
|---------|---------|-------------|
| `bulk_update_article_status` | 3 | Update multiple articles' status |
| `bulk_add_tag_to_articles` | 3 | Add tag to multiple articles |
| `bulk_add_label_to_articles` | 3 | Add label to multiple articles |
| `suggest_criteria` | A | AI-generate criteria from research aims |
| `screening:decision` event | 11 | Per-article screening decision event |
| `export_summary_html` | B | Export summary as formatted HTML |

---

## Alignment with v3 Spec

| Spec Section | Features Covered |
|-------------|-----------------|
| §13 Search, Sort, and Filter | Feature 4 (filter presets, URL sync) |
| §7 Article State Machine | Feature 3 (batch status transitions), Feature 7 (undo) |
| §8 Tag and Label Generation | Feature A (follows same AI suggestion pattern) |
| §9 AI Screening Process | Feature 11 (live decision feed, enhanced workflow) |
| §11 AI Summary | Feature B (enhanced with citations and referencing) |
| §12 PRISMA 2020 Flow Diagram | Feature B (summary integration in PRISMA view) |
| §18 UI Design System | Feature 13 (dark mode tokens) |
| §16 Non-Functional Requirements | Feature 12 (accessibility), Feature 8 (error handling) |

---

## Change Log

| Date | Author | Changes |
|------|--------|---------|
| 2026-05-30 | Code Agent | Initial plan — 10 features (8 UX + 2 AI), 5 implementation phases |