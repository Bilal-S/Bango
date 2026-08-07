<script setup lang="ts">
import { onMounted, ref, computed, watch } from 'vue';

import { useOpenAlexStore } from '@/stores/openalex';
import { useToast } from '@/composables/use-toast';
import {
  SORT_OPTIONS,
  PER_PAGE_OPTIONS,
  WORK_TYPE_OPTIONS,
  LANGUAGE_OPTIONS,
  DEFAULT_OPENALEX_FILTERS,
  type OpenAlexFilters,
} from '@/types/openalex';
import ClearableInput from '@/components/clearable-input.vue';
import OpenAlexResultItem from './openalex-result-item.vue';
import OpenAlexDetailPanel from './openalex-detail-panel.vue';

const store = useOpenAlexStore();
const toast = useToast();
const importingSingle = ref(false);
/** True while `importSelected` is in flight so the bulk button can show
 * "Adding..." and disable, preventing double-clicks. */
const importing = ref(false);

const emit = defineEmits<{
  /** Emitted after a successful import so the parent can refresh status-tab counts. */
  imported: [];
}>();

onMounted(async () => {
  /* `smartSearchAvailable` is now a reactive computed over `useLlmConfigured()`,
     so there is no availability probe to call on mount - the gate tracks the
     Pinia store automatically. */
  await store.loadSettings();
});

async function handleSearch(): Promise<void> {
  await store.search();
}

function handleClear(): void {
  store.clearSearch();
}

// ── Search Options panel ────────────────────────────────────────────────
/* Collapsible panel exposing the OpenAlexFilters dimensions the backend
 * already supports (work type, year range, language, OA, retracted) but the
 * UI previously did not. All defaults preserve the prior search behavior. */
const showOptions = ref(false);
/* Local editable copy of the store filters; committed on Apply so partial
 * edits don't trigger intermediate re-searches. Seeded from the store at
 * setup so committed filters (prior session, Smart Search, deep-link) are
 * reflected immediately in both the panel controls and the collapsed-header
 * count. A watcher re-syncs from the store only while the panel is collapsed
 * so Smart Search (which writes store.filters directly) still flows in, but
 * uncommitted panel edits survive a collapse/re-expand. Mirrors the
 * `article-metadata.vue` "Metadata" box pattern (DOM persists via v-show). */
const panelFilters = ref<OpenAlexFilters>({
  ...store.filters,
  workTypes: [...store.filters.workTypes],
});
watch(
  () => store.filters,
  (f) => {
    if (!showOptions.value) {
      panelFilters.value = { ...f, workTypes: [...f.workTypes] };
    }
  },
  { deep: true }
);

/** Year-range bounds + validation (mirrors article-filter-panel conventions). */
const YEAR_MIN = 1900;
const YEAR_MAX = 2100;

const yearFromInvalid = computed((): boolean => {
  const from = panelFilters.value.yearFrom;
  const to = panelFilters.value.yearTo;
  if (from !== null && (from < YEAR_MIN || from > YEAR_MAX)) return true;
  return from !== null && to !== null && from > to;
});

const yearToInvalid = computed((): boolean => {
  const from = panelFilters.value.yearFrom;
  const to = panelFilters.value.yearTo;
  if (to !== null && (to < YEAR_MIN || to > YEAR_MAX)) return true;
  return from !== null && to !== null && from > to;
});

const yearRangeInvalid = computed((): boolean => yearFromInvalid.value || yearToInvalid.value);

const yearHint = computed((): string => {
  const from = panelFilters.value.yearFrom;
  const to = panelFilters.value.yearTo;
  if (from !== null && to !== null && from > to) return 'From year must be <= To year.';
  if (from !== null && (from < YEAR_MIN || from > YEAR_MAX))
    return `From year must be between ${YEAR_MIN}-${YEAR_MAX}.`;
  if (to !== null && (to < YEAR_MIN || to > YEAR_MAX))
    return `To year must be between ${YEAR_MIN}-${YEAR_MAX}.`;
  return '';
});

/** Count of non-default option dimensions active in the panel (in-progress
 * edits), shown in the panel's action row. */
