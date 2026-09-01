<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import type { Criterion, SuggestOption } from '@/types';
import SuggestInput from './suggest-input.vue';

const props = defineProps<{
  modelValue: boolean;
  articleId: string;
  matchedInclusionIds: string[];
  matchedExclusionIds: string[];
  inclusionCriteria: Criterion[];
  exclusionCriteria: Criterion[];
}>();

const emit = defineEmits<{
  'update:modelValue': [value: boolean];
  save: [articleId: string, inclusionIds: string[], exclusionIds: string[]];
}>();

/* Working copies of the two stored arrays. Values are criterion UUIDs plus
 * "ghost" strings (raw numbers, deleted-criterion UUIDs) rendered as amber
 * dashed pills and removable in place. */
const inclusionIds = ref<string[]>([]);
const exclusionIds = ref<string[]>([]);

/** Global criterion numbering: inclusion 1..N, exclusion N+1..N+M. */
const indexMap = computed(() => {
  const map = new Map<string, number>();
  let n = 1;
  for (const c of props.inclusionCriteria) map.set(c.id, n++);
  for (const c of props.exclusionCriteria) map.set(c.id, n++);
  return map;
});

/** All live criteria by id (both types) for pill resolution. */
const criterionById = computed(() => {
  const map = new Map<string, Criterion>();
  for (const c of props.inclusionCriteria) map.set(c.id, c);
  for (const c of props.exclusionCriteria) map.set(c.id, c);
  return map;
});

/** Resolved values assigned anywhere: a criterion can sit in only one section
 * (satisfied inclusion XOR failed/violated via the exclusion section). */
const assignedIds = computed(
  () =>
    new Set(
      [...inclusionIds.value, ...exclusionIds.value].filter((v) => criterionById.value.has(v))
    )
);

/** Inclusion combobox: unassigned inclusion criteria with number badges. */
const inclusionOptions = computed<SuggestOption[]>(() =>
  props.inclusionCriteria
    .filter((c) => !assignedIds.value.has(c.id))
    .map((c) => ({ id: c.id, label: c.text, badge: String(indexMap.value.get(c.id) ?? '-') }))
);

/** Exclusion combobox: unassigned criteria of both types; inclusion entries
 * are labeled NOT MET so a pick reads as "this requirement failed". */
const exclusionOptions = computed<SuggestOption[]>(() =>
  [...props.exclusionCriteria, ...props.inclusionCriteria]
    .filter((c) => !assignedIds.value.has(c.id))
    .map((c) => ({
      id: c.id,
      label: c.criterionType === 'exclusion' ? c.text : `NOT MET: ${c.text}`,
      badge: String(indexMap.value.get(c.id) ?? '-'),
    }))
);

const inclusionInput = ref('');
const exclusionInput = ref('');

// Sync from props when dialog opens (immediate covers mount-while-open)
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      inclusionIds.value = [...props.matchedInclusionIds];
      exclusionIds.value = [...props.matchedExclusionIds];
    }
  },
  { immediate: true }
);

/** Combobox select: add the picked criterion to the section it was chosen in. */
function addInclusion(_name: string, option?: SuggestOption): void {
  if (!option || inclusionIds.value.includes(option.id)) return;
  inclusionIds.value = [...inclusionIds.value, option.id];
}

function addExclusion(_name: string, option?: SuggestOption): void {
  if (!option || exclusionIds.value.includes(option.id)) return;
  exclusionIds.value = [...exclusionIds.value, option.id];
}

/** Pill "x": remove the first occurrence of the value from its section. */
function removeInclusion(value: string): void {
  const idx = inclusionIds.value.indexOf(value);
  if (idx !== -1) inclusionIds.value = inclusionIds.value.filter((_, i) => i !== idx);
}

function removeExclusion(value: string): void {
  const idx = exclusionIds.value.indexOf(value);
  if (idx !== -1) exclusionIds.value = exclusionIds.value.filter((_, i) => i !== idx);
}

