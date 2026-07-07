<script setup lang="ts">
import { ref, computed } from 'vue';
import { marked } from 'marked';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigStore } from '@/stores/llm-config';
import { formatLlmError } from '@/utils/llm-error';
import type { SearchStrategyResult } from '@/types/search-strategy';
import SearchStrategyCard from '@/components/search-strategy-card.vue';
import type { Priority } from '@/types';

const criteriaStore = useCriteriaStore();
const llmConfigStore = useLlmConfigStore();

// Pre-warm LLM config if not already loaded
llmConfigStore.fetchIfNeeded();

// Read directly from store - pre-warmed at startup, no onMounted fetch needed.
const aims = computed(() => criteriaStore.aims);
const criteria = computed(() => criteriaStore.criteria);

const newAimText = ref('');
const newInclusionText = ref('');
const newExclusionText = ref('');
const newInclusionPriority = ref<Priority>('standard');
const newExclusionPriority = ref<Priority>('standard');

// AI assistant state lives in the Pinia store (persists across route navigation)
const generatingInclusion = computed(() => criteriaStore.generatingInclusion);
const generatingExclusion = computed(() => criteriaStore.generatingExclusion);
const inclusionCritiqueText = computed(() => criteriaStore.inclusionCritique);
const exclusionCritiqueText = computed(() => criteriaStore.exclusionCritique);
const inclusionError = computed(() => criteriaStore.inclusionError);
const exclusionError = computed(() => criteriaStore.exclusionError);

// Search Strategy Builder state (session-scoped, mirrors the critique refs).
const generatingSearchStrategy = computed(() => criteriaStore.generatingSearchStrategy);
const searchStrategyResult = computed<SearchStrategyResult | null>(
  () => criteriaStore.searchStrategyResult
);
const searchStrategyError = computed(() => criteriaStore.searchStrategyError);

const inclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'inclusion')
);
const exclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'exclusion')
);

/** Global criterion numbering: inclusion [1]..[N], exclusion [N+1]..[N+M] */
const criterionIndexMap = computed(() => {
  const map = new Map<string, number>();
  let n = 1;
  for (const c of inclusionCriteria.value) {
    map.set(c.id, n++);
  }
  for (const c of exclusionCriteria.value) {
    map.set(c.id, n++);
  }
  return map;
});

async function refetch(): Promise<void> {
  await criteriaStore.refresh();
}

async function addAim(): Promise<void> {
  if (!newAimText.value.trim()) return;
  await tauriCommand('create_research_aim', {
    request: { text: newAimText.value.trim() },
  });
  newAimText.value = '';
  await refetch();
}

async function deleteAim(id: string): Promise<void> {
  await tauriCommand('delete_research_aim', { id });
  await refetch();
}

async function addInclusion(): Promise<void> {
  if (!newInclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: {
      criterionType: 'inclusion',
      text: newInclusionText.value.trim(),
      priority: newInclusionPriority.value,
    },
  });
  newInclusionText.value = '';
  newInclusionPriority.value = 'standard';
  await refetch();
}

async function addExclusion(): Promise<void> {
  if (!newExclusionText.value.trim()) return;
  await tauriCommand('create_criterion', {
    request: {
      criterionType: 'exclusion',
      text: newExclusionText.value.trim(),
      priority: newExclusionPriority.value,
    },
  });
  newExclusionText.value = '';
  newExclusionPriority.value = 'standard';
  await refetch();
}

async function updateCriterionPriority(
  id: string,
  text: string,
  priority: Priority
): Promise<void> {
  await tauriCommand('update_criterion', {
    request: { id, text, priority },
  });
  await refetch();
}

async function deleteCriterion(id: string): Promise<void> {
  await tauriCommand('delete_criterion', { id });
  await refetch();
}

function priorityBorderClass(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'border-l-4 border-red-500 bg-red-50/30',
    high: 'border-l-4 border-orange-500 bg-orange-50/30',
    standard: 'border-l-4 border-indigo-500 bg-indigo-50/30',
    low: 'border-l-4 border-slate-400 bg-slate-50/50',
    optional: 'border-l-4 border-slate-300 border-dashed bg-white',
  };
  return map[priority];
}