const activeOptionCount = computed((): number => {
  const f = panelFilters.value;
  return countActiveOptions(f);
});

function countActiveOptions(f: OpenAlexFilters): number {
  let n = 0;
  if (f.workTypes.length > 0) n += 1;
  if (f.yearFrom !== null || f.yearTo !== null) n += 1;
  if (f.language !== null) n += 1;
  if (f.isOa) n += 1;
  if (f.showRetracted) n += 1;
  return n;
}

function toggleOptions(): void {
  showOptions.value = !showOptions.value;
}

function toggleWorkType(value: string): void {
  const current = panelFilters.value.workTypes;
  panelFilters.value = {
    ...panelFilters.value,
    workTypes: current.includes(value) ? current.filter((t) => t !== value) : [...current, value],
  };
}

function applyOptions(): void {
  if (yearRangeInvalid.value) return;
  store.setFilters({ ...panelFilters.value, workTypes: [...panelFilters.value.workTypes] });
}

/** Block `e`/`E`/`+`/`-`/`.` in year inputs so only digits (and editing keys)
 * are accepted. `ClearableInput` has a single-root div without a `keydown`
 * emit, so this native listener falls through to the inner `<input>`. */
function onYearKeydown(event: KeyboardEvent): void {
  if (['e', 'E', '+', '-', '.'].includes(event.key)) {
    event.preventDefault();
  }
}

function clearOptions(): void {
  panelFilters.value = { ...DEFAULT_OPENALEX_FILTERS, workTypes: [] };
  store.setFilters({ ...DEFAULT_OPENALEX_FILTERS, workTypes: [] });
}

async function handleImport(): Promise<void> {
  importing.value = true;
  try {
    const result = await store.importSelected();
    if (result) {
      const working = result.importedCount - result.skippedCount;
      const dupes = result.skippedCount;
      const msg =
        dupes > 0
          ? `Added ${working} article(s) to Working, ${dupes} to Duplicates`
          : `Added ${working} article(s) to Working list`;
      toast.show(msg, 'success');
      emit('imported');
    }
  } finally {
    importing.value = false;
  }
}

async function handleAddSingle(): Promise<void> {
  if (!store.selectedResult) return;
  importingSingle.value = true;
  try {
    const result = await store.importSingle(store.selectedResult.work.id);
    if (result) {
      toast.show('Article added to Working list.', 'success');
      emit('imported');
    }
  } catch (e: unknown) {
    toast.show(`Failed to add article: ${e}`, 'error');
  } finally {
    importingSingle.value = false;
  }
}
</script>

