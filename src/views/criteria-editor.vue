<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { ResearchAim, Criterion, Priority } from '@/types';

const aims = ref<ResearchAim[]>([]);
const criteria = ref<Criterion[]>([]);
const newAimText = ref('');
const newInclusionText = ref('');
const newExclusionText = ref('');
const newInclusionPriority = ref<Priority>('standard');
const newExclusionPriority = ref<Priority>('standard');
const loading = ref(false);

const inclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'inclusion')
);
const exclusionCriteria = computed(() =>
  criteria.value.filter((c) => c.criterionType === 'exclusion')
);

onMounted(fetchAll);

async function fetchAll(): Promise<void> {
  loading.value = true;
  try {
    const [aimsResult, criteriaResult] = await Promise.all([
      tauriCommand<ResearchAim[]>('get_research_aims'),
      tauriCommand<Criterion[]>('get_criteria'),
    ]);
    aims.value = aimsResult;
    criteria.value = criteriaResult;
  } finally {
    loading.value = false;
  }
}

async function addAim(): Promise<void> {
  if (!newAimText.value.trim()) return;
  await tauriCommand('create_research_aim', {
    request: { text: newAimText.value.trim() },
  });
  newAimText.value = '';
  await fetchAll();
}

async function deleteAim(id: string): Promise<void> {
  await tauriCommand('delete_research_aim', { id });
  await fetchAll();
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
  await fetchAll();
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
  await fetchAll();
}

async function updateCriterionPriority(
  id: string,
  text: string,
  priority: Priority
): Promise<void> {
  await tauriCommand('update_criterion', {
    request: { id, text, priority },
  });
  await fetchAll();
}

async function deleteCriterion(id: string): Promise<void> {
  await tauriCommand('delete_criterion', { id });
  await fetchAll();
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
</script>

<template>
  <div class="criteria-editor">
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
        <button class="add-btn" @click="addInclusion">
          <span class="material-symbols-outlined add-btn__icon">add</span>
          Add Criterion
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
              <span class="material-symbols-outlined">more_vert</span>
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- Section 3: Exclusion Criteria -->
    <section class="section-panel">
      <div class="section-panel__header">
        <div class="section-panel__title-group">
          <span class="material-symbols-outlined text-error">cancel</span>
          <h2 class="section-panel__title">Exclusion Criteria</h2>
        </div>
        <button class="add-btn" @click="addExclusion">
          <span class="material-symbols-outlined add-btn__icon">add</span>
          Add Criterion
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
              <span class="material-symbols-outlined">more_vert</span>
            </button>
          </div>
        </div>
      </div>
    </section>
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
</style>
