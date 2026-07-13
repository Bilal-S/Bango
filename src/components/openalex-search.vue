<script setup lang="ts">
import { onMounted, ref } from 'vue';

import { useOpenAlexStore } from '@/stores/openalex';
import { useToast } from '@/composables/use-toast';
import { SORT_OPTIONS, PER_PAGE_OPTIONS } from '@/types/openalex';
import OpenAlexResultItem from './openalex-result-item.vue';
import OpenAlexDetailPanel from './openalex-detail-panel.vue';

const store = useOpenAlexStore();
const toast = useToast();
const importingSingle = ref(false);

onMounted(async () => {
  await Promise.all([store.loadSettings(), store.checkSmartSearchAvailability()]);
});

async function handleSearch(): Promise<void> {
  await store.search();
}

function handleClear(): void {
  store.clearSearch();
}

async function handleImport(): Promise<void> {
  const result = await store.importSelected();
  if (result) {
    const working = result.importedCount - result.skippedCount;
    const dupes = result.skippedCount;
    const msg =
      dupes > 0
        ? `Added ${working} article(s) to Working, ${dupes} to Duplicates`
        : `Added ${working} article(s) to Working list`;
    toast.show(msg, 'success');
  }
}

async function handleAddSingle(): Promise<void> {
  if (!store.selectedResult) return;
  importingSingle.value = true;
  try {
    const result = await store.importSingle(store.selectedResult.work.id);
    if (result) {
      toast.show('Article added to Working list.', 'success');
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
          <input
            v-model="store.query"
            type="text"
            placeholder="Search OpenAlex..."
            class="search-input"
            @keydown.enter="handleSearch"
          />
          <button class="btn btn--primary" :disabled="store.loading" @click="handleSearch">
            {{ store.loading ? 'Searching...' : 'Search' }}
          </button>
          <button class="btn btn--secondary" @click="handleClear">Clear</button>
        </div>

        <!-- Smart Search button (Tier 2) -->
        <button
          v-if="store.smartSearchAvailable"
          class="btn btn--accent mt-2"
          :disabled="store.smartSearchLoading || store.loading"
          @click="store.smartSearch()"
        >
          {{ store.smartSearchLoading ? 'Generating...' : 'Smart Search' }}
        </button>
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
              :checked="store.selectedCount === store.results.length && store.results.length > 0"
              @change="
                store.selectedCount === store.results.length
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
              :disabled="store.selectedCount === 0"
              @click="handleImport"
            >
              Add to Working
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

.search-input {
  flex: 1;
  padding: 0.5rem 0.75rem;
  border: 1px solid #cbd5e1;
  border-radius: 0.375rem;
  font-size: 0.875rem;
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
</style>