function priorityLabelClass(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'text-red-700',
    high: 'text-orange-700',
    standard: 'text-indigo-700',
    low: 'text-slate-600',
    optional: 'text-slate-400',
  };
  return map[priority];
}

function priorityLabel(priority: Priority): string {
  const map: Record<Priority, string> = {
    critical: 'Critical Criterion',
    high: 'High Priority',
    standard: 'Standard Criterion',
    low: 'Low Priority',
    optional: 'Optional/Draft',
  };
  return map[priority];
}

// ── AI assistant logic ──────────────────────────────────────────────

const hasAims = computed(() => aims.value.length > 0);
// Use the store's canonical getter so local providers (LM Studio / Ollama /
// llama.cpp) enable the AI buttons. Re-deriving from `apiKeyEncrypted` would
// wrongly disable them - see `isConfigured` docstring in `llm-config.ts`.
const canUseAi = computed(() => hasAims.value && llmConfigStore.isConfigured);

const canGenerateStrategy = computed(() => hasAims.value && llmConfigStore.isConfigured);
const strategyButtonTitle = computed(() => {
  if (!hasAims.value) return 'Add at least one research aim first';
  if (!llmConfigStore.isConfigured) return 'Configure an LLM in Settings first';
  return 'Generate database-ready Boolean search strings from your aims';
});

const inclusionButtonLabel = computed(() =>
  inclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);
const exclusionButtonLabel = computed(() =>
  exclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);

async function handleInclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingInclusion.value) return;
  criteriaStore.generatingInclusion = true;
  criteriaStore.inclusionCritique = '';
  criteriaStore.inclusionError = null;
  try {
    if (inclusionCriteria.value.length === 0) {
      await tauriCommand('generate_criteria', {
        request: { criterionType: 'inclusion' },
      });
      await refetch();
    } else {
      const result = await tauriCommand<{ critique: string }>('critique_criteria', {
        request: { criterionType: 'inclusion' },
      });
      criteriaStore.inclusionCritique = result.critique;
      // Auto-expand so a freshly-generated critique shows its body.
      criteriaStore.inclusionCritiqueExpanded = true;
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.inclusionError = msg;
  } finally {
    criteriaStore.generatingInclusion = false;
  }
}

/** Dismiss the inclusion critique card: clear the text and reset the collapse
 * state so the next generation starts expanded. */
function dismissInclusionCritique(): void {
  criteriaStore.inclusionCritique = '';
  criteriaStore.inclusionCritiqueExpanded = true;
}

/** Render LLM Markdown critique to safe HTML. Matches the pattern in
 * `summary-view.vue`, `chat-view.vue`, and `wiki-page-editor.vue`:
 * `marked.parse(text) as string` fed to `v-html`. Content is LLM-generated
 * critique prose (no user-controlled wikilinks/footnotes). */
function renderCritiqueMarkdown(text: string): string {
  return marked.parse(text) as string;
}

async function handleExclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingExclusion.value) return;
  criteriaStore.generatingExclusion = true;
  criteriaStore.exclusionCritique = '';
  criteriaStore.exclusionError = null;
  try {
    if (exclusionCriteria.value.length === 0) {
      await tauriCommand('generate_criteria', {
        request: { criterionType: 'exclusion' },
      });
      await refetch();
    } else {
      const result = await tauriCommand<{ critique: string }>('critique_criteria', {
        request: { criterionType: 'exclusion' },
      });
      criteriaStore.exclusionCritique = result.critique;
      // Auto-expand so a freshly-generated critique shows its body.
      criteriaStore.exclusionCritiqueExpanded = true;
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.exclusionError = msg;
  } finally {
    criteriaStore.generatingExclusion = false;
  }
}

/** Dismiss the exclusion critique card: clear the text and reset the collapse
 * state so the next generation starts expanded. */
function dismissExclusionCritique(): void {
  criteriaStore.exclusionCritique = '';
  criteriaStore.exclusionCritiqueExpanded = true;
}

// ── Search Strategy Builder ────────────────────────────────────────────

