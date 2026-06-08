<script setup lang="ts">
import { ref, computed } from 'vue';
import type { Article } from '@/types';
import { useLabelsStore } from '@/stores/labels';
import LabelChip from './label-chip.vue';
import SuggestInput from './suggest-input.vue';

const props = defineProps<{
  article: Article;
}>();

const emit = defineEmits<{
  updateLabels: [id: string, labelIds: string[]];
}>();

const labelsStore = useLabelsStore();
const newLabel = ref('');

// Alphabetically sorted labels (case-insensitive)
const sortedLabels = computed(() =>
  [...props.article.labels].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
);

// Suggestions from global store, excluding already-assigned values
const labelSuggestions = computed(() => {
  const assigned = new Set(props.article.labels.map((l) => l.toLowerCase()));
  return labelsStore.labels
    .map((l) => l.name)
    .filter((name) => !assigned.has(name.toLowerCase()))
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
});

/** Look up the color for a label name from the global store */
function labelColor(name: string): string | null {
  return labelsStore.labels.find((l) => l.name === name)?.color ?? null;
}

function removeLabel(label: string): void {
  const updated = props.article.labels.filter((l) => l !== label);
  emit('updateLabels', props.article.id, updated);
}

async function addLabel(val: string): Promise<void> {
  if (!val || props.article.labels.includes(val)) return;
  emit('updateLabels', props.article.id, [...props.article.labels, val]);
  newLabel.value = '';
  // If the label doesn't exist in the global store, create it
  const existsInStore = labelsStore.labels.some((l) => l.name.toLowerCase() === val.toLowerCase());
  if (!existsInStore) {
    await labelsStore.createLabel(val);
    await labelsStore.fetchIfNeeded();
  }
}
</script>

<template>
  <section>
    <h3 class="text-xs font-label-caps text-slate-500 uppercase mb-3 tracking-wider">Labels</h3>
    <div class="flex flex-wrap gap-2 mb-2">
      <span
        v-for="label in sortedLabels"
        :key="'label-' + label"
        class="inline-flex items-center gap-1 group"
      >
        <LabelChip :name="label" :color="labelColor(label)" />
        <button
          class="material-symbols-outlined text-[14px] text-slate-400 hover:text-slate-700 cursor-pointer rounded-full hover:bg-slate-100 leading-none opacity-0 group-hover:opacity-100 transition-opacity"
          @click="removeLabel(label)"
        >
          close
        </button>
      </span>
    </div>
    <div class="flex gap-2">
      <SuggestInput
        v-model="newLabel"
        :suggestions="labelSuggestions"
        placeholder="Add label…"
        class="flex-1"
        @select="addLabel"
        @enter="addLabel"
      />
    </div>
  </section>
</template>