/** A ghost value matches no live criterion (raw number, deleted UUID). */
function isGhost(value: string): boolean {
  return !criterionById.value.has(value);
}

/** An inclusion-type UUID recorded via the exclusion section: the required
 * criterion FAILED and drove the rejection (implicit cross-type contract). */
function isFailedInclusion(value: string): boolean {
  return criterionById.value.get(value)?.criterionType === 'inclusion';
}

function pillText(value: string): string {
  return criterionById.value.get(value)?.text ?? value;
}

function truncate(text: string, max = 24): string {
  return text.length > max ? text.slice(0, max) + '…' : text;
}

function pillTitle(value: string): string {
  if (isGhost(value)) return `Unmatched stored entry: ${value}`;
  if (isFailedInclusion(value) && exclusionIds.value.includes(value)) {
    return `Failed inclusion criterion (reason for rejection): ${pillText(value)}`;
  }
  return pillText(value);
}

function save(): void {
  emit('save', props.articleId, [...inclusionIds.value], [...exclusionIds.value]);
  emit('update:modelValue', false);
}

function close(): void {
  emit('update:modelValue', false);
}

const hasChanges = computed(() => {
  const sameSet = (a: string[], b: string[]): boolean =>
    a.length === b.length && a.every((id) => b.includes(id)) && b.every((id) => a.includes(id));
  return (
    !sameSet(inclusionIds.value, props.matchedInclusionIds) ||
    !sameSet(exclusionIds.value, props.matchedExclusionIds)
  );
});
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-overlay">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-[100] flex items-center justify-center bg-black/40"
        @click.self="close"
      >
        <Transition name="dialog-panel">
          <div
            v-if="modelValue"
            class="bg-white rounded-xl shadow-2xl border border-slate-200 w-full max-w-lg max-h-[90vh] flex flex-col mx-4"
          >
            <!-- Header -->
            <div class="flex items-center justify-between px-5 py-4 border-b border-slate-100">
              <h2 class="text-base font-bold text-slate-900">Edit Matched Criteria</h2>
              <button
                class="material-symbols-outlined text-slate-400 hover:text-slate-700 cursor-pointer"
                @click="close"
              >
                close
              </button>
            </div>

            <!-- Body -->
            <div class="flex-1 overflow-y-auto px-5 py-4 space-y-5">
              <!-- Inclusion Criteria: removable pills + search-and-add combobox -->
              <div>
                <h3
                  class="text-[11px] font-bold uppercase tracking-wider text-emerald-700 mb-1 flex items-center gap-1.5"
                >
                  <span class="material-symbols-outlined text-sm">check_circle</span>
                  Inclusion Criteria
                </h3>
                <p class="text-xs text-slate-400 italic mb-2 pl-1">
                  Criteria the article satisfies.
                </p>
                <div
                  v-if="inclusionIds.length > 0"
                  class="flex flex-wrap gap-1.5 mb-2 max-h-32 overflow-y-auto"
                >
                  <span
                    v-for="value in inclusionIds"
                    :key="'inc-' + value"
                    class="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] font-medium border select-none"
                    :class="
                      isGhost(value)
                        ? 'bg-amber-50 border-dashed border-amber-300 text-amber-800'
                        : 'bg-emerald-50 border-emerald-200 text-emerald-800'
                    "
                    :title="pillTitle(value)"
                  >
                    <span
                      class="font-bold rounded px-1 leading-tight"
                      :class="
                        isGhost(value)
                          ? 'text-amber-700 bg-amber-100'
                          : 'text-emerald-700 bg-emerald-100'
                      "
                      >{{ indexMap.get(value) ?? '-' }}</span
                    >
                    <span class="truncate max-w-[10rem]">{{ truncate(pillText(value)) }}</span>
                    <button
                      type="button"
                      class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
                      :title="`Remove ${truncate(pillText(value))}`"
                      @click="removeInclusion(value)"
                    >
                      ×
                    </button>
                  </span>
                </div>
                <p v-else class="text-xs text-slate-400 italic mb-2 pl-1">
                  No inclusion criteria assigned.
                </p>
                <SuggestInput
                  v-if="inclusionOptions.length > 0"
                  v-model="inclusionInput"
                  :options="inclusionOptions"
                  placeholder="Search inclusion criteria to add..."
                  @select="addInclusion"
                />
              </div>

              <!-- Exclusion Criteria: removable pills + search-and-add combobox.
                   Rose = violated exclusion criteria; amber = failed inclusion
                   criteria (NOT MET, the rejection reason); amber dashed =
                   ghost values (raw numbers, deleted-criterion UUIDs). -->
              <div>
                <h3
                  class="text-[11px] font-bold uppercase tracking-wider text-rose-700 mb-1 flex items-center gap-1.5"
                >
                  <span class="material-symbols-outlined text-sm">cancel</span>
                  Exclusion Criteria
                </h3>
                <p class="text-xs text-slate-400 italic mb-2 pl-1">
                  Violated criteria and failed inclusion criteria (reasons for rejection).
                </p>
                <div
                  v-if="exclusionIds.length > 0"
                  class="flex flex-wrap gap-1.5 mb-2 max-h-32 overflow-y-auto"
                >
                  <span
                    v-for="value in exclusionIds"
                    :key="'exc-' + value"
                    class="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] font-medium border select-none"
                    :class="
                      isGhost(value)
                        ? 'bg-amber-50 border-dashed border-amber-300 text-amber-800'
                        : isFailedInclusion(value)
                          ? 'bg-amber-50 border-amber-200 text-amber-800'
                          : 'bg-rose-50 border-rose-200 text-rose-800'
                    "
                    :title="pillTitle(value)"
                  >
                    <span
                      class="font-bold rounded px-1 leading-tight"
                      :class="
                        isGhost(value) || isFailedInclusion(value)
                          ? 'text-amber-700 bg-amber-100'
                          : 'text-rose-700 bg-rose-100'
                      "
                      >{{ indexMap.get(value) ?? '-' }}</span
                    >
                    <span class="truncate max-w-[10rem]">{{ truncate(pillText(value)) }}</span>
                    <button
                      type="button"
                      class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
                      :title="`Remove ${truncate(pillText(value))}`"
                      @click="removeExclusion(value)"
                    >
                      ×
                    </button>
                  </span>
                </div>
                <p v-else class="text-xs text-slate-400 italic mb-2 pl-1">
                  No exclusion criteria assigned.
                </p>
                <SuggestInput
                  v-if="exclusionOptions.length > 0"
                  v-model="exclusionInput"
                  :options="exclusionOptions"
                  placeholder="Search criteria to add..."
                  @select="addExclusion"
                />
              </div>
            </div>

            <!-- Footer -->
            <div
              class="flex items-center justify-end gap-2 px-5 py-3 border-t border-slate-100 bg-slate-50/50 rounded-b-xl"
            >
              <button
                class="text-xs text-slate-500 hover:text-slate-700 font-semibold cursor-pointer px-4 py-2 border border-slate-300 rounded-lg"
                @click="close"
              >
                Cancel
              </button>
              <button
                class="text-xs bg-indigo-600 text-white px-4 py-2 rounded-lg font-semibold hover:bg-indigo-700 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                :disabled="!hasChanges"
                @click="save"
              >
                Save Changes
              </button>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dialog-overlay-enter-active,
.dialog-overlay-leave-active {
  transition: opacity 0.2s ease;
}
.dialog-overlay-enter-from,
.dialog-overlay-leave-to {
  opacity: 0;
}

.dialog-panel-enter-active {
  transition: all 0.2s ease;
}
.dialog-panel-leave-active {
  transition: all 0.15s ease;
}
.dialog-panel-enter-from,
.dialog-panel-leave-to {
  opacity: 0;
  transform: scale(0.95) translateY(10px);
}
</style>