async function handleSearchStrategy(): Promise<void> {
  if (!canGenerateStrategy.value || generatingSearchStrategy.value) return;
  criteriaStore.generatingSearchStrategy = true;
  criteriaStore.searchStrategyError = null;
  try {
    const result = await tauriCommand<SearchStrategyResult>('suggest_search_strategy');
    criteriaStore.searchStrategyResult = result;
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    criteriaStore.searchStrategyError = msg;
  } finally {
    criteriaStore.generatingSearchStrategy = false;
  }
}
</script>

<template>
  <div class="criteria-editor">
    <div class="criteria-editor__header">
      <h1 class="page-title">Criteria</h1>
      <p class="page-subtitle">Define research aims and inclusion/exclusion criteria</p>
    </div>

    <!-- Section 1: Research Aims -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-primary">target</span>
          <h2 class="section-panel__title">Research Aims</h2>
        </div>
        <div v-if="generatingSearchStrategy" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button
          v-else
          class="ai-btn"
          :disabled="!canGenerateStrategy"
          :title="strategyButtonTitle"
          @click="handleSearchStrategy"
        >
          <span class="material-symbols-outlined">manage_search</span>
          Suggest Search Strategy
        </button>
      </div>
      <div class="space-y-3">
        <div v-for="(aim, index) in aims" :key="aim.id" class="aim-row group">
          <span class="aim-row__number">{{ index + 1 }}</span>
          <span class="aim-row__text">{{ aim.text }}</span>
          <button class="aim-row__delete" @click="deleteAim(aim.id)">
            <span class="material-symbols-outlined">delete</span>
          </button>
        </div>
        <!-- Add new aim -->
        <div class="aim-row">
          <span class="aim-row__number">{{ aims.length + 1 }}</span>
          <input
            v-model="newAimText"
            type="text"
            class="aim-row__input"
            placeholder="Add new research aim..."
            @keyup.enter="addAim"
          />
        </div>
      </div>
    </section>

    <!-- Search Strategy error card -->
    <div v-if="searchStrategyError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">Search Strategy Generation Failed</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.searchStrategyError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(searchStrategyError).prefix }}</p>
        <p v-if="formatLlmError(searchStrategyError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(searchStrategyError).cause }}
        </p>
        <p v-if="formatLlmError(searchStrategyError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(searchStrategyError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ searchStrategyError }}</pre>
        </details>
        <a
          :href="formatLlmError(searchStrategyError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- Search Strategy result card (session-scoped) -->
    <SearchStrategyCard
      v-if="searchStrategyResult"
      :result="searchStrategyResult"
      @dismiss="criteriaStore.searchStrategyResult = null"
    />

    <!-- Section 2: Inclusion Criteria -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-green-600">check_circle</span>
          <h2 class="section-panel__title">Inclusion Criteria</h2>
        </div>
        <div v-if="generatingInclusion" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button v-else class="ai-btn" :disabled="!canUseAi" @click="handleInclusionAi">
          <span class="material-symbols-outlined">auto_awesome</span>
          {{ inclusionButtonLabel }}
        </button>
      </div>

      <div class="space-y-4 mt-4">
        <div
          v-for="c in inclusionCriteria"
          :key="c.id"
          class="criterion-card group"
          :class="priorityBorderClass(c.priority)"
        >
          <span class="criterion-card__index criterion-card__index--inc">
            {{ criterionIndexMap.get(c.id) }}
          </span>
          <div class="flex-1">
            <label class="criterion-card__label" :class="priorityLabelClass(c.priority)">
              {{ priorityLabel(c.priority) }}
            </label>
            <p class="criterion-card__text">{{ c.text }}</p>
          </div>
          <div class="criterion-card__actions">
            <select
              :value="c.priority"
              class="priority-select"
              @change="
                updateCriterionPriority(
                  c.id,
                  c.text,
                  ($event.target as HTMLSelectElement).value as Priority
                )
              "
            >
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="standard">Standard</option>
              <option value="low">Low</option>
              <option value="optional">Optional</option>
            </select>
            <button class="criterion-card__delete" @click="deleteCriterion(c.id)">
              <span class="material-symbols-outlined">delete</span>
            </button>
          </div>
        </div>

        <!-- Add new inclusion criterion (at the END, like Research Aims) -->
        <div class="criterion-add-row">
          <span class="aim-row__number">{{ inclusionCriteria.length + 1 }}</span>
          <select
            v-model="newInclusionPriority"
            class="priority-select criterion-add-row__priority"
          >
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>

          <input
            v-model="newInclusionText"
            type="text"
            class="criterion-add-row__input"
            placeholder="Add new inclusion criterion..."
            @keyup.enter="addInclusion"
          />
          <button class="btn-primary-sm criterion-add-row__add-btn" @click="addInclusion">
            Add
          </button>
        </div>
        <p class="criterion-add-row__hint">Hit enter or click Add button to save criterion</p>
      </div>
    </section>

    <!-- AI Error: Inclusion -->
    <div v-if="inclusionError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">AI Generation Failed - Inclusion Criteria</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.inclusionError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(inclusionError).prefix }}</p>
        <p v-if="formatLlmError(inclusionError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(inclusionError).cause }}
        </p>
        <p v-if="formatLlmError(inclusionError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(inclusionError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ inclusionError }}</pre>
        </details>
        <a
          :href="formatLlmError(inclusionError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- AI Critique: Inclusion -->
    <div v-if="inclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique - Inclusion Criteria</span>
        </div>
        <div class="ai-critique-card__header-actions">
          <button
            class="ai-critique-card__toggle"
            :title="criteriaStore.inclusionCritiqueExpanded ? 'Collapse' : 'Expand'"
            @click="
              criteriaStore.inclusionCritiqueExpanded = !criteriaStore.inclusionCritiqueExpanded
            "
          >
            <span class="material-symbols-outlined">{{
              criteriaStore.inclusionCritiqueExpanded ? 'expand_less' : 'expand_more'
            }}</span>
          </button>
          <button class="ai-critique-card__dismiss" @click="dismissInclusionCritique">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div
        v-if="criteriaStore.inclusionCritiqueExpanded"
        class="markdown-content ai-critique-card__body"
        v-html="renderCritiqueMarkdown(inclusionCritiqueText)"
      />
    </div>

    <!-- Section 3: Exclusion Criteria -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-error">cancel</span>
          <h2 class="section-panel__title">Exclusion Criteria</h2>
        </div>
        <div v-if="generatingExclusion" class="ai-loading">
          <span class="material-symbols-outlined animate-spin">progress_activity</span>
          <span>Generating…</span>
        </div>
        <button v-else class="ai-btn" :disabled="!canUseAi" @click="handleExclusionAi">
          <span class="material-symbols-outlined">auto_awesome</span>
          {{ exclusionButtonLabel }}
        </button>
      </div>

      <div class="space-y-4 mt-4">
        <div
          v-for="c in exclusionCriteria"
          :key="c.id"
          class="criterion-card group"
          :class="priorityBorderClass(c.priority)"
        >
          <span class="criterion-card__index criterion-card__index--exc">
            {{ criterionIndexMap.get(c.id) }}
          </span>
          <div class="flex-1">
            <label class="criterion-card__label" :class="priorityLabelClass(c.priority)">
              {{ priorityLabel(c.priority) }}
            </label>
            <p class="criterion-card__text">{{ c.text }}</p>
          </div>
          <div class="criterion-card__actions">
            <select
              :value="c.priority"
              class="priority-select"
              @change="
                updateCriterionPriority(
                  c.id,
                  c.text,
                  ($event.target as HTMLSelectElement).value as Priority
                )
              "
            >
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="standard">Standard</option>
              <option value="low">Low</option>
              <option value="optional">Optional</option>
            </select>
            <button class="criterion-card__delete" @click="deleteCriterion(c.id)">
              <span class="material-symbols-outlined">delete</span>
            </button>
          </div>
        </div>

        <!-- Add new exclusion criterion (at the END, like Research Aims) -->
        <div class="criterion-add-row">
          <span class="aim-row__number">{{ exclusionCriteria.length + 1 }}</span>
          <select
            v-model="newExclusionPriority"
            class="priority-select criterion-add-row__priority"
          >
            <option value="critical">Critical</option>
            <option value="high">High</option>
            <option value="standard">Standard</option>
            <option value="low">Low</option>
            <option value="optional">Optional</option>
          </select>

          <input
            v-model="newExclusionText"
            type="text"
            class="criterion-add-row__input"
            placeholder="Add new exclusion criterion..."
            @keyup.enter="addExclusion"
          />
          <button class="btn-primary-sm criterion-add-row__add-btn" @click="addExclusion">
            Add
          </button>
        </div>
        <p class="criterion-add-row__hint">Hit enter or click Add button to save criterion</p>
      </div>
    </section>

    <!-- AI Error: Exclusion -->
    <div v-if="exclusionError" class="ai-error-card">
      <div class="ai-error-card__header">
        <div class="ai-error-card__title-group">
          <span class="material-symbols-outlined">error</span>
          <span class="ai-error-card__title">AI Generation Failed - Exclusion Criteria</span>
        </div>
        <button class="ai-error-card__dismiss" @click="criteriaStore.exclusionError = null">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-error-card__body">
        <p class="ai-error-card__prefix">{{ formatLlmError(exclusionError).prefix }}</p>
        <p v-if="formatLlmError(exclusionError).cause" class="ai-error-card__cause">
          <strong>Cause:</strong> {{ formatLlmError(exclusionError).cause }}
        </p>
        <p v-if="formatLlmError(exclusionError).solution" class="ai-error-card__solution">
          <strong>Solution:</strong> {{ formatLlmError(exclusionError).solution }}
        </p>
        <details class="ai-error-card__details">
          <summary>Technical details</summary>
          <pre>{{ exclusionError }}</pre>
        </details>
        <a
          :href="formatLlmError(exclusionError).helpLink"
          class="ai-error-card__help-link"
          target="_blank"
        >
          <span class="material-symbols-outlined" style="font-size: 14px">help</span>
          Troubleshooting guide
        </a>
      </div>
    </div>

    <!-- AI Critique: Exclusion -->
    <div v-if="exclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique - Exclusion Criteria</span>
        </div>
        <div class="ai-critique-card__header-actions">
          <button
            class="ai-critique-card__toggle"
            :title="criteriaStore.exclusionCritiqueExpanded ? 'Collapse' : 'Expand'"
            @click="
              criteriaStore.exclusionCritiqueExpanded = !criteriaStore.exclusionCritiqueExpanded
            "
          >
            <span class="material-symbols-outlined">{{
              criteriaStore.exclusionCritiqueExpanded ? 'expand_less' : 'expand_more'
            }}</span>
          </button>
          <button class="ai-critique-card__dismiss" @click="dismissExclusionCritique">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      </div>
      <div
        v-if="criteriaStore.exclusionCritiqueExpanded"
        class="markdown-content ai-critique-card__body"
        v-html="renderCritiqueMarkdown(exclusionCritiqueText)"
      />
    </div>
  </div>
