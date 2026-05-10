<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import type { Criterion } from '@/types';

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

// Local working copies of the checked IDs
const checkedInclusion = ref<Set<string>>(new Set());
const checkedExclusion = ref<Set<string>>(new Set());

// Sync from props when dialog opens
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      checkedInclusion.value = new Set(props.matchedInclusionIds);
      checkedExclusion.value = new Set(props.matchedExclusionIds);
    }
  }
);

function toggleInclusion(id: string): void {
  const s = new Set(checkedInclusion.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  checkedInclusion.value = s;
}

function toggleExclusion(id: string): void {
  const s = new Set(checkedExclusion.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  checkedExclusion.value = s;
}

function save(): void {
  emit(
    'save',
    props.articleId,
    Array.from(checkedInclusion.value),
    Array.from(checkedExclusion.value)
  );
  emit('update:modelValue', false);
}

function close(): void {
  emit('update:modelValue', false);
}

const hasChanges = computed(() => {
  const incChanged =
    checkedInclusion.value.size !== props.matchedInclusionIds.length ||
    props.matchedInclusionIds.some((id) => !checkedInclusion.value.has(id));
  const excChanged =
    checkedExclusion.value.size !== props.matchedExclusionIds.length ||
    props.matchedExclusionIds.some((id) => !checkedExclusion.value.has(id));
  return incChanged || excChanged;
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
            class="bg-white rounded-xl shadow-2xl border border-slate-200 w-full max-w-lg max-h-[80vh] flex flex-col mx-4"
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
              <!-- Inclusion Criteria -->
              <div>
                <h3
                  class="text-[11px] font-bold uppercase tracking-wider text-emerald-700 mb-2 flex items-center gap-1.5"
                >
                  <span class="material-symbols-outlined text-sm">check_circle</span>
                  Inclusion Criteria
                </h3>
                <div
                  v-if="inclusionCriteria.length === 0"
                  class="text-xs text-slate-400 italic pl-1"
                >
                  No inclusion criteria defined
                </div>
                <ul v-else class="space-y-1">
                  <li
                    v-for="c in inclusionCriteria"
                    :key="c.id"
                    class="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-slate-50 cursor-pointer transition-colors select-none"
                    @click="toggleInclusion(c.id)"
                  >
                    <input
                      type="checkbox"
                      :checked="checkedInclusion.has(c.id)"
                      class="accent-emerald-600 w-4 h-4 rounded cursor-pointer pointer-events-none"
                      tabindex="-1"
                      readonly
                    />
                    <span class="text-sm text-slate-800 flex-1">{{ c.text }}</span>
                  </li>
                </ul>
              </div>

              <!-- Exclusion Criteria -->
              <div>
                <h3
                  class="text-[11px] font-bold uppercase tracking-wider text-rose-700 mb-2 flex items-center gap-1.5"
                >
                  <span class="material-symbols-outlined text-sm">cancel</span>
                  Exclusion Criteria
                </h3>
                <div
                  v-if="exclusionCriteria.length === 0"
                  class="text-xs text-slate-400 italic pl-1"
                >
                  No exclusion criteria defined
                </div>
                <ul v-else class="space-y-1">
                  <li
                    v-for="c in exclusionCriteria"
                    :key="c.id"
                    class="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-slate-50 cursor-pointer transition-colors select-none"
                    @click="toggleExclusion(c.id)"
                  >
                    <input
                      type="checkbox"
                      :checked="checkedExclusion.has(c.id)"
                      class="accent-rose-600 w-4 h-4 rounded cursor-pointer pointer-events-none"
                      tabindex="-1"
                      readonly
                    />
                    <span class="text-sm text-slate-800 flex-1">{{ c.text }}</span>
                  </li>
                </ul>
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
