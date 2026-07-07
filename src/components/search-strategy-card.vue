<script setup lang="ts">
import { ref } from 'vue';
import { useToast } from '@/composables/use-toast';
import type {
  SearchStrategyResult,
  StrategiesByDatabase,
  DatabaseStrategy,
  ConceptBlock,
} from '@/types/search-strategy';

const props = defineProps<{ result: SearchStrategyResult }>();
const emit = defineEmits<{ dismiss: [] }>();

const toast = useToast();

/** Collapsed state for the whole card body. Expanded by default on first render. */
const expanded = ref(true);

/** Collapsed state for the PICO concept table. Collapsed by default. */
const picoExpanded = ref(false);

/** Ordered list of databases for stable grid rendering. Each entry carries a
 * human-readable label + the snake/camel key from `StrategiesByDatabase`. */
const DATABASES: ReadonlyArray<{ key: keyof StrategiesByDatabase; label: string }> = [
  { key: 'pubmed', label: 'PubMed' },
  { key: 'scopus', label: 'Scopus' },
  { key: 'webOfScience', label: 'Web of Science' },
  { key: 'cochrane', label: 'Cochrane Library' },
  { key: 'ebscohost', label: 'EBSCOhost' },
  { key: 'jstor', label: 'JSTOR' },
  { key: 'sciencedirect', label: 'ScienceDirect' },
  { key: 'arxiv', label: 'arXiv' },
];

/** PICO arms in display order; nulls are filtered out of the rendered table. */
const picoArms: ReadonlyArray<{ label: string; block: ConceptBlock | null }> = [
  { label: 'Population', block: props.result.picoBreakdown.population },
  { label: 'Intervention', block: props.result.picoBreakdown.intervention },
  { label: 'Comparison', block: props.result.picoBreakdown.comparison },
  { label: 'Outcome', block: props.result.picoBreakdown.outcome },
];

async function copy(label: string, text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.show(`Copied ${label} search string`, 'success');
  } catch {
    toast.show('Failed to copy to clipboard', 'error');
  }
}
</script>

<template>
  <div class="ai-critique-card search-strategy-card">
    <div class="ai-critique-card__header">
      <div class="ai-critique-card__title-group">
        <span class="material-symbols-outlined">auto_awesome</span>
        <span class="ai-critique-card__title">Suggested Search Strategy</span>
      </div>
      <div class="search-strategy-card__header-actions">
        <button
          class="search-strategy-card__toggle"
          :title="expanded ? 'Collapse' : 'Expand'"
          @click="expanded = !expanded"
        >
          <span class="material-symbols-outlined">{{
            expanded ? 'expand_less' : 'expand_more'
          }}</span>
        </button>
        <button class="ai-critique-card__dismiss" title="Dismiss" @click="emit('dismiss')">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
    </div>

    <div v-if="expanded" class="ai-critique-card__body">
      <p class="search-strategy-card__hint">
        Database-specific Boolean search strings generated from your research aims. These are
        starting points - verify field codes and syntax in each database's search interface before
        running the final search.
      </p>

      <div class="search-strategy-card__grid">
        <div v-for="db in DATABASES" :key="db.key" class="search-strategy-card__db">
          <div class="search-strategy-card__db-header">
            <span class="search-strategy-card__db-label">{{ db.label }}</span>
            <button
              class="search-strategy-card__copy"
              title="Copy search string"
              @click="copy(db.label, (result.strategies[db.key] as DatabaseStrategy).oneLine)"
            >
              <span class="material-symbols-outlined">content_copy</span>
              Copy
            </button>
          </div>
          <pre class="search-strategy-card__pre">{{
            (result.strategies[db.key] as DatabaseStrategy).oneLine
          }}</pre>
          <p
            v-if="(result.strategies[db.key] as DatabaseStrategy).notes"
            class="search-strategy-card__notes"
          >
            {{ (result.strategies[db.key] as DatabaseStrategy).notes }}
          </p>
        </div>
      </div>

      <button
        v-if="picoArms.some((a) => a.block !== null)"
        class="search-strategy-card__pico-toggle"
        @click="picoExpanded = !picoExpanded"
      >
        <span class="material-symbols-outlined">{{
          picoExpanded ? 'expand_less' : 'expand_more'
        }}</span>
        PICO Concept Table
      </button>
      <div v-if="picoExpanded" class="search-strategy-card__pico">
        <div
          v-for="arm in picoArms.filter((a) => a.block !== null)"
          :key="arm.label"
          class="search-strategy-card__pico-row"
        >
          <span class="search-strategy-card__pico-label">{{ arm.label }}</span>
          <div class="search-strategy-card__pico-body">
            <strong>{{ arm.block?.concept }}</strong>
            <span
              v-if="arm.block?.synonyms && arm.block.synonyms.length > 0"
              class="search-strategy-card__pico-synonyms"
            >
              {{ arm.block.synonyms.join(', ') }}
            </span>
          </div>
        </div>
      </div>

      <div
        v-if="result.warnings && result.warnings.length > 0"
        class="search-strategy-card__warnings"
      >
        <div v-for="(w, i) in result.warnings" :key="i" class="search-strategy-card__warning">
          <span class="material-symbols-outlined">warning</span>
          <span>{{ w.message }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Re-declare the `.ai-critique-card__*` layout/typography rules locally.
 * Vue scoped CSS does not cross into child component internals (only the
 * child's root inherits the parent's scope), so the classes from
 * `criteria-editor.vue`'s scoped block do not reach these elements. Values
 * are identical so the card stays visually consistent with the
 * inclusion/exclusion critique cards. */
.ai-critique-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.ai-critique-card__title-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: #6b21a8;
}