</template>

<style scoped>
.criteria-editor {
  padding: var(--container-padding);
  max-width: 64rem;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

@media (max-width: 767px) {
  .criteria-editor {
    padding: var(--container-padding-sm);
    gap: 1.5rem;
  }
}

.section-panel {
  background-color: #ffffff;
  border-radius: 0.75rem;
  padding: 1.5rem;
  border: 1px solid #e2e8f0;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
}

@media (max-width: 767px) {
  .section-panel {
    padding: 1rem;
  }

  .section-panel__header {
    flex-wrap: wrap;
    gap: 0.5rem;
  }
}

.section-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1.5rem;
}

.section-panel__title-group {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.section-panel__title {
  font-size: 20px;
  line-height: 28px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: #1b1b24;
}

.section-panel__count {
  font-size: 12px;
  color: #94a3b8;
  font-family: ui-monospace, SFMono-Regular, monospace;
}

/* Aim rows */
.aim-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.aim-row__number {
  width: 1.5rem;
  height: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: #f1f5f9;
  color: #94a3b8;
  font-size: 12px;
  font-weight: 700;
  border-radius: 9999px;
  flex-shrink: 0;
}

.aim-row__text {
  flex: 1;
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
}

.aim-row__input {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px dashed #e2e8f0;
  padding: 0.5rem 0;
  font-size: 14px;
  line-height: 20px;
  color: #94a3b8;
  font-style: italic;
  outline: none;
  transition: border-color 0.15s;
}

.aim-row__input:focus {
  border-bottom-color: #4f46e5;
  color: #1b1b24;
  font-style: normal;
}

.aim-row__delete {
  opacity: 0;
  transition: opacity 0.15s;
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
}

.aim-row__delete:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.aim-row:hover .aim-row__delete,
.group:hover .aim-row__delete {
  opacity: 1;
}

/* Criterion add-row (mirrors the Research Aims dashed-input pattern, extended
 * with a priority dropdown + Add button on the left of the input). Sits at the
 * END of the criteria list, with a number prefix matching the existing cards. */
.criterion-add-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.5rem;
}