<template>
  <div class="openalex-search" :class="{ 'openalex-search--split': store.selectedResult }">
    <!-- Main column: search bar + results -->
    <div class="search-main">
      <!-- Search Bar -->
      <div class="search-bar">
        <div class="search-input-row">
          <div class="search-input-wrap">
            <ClearableInput
              :model-value="store.query"
              placeholder="Search OpenAlex..."
              input-class="search-input"
              @update:model-value="store.query = $event"
              @clear="store.query = ''"
              @enter="handleSearch"
            />
          </div>
          <button class="btn btn--primary" :disabled="store.loading" @click="handleSearch">
            {{ store.loading ? 'Searching...' : 'Search' }}
          </button>
          <button class="btn btn--secondary" :disabled="store.loading" @click="handleClear">
            Clear
          </button>
          <!-- Smart Search: accent button, same size as Search/Clear (LLM-gated). -->
          <button
            v-if="store.smartSearchAvailable"
            class="btn btn--accent oa-smart-search"
            :disabled="store.smartSearchLoading || store.loading"
            title="Generate an OpenAlex Boolean query from your research aims + criteria"
            @click="store.smartSearch()"
          >
            <span class="material-symbols-outlined text-[16px]">auto_awesome</span>
            {{ store.smartSearchLoading ? 'Generating...' : 'Smart Search' }}
          </button>
        </div>

        <!-- Collapsible SEARCH OPTIONS box. Mirrors the `article-metadata.vue`
             "Metadata" box: border+rounded container, header row, v-show body
             (DOM persists so uncommitted panel edits survive collapse). -->
        <div class="options-box border border-slate-200 rounded overflow-hidden">
          <button
            type="button"
            class="options-header relative w-full flex items-center justify-between px-3 py-2 text-xs font-label-caps text-slate-500 uppercase tracking-wider hover:bg-slate-50 cursor-pointer transition-colors"
            :aria-expanded="showOptions"
            @click="toggleOptions"
          >
            <span class="shrink-0">Search Options</span>
            <!-- Active-option count: centered in the header, shown only when
                 collapsed and at least one option is selected in the panel. -->
            <span
              v-if="!showOptions && activeOptionCount > 0"
              class="options-header__count absolute left-1/2 -translate-x-1/2 text-[11px] text-indigo-600 normal-case tracking-normal font-medium whitespace-nowrap"
            >
              {{ activeOptionCount }} option{{ activeOptionCount === 1 ? '' : 's' }} selected
            </span>
            <span
              class="material-symbols-outlined text-[16px] text-slate-400 transition-transform duration-200 shrink-0"
              :class="{ 'rotate-180': showOptions }"
            >
              expand_more
            </span>
          </button>

          <!-- Panel body (v-show so edits persist across collapse). -->
          <div v-show="showOptions" class="options-body px-3 pb-3 space-y-3">
            <!-- Work Type chips -->
            <div class="options-section">
              <label class="options-label">Work Type</label>
              <div class="options-chips">
                <button
                  v-for="opt in WORK_TYPE_OPTIONS"
                  :key="opt.value"
                  type="button"
                  class="chip"
                  :class="{ 'chip--on': panelFilters.workTypes.includes(opt.value) }"
                  @click="toggleWorkType(opt.value)"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- One row: Year + Language + OA + Retracted. Wraps only when the
                 viewport is too narrow (flex-wrap). -->
            <div class="options-inline-row">
              <div class="options-section options-inline-group">
                <label class="options-label">Publication Year</label>
                <div class="options-year-row">
                  <ClearableInput
                    :model-value="
                      panelFilters.yearFrom !== null ? String(panelFilters.yearFrom) : ''
                    "
                    type="number"
                    :min="YEAR_MIN"
                    :max="YEAR_MAX"
                    placeholder="From"
                    :input-class="`options-year-input ${yearFromInvalid ? 'options-input--invalid' : ''}`"
                    @update:model-value="
                      panelFilters.yearFrom = $event === '' ? null : Number($event)
                    "
                    @keydown="onYearKeydown"
                  />
                  <span class="options-year-sep">&ndash;</span>
                  <ClearableInput
                    :model-value="panelFilters.yearTo !== null ? String(panelFilters.yearTo) : ''"
                    type="number"
                    :min="YEAR_MIN"
                    :max="YEAR_MAX"
                    placeholder="To"
                    :input-class="`options-year-input ${yearToInvalid ? 'options-input--invalid' : ''}`"
                    @update:model-value="
                      panelFilters.yearTo = $event === '' ? null : Number($event)
                    "
                    @keydown="onYearKeydown"
                  />
                </div>
              </div>

              <div class="options-section options-inline-group">
                <label class="options-label">Language</label>
                <select
                  class="options-select"
                  :value="panelFilters.language ?? ''"
                  @change="
                    panelFilters.language = ($event.target as HTMLSelectElement).value || null
                  "
                >
                  <option
                    v-for="opt in LANGUAGE_OPTIONS"
                    :key="opt.value ?? 'any'"
                    :value="opt.value ?? ''"
                  >
                    {{ opt.label }}
                  </option>
                </select>
              </div>

              <label class="options-toggle options-inline-group">
                <input v-model="panelFilters.isOa" type="checkbox" />
                <span class="options-switch-track">
                  <span class="options-switch-thumb"></span>
                </span>
                <span>Open access only</span>
              </label>

              <label class="options-toggle options-inline-group">
                <input v-model="panelFilters.showRetracted" type="checkbox" />
                <span class="options-switch-track">
                  <span class="options-switch-thumb"></span>
                </span>
                <span>Include retracted</span>
              </label>
            </div>

            <p v-if="yearRangeInvalid" class="options-hint">{{ yearHint }}</p>

            <!-- Action row -->
            <div class="options-actions">
              <span v-if="activeOptionCount > 0" class="options-active-notice">
                {{ activeOptionCount }} option{{ activeOptionCount === 1 ? '' : 's' }} active
              </span>
              <span v-else class="options-active-notice options-active-notice--idle"
                >No options active</span
              >
              <button class="btn btn--secondary btn--sm" type="button" @click="clearOptions">
                Clear options
              </button>
              <button
                class="btn btn--primary btn--sm"
                type="button"
                :disabled="yearRangeInvalid"
                @click="applyOptions"
              >
                Apply
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Sort + Pagination Controls -->
      <div v-if="store.hasSearched" class="controls-bar">
        <select v-model="store.sortBy" class="sort-select" @change="store.setSort(store.sortBy)">
          <option v-for="opt in SORT_OPTIONS" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>

        <select
          :value="store.perPage"
          class="per-page-select"
          @change="store.setPerPage(Number(($event.target as HTMLSelectElement).value))"
        >
          <option v-for="size in PER_PAGE_OPTIONS" :key="size" :value="size">
            {{ size }} per page
          </option>
        </select>

        <span class="result-count">
          Results: {{ store.cappedTotalCount
          }}{{ store.totalCount > 1000 ? ` of ${store.totalCount}` : '' }}
        </span>

        <div class="pagination">
          <button
            class="btn btn--sm"
            :disabled="store.currentPage <= 1"
            @click="store.goToPage(store.currentPage - 1)"
          >
            Prev
          </button>
          <span>Page {{ store.currentPage }} of {{ store.totalPages }}</span>
          <button
            class="btn btn--sm"
            :disabled="store.currentPage >= store.totalPages"
            @click="store.goToPage(store.currentPage + 1)"
          >
            Next
          </button>
        </div>
      </div>

      <!-- Error -->
      <div v-if="store.error" class="error-card">
        <p>{{ store.error }}</p>
        <button class="btn btn--secondary" @click="handleSearch">Retry</button>
      </div>

      <!-- Loading -->
      <div v-if="store.loading" class="loading-state">
        <div v-for="i in 5" :key="i" class="skeleton-row"></div>
      </div>

      <!-- Empty State -->
      <div v-if="!store.loading && !store.hasSearched && !store.error" class="empty-state">
        <p>Search OpenAlex's catalog of 300M+ scholarly works</p>
      </div>

      <!-- No Results -->
      <div
        v-if="!store.loading && store.hasSearched && store.results.length === 0 && !store.error"
        class="no-results"
      >
        <p>No works found matching your search. Try different keywords or adjust filters.</p>
      </div>

      <!-- Results List -->
      <div v-if="!store.loading && store.results.length > 0" class="results-list">
        <!-- Sticky action bar: always visible, buttons disabled when 0 selected -->
        <div class="action-bar">
          <label class="action-bar__select">
            <input
              type="checkbox"
              :checked="store.selectableCount > 0 && store.selectedCount === store.selectableCount"
              :disabled="store.selectableCount === 0"
              @change="
                store.selectedCount === store.selectableCount
                  ? store.clearSelection()
                  : store.selectAll()
              "
            />
            <span v-if="store.selectedCount > 0">{{ store.selectedCount }} selected</span>
            <span v-else>Select All</span>
          </label>
          <div class="action-bar__buttons">
            <button
              class="btn btn--primary btn--sm"
              :disabled="store.selectedCount === 0 || importing"
              @click="handleImport"
            >
              {{ importing ? 'Adding...' : 'Add to Working' }}
            </button>
            <button
              class="btn btn--clear-selection btn--sm"
              :disabled="store.selectedCount === 0"
              @click="store.clearSelection()"
            >
              Clear
            </button>
          </div>
        </div>

        <OpenAlexResultItem
          v-for="item in store.results"
          :key="item.work.id"
          :item="item"
          :selected="store.selectedIds.has(item.work.id)"
          :detail-open="store.selectedResultId === item.work.id"
          @toggle-select="store.toggleSelection(item.work.id)"
          @open-detail="store.selectResult(item.work.id)"
        />
      </div>
    </div>

    <!-- Detail Panel (split, not overlay) -->
    <aside v-if="store.selectedResult" class="search-detail">
      <OpenAlexDetailPanel
        :item="store.selectedResult"
        :importing="importingSingle"
        @close="store.selectResult(null)"
        @add="handleAddSingle"
      />
    </aside>
  </div>
