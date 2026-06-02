<script setup lang="ts">
import { ref, computed } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigStore } from '@/stores/llm-config';
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

// AI assistant state
const generatingInclusion = ref(false);
const generatingExclusion = ref(false);
const inclusionCritiqueText = ref('');
const exclusionCritiqueText = ref('');

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
const llmConnected = computed(
  () => llmConfigStore.initialized && llmConfigStore.config.apiKeyEncrypted !== null
);
const canUseAi = computed(() => hasAims.value && llmConnected.value);

const inclusionButtonLabel = computed(() =>
  inclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);
const exclusionButtonLabel = computed(() =>
  exclusionCriteria.value.length === 0 ? 'Generate with AI' : 'Critique with AI'
);

async function handleInclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingInclusion.value) return;
  generatingInclusion.value = true;
  inclusionCritiqueText.value = '';
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
      inclusionCritiqueText.value = result.critique;
    }
  } finally {
    generatingInclusion.value = false;
  }
}

/** Split plain-text critique into paragraphs for safe rendering. */
function critiqueParagraphs(text: string): string[] {
  return text.split('\n\n').filter((p) => p.trim().length > 0);
}

async function handleExclusionAi(): Promise<void> {
  if (!canUseAi.value || generatingExclusion.value) return;
  generatingExclusion.value = true;
  exclusionCritiqueText.value = '';
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
      exclusionCritiqueText.value = result.critique;
    }
  } finally {
    generatingExclusion.value = false;
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
        <span class="section-panel__count">{{ aims.length }} Entries</span>
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

      <!-- Add new inclusion criterion -->
      <div class="add-criterion-row">
        <input
          v-model="newInclusionText"
          type="text"
          class="add-criterion-row__input"
          placeholder="Define an inclusion criterion..."
          @keyup.enter="addInclusion"
        />
        <select v-model="newInclusionPriority" class="priority-select">
          <option value="critical">Critical</option>
          <option value="high">High</option>
          <option value="standard">Standard</option>
          <option value="low">Low</option>
          <option value="optional">Optional</option>
        </select>
        <button class="btn-primary-sm" @click="addInclusion">Add</button>
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
      </div>
    </section>

    <!-- AI Critique: Inclusion -->
    <div v-if="inclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique — Inclusion Criteria</span>
        </div>
        <button class="ai-critique-card__dismiss" @click="inclusionCritiqueText = ''">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-critique-card__body">
        <p v-for="(para, i) in critiqueParagraphs(inclusionCritiqueText)" :key="i">
          {{ para }}
        </p>
      </div>
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

      <!-- Add new exclusion criterion -->
      <div class="add-criterion-row">
        <input
          v-model="newExclusionText"
          type="text"
          class="add-criterion-row__input"
          placeholder="Define an exclusion criterion..."
          @keyup.enter="addExclusion"
        />
        <select v-model="newExclusionPriority" class="priority-select">
          <option value="critical">Critical</option>
          <option value="high">High</option>
          <option value="standard">Standard</option>
          <option value="low">Low</option>
          <option value="optional">Optional</option>
        </select>
        <button class="btn-primary-sm" @click="addExclusion">Add</button>
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
      </div>
    </section>

    <!-- AI Critique: Exclusion -->
    <div v-if="exclusionCritiqueText" class="ai-critique-card">
      <div class="ai-critique-card__header">
        <div class="ai-critique-card__title-group">
          <span class="material-symbols-outlined">auto_awesome</span>
          <span class="ai-critique-card__title">AI Critique — Exclusion Criteria</span>
        </div>
        <button class="ai-critique-card__dismiss" @click="exclusionCritiqueText = ''">
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      <div class="ai-critique-card__body">
        <p v-for="(para, i) in critiqueParagraphs(exclusionCritiqueText)" :key="i">
          {{ para }}
        </p>
      </div>
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

/* Add criterion row */
.add-criterion-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

@media (max-width: 767px) {
  .add-criterion-row {
    flex-wrap: wrap;
  }

  .add-criterion-row__input {
    flex: 1 1 100%;
  }

  .priority-select {
    flex: 1;
  }
}

.add-criterion-row__input {
  flex: 1;
  padding: 0.5rem 0.75rem;
  border: 1px solid #c7c4d8;
  border-radius: 0.5rem;
  font-size: 14px;
  line-height: 20px;
  outline: none;
  transition: border-color 0.15s;
}

.add-criterion-row__input:focus {
  border-color: #3525cd;
  box-shadow: 0 0 0 1px #3525cd;
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

/* AI button — matches Tags & Labels pattern */
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
</style>