.criterion-add-row__priority {
  width: auto;
  flex-shrink: 0;
}

.criterion-add-row__add-btn {
  flex-shrink: 0;
}

.criterion-add-row__input {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px dashed #e2e8f0;
  padding: 0.5rem 0;
  font-size: 14px;
  line-height: 20px;
  color: #94a3b8;
  font-style: italic;
  outline: none;
  transition: border-color 0.15s;
}

.criterion-add-row__input::placeholder {
  color: #94a3b8;
  font-style: italic;
}

.criterion-add-row__input:focus {
  border-bottom-color: #4f46e5;
  color: #1b1b24;
  font-style: normal;
}

.criterion-add-row__hint {
  font-size: 11px;
  color: #94a3b8;
  margin: 0.25rem 0 0 0;
  padding-left: 2.75rem;
}

@media (max-width: 767px) {
  .criterion-add-row {
    flex-wrap: wrap;
  }

  .criterion-add-row__input {
    flex: 1 1 100%;
    order: 99;
  }
}

/* Priority select */
.priority-select {
  font-size: 12px;
  background-color: #ffffff;
  border: 1px solid #e2e8f0;
  border-radius: 0.25rem;
  padding: 0.25rem 0.5rem;
  outline: none;
  cursor: pointer;
}

.priority-select:focus {
  border-color: #3525cd;
}

