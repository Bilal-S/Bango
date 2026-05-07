<script setup lang="ts">
import { ref, computed } from 'vue';
import type { ArticleFilter } from '@/composables/use-article-search';
import type { TitleMatchType } from '@/composables/use-article-search';

const props = defineProps<{
  filter: ArticleFilter;
  allAuthors: string[];
  allTags: string[];
  allLabels: string[];
}>();

const emit = defineEmits<{
  apply: [];
  clear: [];
  'update:filter': [key: keyof ArticleFilter, value: unknown];
}>();

const MATCH_TYPES: { value: TitleMatchType; label: string }[] = [
  { value: 'starts_with', label: 'Starts with' },
  { value: 'contains', label: 'Contains' },
  { value: 'ends_with', label: 'Ends with' },
  { value: 'exact', label: 'Exact' },
];

function updateField(key: keyof ArticleFilter, value: unknown): void {
  emit('update:filter', key, value);
}

function toggleTag(tag: string): void {
  const current = props.filter.tags;
  const updated = current.includes(tag) ? current.filter((t) => t !== tag) : [...current, tag];
  updateField('tags', updated);
}

function toggleLabel(label: string): void {
  const current = props.filter.labels;
  const updated = current.includes(label)
    ? current.filter((l) => l !== label)
    : [...current, label];
  updateField('labels', updated);
}

const showAuthorDropdown = ref(false);

function hideAuthorDropdown(): void {
  window.setTimeout(() => (showAuthorDropdown.value = false), 200);
}

const matchedAuthors = computed(() => {
  const text = props.filter.authorText.toLowerCase();
  if (!text) return [];
  return props.allAuthors.filter((a) => a.toLowerCase().includes(text));
});
</script>

<template>
  <div class="bg-white rounded-xl border border-slate-200 shadow-sm p-6 mb-6">
    <div class="flex items-center justify-between mb-4">
      <h3 class="font-h2 text-h2 text-on-surface">Filters</h3>
      <button
        class="text-xs text-slate-500 hover:text-indigo-600 transition-colors font-medium"
        @click="emit('clear')"
      >
        Clear All
      </button>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
      <!-- Title -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Title</label>
        <div class="flex gap-2">
          <select
            class="shrink-0 w-32 bg-slate-50 border border-slate-200 rounded-lg px-2 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.titleMatch"
            @change="
              updateField(
                'titleMatch',
                ($event.target as HTMLSelectElement).value as TitleMatchType
              )
            "
          >
            <option v-for="mt in MATCH_TYPES" :key="mt.value" :value="mt.value">
              {{ mt.label }}
            </option>
          </select>
          <input
            type="text"
            placeholder="Filter by title..."
            class="flex-1 bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.titleText"
            @input="updateField('titleText', ($event.target as HTMLInputElement).value)"
          />
        </div>
      </div>

      <!-- Author -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Author</label>
        <div class="relative w-[70%]">
          <input
            type="text"
            placeholder="Filter by author..."
            class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.authorText"
            @focus="showAuthorDropdown = true"
            @blur="hideAuthorDropdown()"
            @input="
              showAuthorDropdown = true;
              updateField('authorText', ($event.target as HTMLInputElement).value);
            "
          />
          <div
            v-if="showAuthorDropdown && matchedAuthors.length > 0"
            class="absolute top-full left-0 w-full mt-1 bg-white border border-slate-200 rounded-lg shadow-lg z-50 max-h-40 overflow-y-auto"
          >
            <button
              v-for="author in matchedAuthors"
              :key="author"
              class="w-full text-left px-3 py-2 text-sm hover:bg-slate-50 transition-colors"
              @click="
                updateField('authorText', author);
                showAuthorDropdown = false;
              "
            >
              {{ author }}
            </button>
          </div>
        </div>
      </div>

      <!-- Year Range -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Year</label>
        <div class="flex items-center gap-2">
          <input
            type="number"
            placeholder="From"
            class="flex-1 w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm font-mono text-center outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.yearFrom ?? ''"
            @input="
              updateField(
                'yearFrom',
                ($event.target as HTMLInputElement).value
                  ? Number(($event.target as HTMLInputElement).value)
                  : null
              )
            "
          />
          <span class="text-slate-400 text-sm">&ndash;</span>
          <input
            type="number"
            placeholder="To"
            class="flex-1 w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm font-mono text-center outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.yearTo ?? ''"
            @input="
              updateField(
                'yearTo',
                ($event.target as HTMLInputElement).value
                  ? Number(($event.target as HTMLInputElement).value)
                  : null
              )
            "
          />
        </div>
      </div>

      <!-- Journal -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Journal</label>
        <input
          type="text"
          placeholder="Filter by journal..."
          class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
          :value="filter.journal"
          @input="updateField('journal', ($event.target as HTMLInputElement).value)"
        />
      </div>

      <!-- Tags -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Tags</label>
        <div class="flex flex-wrap gap-1.5">
          <button
            v-for="tag in allTags"
            :key="tag"
            class="px-2 py-0.5 rounded-lg text-[11px] font-medium transition-colors"
            :class="
              filter.tags.includes(tag)
                ? 'bg-indigo-100 text-indigo-700'
                : 'bg-slate-100 text-slate-500 hover:bg-slate-200'
            "
            @click="toggleTag(tag)"
          >
            {{ tag }}
          </button>
          <span v-if="allTags.length === 0" class="text-[11px] text-slate-400 italic">
            No tags available
          </span>
        </div>
      </div>

      <!-- Labels -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Labels</label>
        <div class="flex flex-wrap gap-1.5">
          <button
            v-for="label in allLabels"
            :key="label"
            class="px-2 py-0.5 rounded-lg text-[11px] font-medium transition-colors border"
            :class="
              filter.labels.includes(label)
                ? 'border-indigo-300 bg-indigo-50 text-indigo-700'
                : 'border-slate-200 bg-white text-slate-500 hover:border-slate-300'
            "
            @click="toggleLabel(label)"
          >
            {{ label }}
          </button>
          <span v-if="allLabels.length === 0" class="text-[11px] text-slate-400 italic">
            No labels available
          </span>
        </div>
      </div>
    </div>

    <!-- Apply button -->
    <div class="flex justify-end mt-4 pt-4 border-t border-slate-100">
      <button
        class="px-4 py-1.5 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors active:scale-95"
        @click="emit('apply')"
      >
        Apply Filters
      </button>
    </div>
  </div>
</template>