</template>

<style scoped>
.openalex-search {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.openalex-search--split {
  flex-direction: row;
  gap: 0;
  height: 100%;
  overflow: hidden;
}

.search-main {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}

.openalex-search--split .search-main {
  flex: 1;
}

.search-detail {
  width: 420px;
  flex-shrink: 0;
  height: 100%;
  overflow: hidden;
}

@media (max-width: 1023px) {
  .openalex-search--split {
    flex-direction: column;
  }

  .search-detail {
    width: 100%;
    height: auto;
    max-height: 60vh;
  }
}

.search-bar {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.search-input-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.search-input-wrap {
  flex: 1 1 auto;
  min-width: 0;
}

/* The main search input is the inner <input> of ClearableInput. The component
   hardcodes `focus:ring-2 focus:border-transparent` (an outside-extending
   box-shadow) which gets clipped by the surrounding flex row on the top/left
   edges. Override via :deep() with higher specificity: drop the ring and use
   an inside-the-box border-color focus that can't be clipped. Also restore
   `rounded-md` to match the previous native input corners. */
.search-input-wrap :deep(input.search-input) {
  border-radius: 0.375rem;
}
.search-input-wrap :deep(input.search-input:focus) {
  border-color: #6366f1;
  box-shadow: none;
  outline: none;
}

.controls-bar {
  display: flex;
  align-items: center;
  gap: 1rem;
  font-size: 0.8125rem;
  color: #64748b;
}

.sort-select,
.per-page-select {
  padding: 0.25rem 0.5rem;
  border: 1px solid #cbd5e1;
  border-radius: 0.25rem;
  font-size: 0.8125rem;
}

.pagination {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-left: auto;
}

.error-card {
  background: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 0.375rem;
  padding: 1rem;
  color: #991b1b;
}

.loading-state {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.skeleton-row {
  height: 5rem;
  background: linear-gradient(90deg, #f1f5f9 25%, #e2e8f0 50%, #f1f5f9 75%);
  background-size: 200% 100%;
  animation: pulse 1.5s infinite;
  border-radius: 0.375rem;
}

@keyframes pulse {
  0% {
    background-position: 200% 0;
  }
  100% {
    background-position: -200% 0;
  }
}

.empty-state,
.no-results {
  text-align: center;
  padding: 3rem 1rem;
  color: #94a3b8;
}

.results-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

/* Sticky action bar: always visible at top of results list */
.action-bar {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0.75rem;
  background: white;
  border-bottom: 1px solid #e2e8f0;
  border-radius: 0.375rem 0.375rem 0 0;
  font-size: 0.8125rem;
  color: #64748b;
}

.action-bar__select {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  cursor: pointer;
  user-select: none;
}

.action-bar__buttons {
  display: flex;
  gap: 0.375rem;
}

.btn {
  padding: 0.5rem 1rem;
  font-size: 0.8125rem;
  font-weight: 500;
  border-radius: 0.375rem;
  border: 1px solid #cbd5e1;
  cursor: pointer;
  transition: background 0.15s;
  white-space: nowrap;
}

.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn--primary {
  background: #4f46e5;
  color: white;
  border-color: #4f46e5;
}

.btn--primary:hover:not(:disabled) {
  background: #4338ca;
}

.btn--secondary {
  background: white;
  color: #334155;
}

.btn--secondary:hover:not(:disabled) {
  background: #f1f5f9;
}

.btn--accent {
  background: #e8def8;
  color: #4a1564;
  border-color: #c8aee6;
}

.btn--accent:hover:not(:disabled) {
  background: #d8c8f0;
}

/* Clear Selection: light red fill */
.btn--clear-selection {
  background: #fef2f2;
  color: #dc2626;
  border-color: #fecaca;
}

.btn--clear-selection:hover {
  background: #fee2e2;
  border-color: #fca5a5;
}

.btn--sm {
  padding: 0.25rem 0.5rem;
  font-size: 0.75rem;
}

.mt-2 {
  margin-top: 0.5rem;
}

/* Smart Search button: icon + label inline (same size as Search/Clear). */
.oa-smart-search {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
}

/* The header's structural classes (w-full flex items-center justify-between
   px-3 py-2 ...) come from Tailwind utilities on the element; the `.options-header`
   class is kept only as a test hook + styling anchor. No scoped overrides needed
   beyond the base reset so the button doesn't inherit default form styles. */
.options-header {
  background: transparent;
}

/* Panel body: solid white background when expanded so the form fields read
   clearly against the surrounding UI. The header keeps its transparent/
   hover-slate-50 background. */
.options-body {
  background: white;
}

.options-section {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.options-label {
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #64748b;
}

/* Work-type chips. */
.options-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
}

.chip {
  padding: 0.25rem 0.6rem;
  font-size: 0.75rem;
  font-weight: 500;
  color: #475569;
  background: white;
  border: 1px solid #cbd5e1;
  border-radius: 9999px;
  cursor: pointer;
  transition:
    background 0.15s,
    border-color 0.15s,
    color 0.15s;
}

.chip:hover {
  background: #f8fafc;
}

.chip--on {
  background: #e8def8;
  border-color: #c8aee6;
  color: #4a1564;
}

.chip--on:hover {
  background: #d8c8f0;
}

/* Year range inputs. */
.options-year-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.options-year-input {
  width: 5.5rem;
  text-align: center;
  font-variant-numeric: tabular-nums;
}

.options-year-sep {
  color: #94a3b8;
  font-size: 0.8rem;
}

.options-input--invalid :deep(input),
.options-input--invalid {
  border-color: #fca5a5;
}

.options-hint {
  margin-top: 0.25rem;
  font-size: 0.7rem;
  color: #ef4444;
}

/* Single inline row: Year + Language + OA + Retracted all on one level by
   default. Wraps only when the viewport can't fit them. Each group aligns
   to the bottom so labels + controls line up despite differing heights. */
.options-inline-row {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 1.5rem;
}

.options-inline-group {
  flex: 0 0 auto;
}

.options-select {
  width: 9rem;
  max-width: 100%;
  padding: 0.3rem 0.5rem;
  border: 1px solid #cbd5e1;
  border-radius: 0.375rem;
  font-size: 0.8rem;
  background: white;
  outline: none;
}

/* Switch toggles (OA + Retracted): bigger hit area, clear on/off state.
   The native checkbox is visually hidden but kept in the label so the
   existing test selector `label.options-toggle input[type="checkbox"]`
   still works. */
.options-toggle {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8rem;
  color: #334155;
  cursor: pointer;
  user-select: none;
}

.options-toggle input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
}

.options-switch-track {
  position: relative;
  width: 2.5rem;
  height: 1.4rem;
  background: #cbd5e1;
  border-radius: 9999px;
  transition: background 0.15s;
  flex-shrink: 0;
}

.options-switch-thumb {
  position: absolute;
  top: 0.15rem;
  left: 0.15rem;
  width: 1.1rem;
  height: 1.1rem;
  background: white;
  border-radius: 9999px;
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
  transition: left 0.15s;
}

.options-toggle input:checked + .options-switch-track {
  background: #4f46e5;
}

.options-toggle input:checked + .options-switch-track .options-switch-thumb {
  left: calc(100% - 1.25rem);
}

.options-toggle input:focus-visible + .options-switch-track {
  outline: 2px solid #4f46e5;
  outline-offset: 2px;
}

/* Action row. */
.options-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding-top: 0.75rem;
  border-top: 1px solid #f1f5f9;
}

.options-active-notice {
  flex: 1;
  font-size: 0.7rem;
  font-weight: 600;
  color: #4338ca;
}

.options-active-notice--idle {
  color: #94a3b8;
  font-weight: 500;
}
</style>