/* Criterion card */
.criterion-card {
  display: flex;
  align-items: flex-start;
  gap: 1rem;
  padding: 1rem;
  border-radius: 0 0.5rem 0.5rem 0;
}

@media (max-width: 767px) {
  .criterion-card {
    flex-direction: column;
    gap: 0.75rem;
  }

  .criterion-card__actions {
    flex-direction: row;
    align-items: center;
    width: 100%;
  }
}

.criterion-card__index {
  width: 1.5rem;
  height: 1.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  border-radius: 0.375rem;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.criterion-card__index--inc {
  background-color: #ecfdf5;
  color: #059669;
}

.criterion-card__index--exc {
  background-color: #fff1f2;
  color: #e11d48;
}

.criterion-card__label {
  font-size: 10px;
  text-transform: uppercase;
  font-weight: 700;
  letter-spacing: 0.05em;
  margin-bottom: 0.25rem;
  display: block;
}

.criterion-card__text {
  font-size: 14px;
  line-height: 20px;
  color: #1b1b24;
}

.criterion-card__actions {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.5rem;
}

.criterion-card__delete {
  opacity: 0;
  transition: opacity 0.15s;
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
}

.criterion-card__delete:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.criterion-card.group:hover .criterion-card__delete {
  opacity: 1;
}

/* Buttons */
.add-btn {
  color: #3525cd;
  font-weight: 600;
  font-size: 14px;
  display: flex;
  align-items: center;
  gap: 0.25rem;
  background: none;
  border: none;
  cursor: pointer;
  transition: color 0.15s;
}

.add-btn:hover {
  color: #4f46e5;
}

.add-btn__icon {
  font-size: 16px;
}

.btn-primary-sm {
  background-color: #3525cd;
  color: #ffffff;
  font-size: 12px;
  font-weight: 600;
  padding: 0.375rem 0.75rem;
  border-radius: 0.375rem;
  border: none;
  cursor: pointer;
  transition: background-color 0.15s;
}

.btn-primary-sm:hover {
  background-color: #4f46e5;
}

/* AI button - matches Tags & Labels pattern */
.ai-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  background-color: #e8def8;
  color: #4a1564;
  font-size: 14px;
  font-weight: 500;
  border: 1px solid #c8aee6;
  border-radius: 0.5rem;
  cursor: pointer;
  white-space: nowrap;
  transition:
    background-color 0.15s,
    opacity 0.15s;
}