.ai-critique-card__title-group .material-symbols-outlined {
  font-size: 20px;
}

.ai-critique-card__title {
  font-size: 14px;
  font-weight: 600;
}

.ai-critique-card__dismiss {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  display: flex;
  align-items: center;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.ai-critique-card__dismiss:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.ai-critique-card__dismiss .material-symbols-outlined {
  font-size: 18px;
}

.ai-critique-card__body {
  font-size: 14px;
  line-height: 22px;
  color: #1b1b24;
}

.search-strategy-card__header-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.search-strategy-card__toggle {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  display: flex;
  align-items: center;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.search-strategy-card__toggle:hover {
  color: #6b21a8;
  background-color: #ede9fe;
}

.search-strategy-card__toggle .material-symbols-outlined {
  font-size: 20px;
}

.search-strategy-card__hint {
  font-size: 12px;
  color: #6b7280;
  margin-bottom: 1rem;
  font-style: italic;
}

/* 2-column database grid; collapses to 1 column on narrow viewports. */
.search-strategy-card__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.75rem;
}

@media (max-width: 767px) {
  .search-strategy-card__grid {
    grid-template-columns: 1fr;
  }
}

.search-strategy-card__db {
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.5rem;
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.search-strategy-card__db-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.search-strategy-card__db-label {
  font-size: 13px;
  font-weight: 600;
  color: #1b1b24;
}

.search-strategy-card__copy {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  background-color: #e8def8;
  color: #4a1564;
  font-size: 11px;
  font-weight: 600;
  padding: 0.25rem 0.5rem;
  border: 1px solid #c8aee6;
  border-radius: 0.25rem;
  cursor: pointer;
  white-space: nowrap;
  transition: background-color 0.15s;
}

.search-strategy-card__copy:hover {
  background-color: #d8c8f0;
}

.search-strategy-card__copy .material-symbols-outlined {
  font-size: 14px;
}

.search-strategy-card__pre {
  margin: 0;
  padding: 0.5rem;
  background-color: #f8fafc;
  border-radius: 0.375rem;
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 11px;
  line-height: 1.5;
  color: #1b1b24;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 8rem;
  overflow-y: auto;
}

.search-strategy-card__notes {
  margin: 0;
  font-size: 11px;
  line-height: 1.4;
  color: #6b7280;
}

.search-strategy-card__pico-toggle {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  cursor: pointer;
  color: #6b21a8;
  font-size: 13px;
  font-weight: 600;
  padding: 0.5rem 0;
  margin-top: 0.75rem;
}

.search-strategy-card__pico-toggle .material-symbols-outlined {
  font-size: 18px;
}

.search-strategy-card__pico {
  margin-top: 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.search-strategy-card__pico-row {
  display: grid;
  grid-template-columns: 8rem minmax(0, 1fr);
  gap: 0.75rem;
  padding: 0.5rem;
  background-color: #ffffff;
  border-radius: 0.375rem;
  border: 1px solid #e2e8f0;
}

@media (max-width: 767px) {
  .search-strategy-card__pico-row {
    grid-template-columns: 1fr;
    gap: 0.25rem;
  }
}

.search-strategy-card__pico-label {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #6b21a8;
}

.search-strategy-card__pico-body {
  font-size: 12px;
  line-height: 1.5;
  color: #1b1b24;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.search-strategy-card__pico-synonyms {
  color: #6b7280;
  font-size: 11px;
}

.search-strategy-card__warnings {
  margin-top: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.search-strategy-card__warning {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  background-color: #fffbeb;
  border: 1px solid #fcd34d;
  border-radius: 0.375rem;
  font-size: 12px;
  line-height: 1.5;
  color: #92400e;
}

.search-strategy-card__warning .material-symbols-outlined {
  font-size: 16px;
  color: #d97706;
  flex-shrink: 0;
  margin-top: 0.0625rem;
}
</style>