.ai-btn:hover:not(:disabled) {
  background-color: #d8c8f0;
}

.ai-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ai-btn .material-symbols-outlined {
  font-size: 18px;
}

/* AI loading indicator */
.ai-loading {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  background-color: #f3e8ff;
  color: #7c3aed;
  font-size: 14px;
  font-weight: 500;
  border: 1px solid #ddd6fe;
  border-radius: 0.5rem;
  white-space: nowrap;
  animation: pulse-subtle 1.5s ease-in-out infinite;
}

.ai-loading .material-symbols-outlined {
  font-size: 18px;
}

@keyframes pulse-subtle {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.7;
  }
}

/* AI Critique card */
.ai-critique-card {
  background-color: #f5f0ff;
  border: 1px solid #d8c8f0;
  border-radius: 0.75rem;
  padding: 1rem 1.25rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
}

.ai-critique-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

/* Collapse/expand + close actions wrapper for the critique card header
 * (same shape as the search-strategy-card header actions). */
.ai-critique-card__header-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

/* Collapse/expand chevron toggle for the critique cards. Values mirror
 * `.search-strategy-card__toggle` so the two card families stay visually
 * consistent. */
.ai-critique-card__toggle {
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

.ai-critique-card__toggle:hover {
  color: #6b21a8;
  background-color: #ede9fe;
}

.ai-critique-card__toggle .material-symbols-outlined {
  font-size: 20px;
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

.ai-critique-card__body :deep(p) {
  margin-bottom: 0.75rem;
}

.ai-critique-card__body :deep(p:last-child) {
  margin-bottom: 0;
}

/* AI Error card */
.ai-error-card {
  background-color: #fef2f2;
  border: 1px solid #fca5a5;
  border-left: 4px solid #dc2626;
  border-radius: 0.75rem;
  padding: 1rem 1.25rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
}

.ai-error-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.ai-error-card__title-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: #991b1b;
}

.ai-error-card__title-group .material-symbols-outlined {
  font-size: 20px;
  color: #dc2626;
}

.ai-error-card__title {
  font-size: 14px;
  font-weight: 600;
}

.ai-error-card__dismiss {
  background: none;
  border: none;
  cursor: pointer;
  color: #94a3b8;
  padding: 0.25rem;
  border-radius: 0.25rem;
  transition:
    color 0.15s,
    background-color 0.15s;
}

.ai-error-card__dismiss:hover {
  color: #ba1a1a;
  background-color: #fef2f2;
}

.ai-error-card__dismiss .material-symbols-outlined {
  font-size: 18px;
}

.ai-error-card__body {
  font-size: 14px;
  line-height: 22px;
  color: #1b1b24;
}

.ai-error-card__prefix {
  color: #7f1d1d;
  font-size: 13px;
  margin-bottom: 0.75rem;
}

.ai-error-card__cause {
  margin-bottom: 0.5rem;
  color: #374151;
}

.ai-error-card__solution {
  margin-bottom: 0.75rem;
  color: #374151;
}

.ai-error-card__details {
  margin-top: 0.75rem;
  margin-bottom: 0.5rem;
}

.ai-error-card__details summary {
  cursor: pointer;
  font-size: 12px;
  color: #6b7280;
  user-select: none;
}

.ai-error-card__details pre {
  margin-top: 0.5rem;
  padding: 0.75rem;
  background-color: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 0.375rem;
  font-size: 12px;
  line-height: 18px;
  color: #374151;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 10rem;
  overflow-y: auto;
}

.ai-error-card__help-link {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 13px;
  color: #3525cd;
  text-decoration: none;
  margin-top: 0.5rem;
}

.ai-error-card__help-link:hover {
  text-decoration: underline;
}
</style>
