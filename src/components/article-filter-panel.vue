<script setup lang="ts">
import { ref, computed } from 'vue';
import type { ArticleFilter } from '@/composables/use-article-search';
import type { TitleMatchType } from '@/composables/use-article-search';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { getColorScheme, type ColorScheme } from '@/utils/color';
import SuggestInput from '@/components/suggest-input.vue';

const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();

const props = defineProps<{
  filter: ArticleFilter;
  allAuthors: string[];
  allTags: string[];
  allLabels: string[];
}>();

const emit = defineEmits<{
  apply: [];
  clear: [];
  close: [];
  'update:filter': [key: keyof ArticleFilter, value: unknown];
}>();

const MATCH_TYPES: { value: TitleMatchType; label: string }[] = [
  { value: 'starts_with', label: 'Starts with' },
  { value: 'contains', label: 'Contains' },
  { value: 'ends_with', label: 'Ends with' },
  { value: 'exact', label: 'Exact' },
];

const yearRangeInvalid = computed((): boolean => {
  const from = props.filter.yearFrom;
  const to = props.filter.yearTo;
  if (from !== null && from < 1850) return true;
  if (to !== null && to > 2100) return true;
  return from !== null && to !== null && from > to;
});

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

/**
 * Remove an excluded tag entirely (from `excludedTags`), without moving it
 * anywhere. This is the `x` handler on an excluded (NOT:) pill; the inclusion
 * `x` handler (`toggleTag`) would wrongly re-add the name to `tags` because it
 * is absent there, so excluded pills need their own remover.
 */
function removeExcludedTag(tag: string): void {
  updateField(
    'excludedTags',
    props.filter.excludedTags.filter((t) => t !== tag)
  );
}

/** Mirror of {@link removeExcludedTag} for labels. */
function removeExcludedLabel(label: string): void {
  updateField(
    'excludedLabels',
    props.filter.excludedLabels.filter((l) => l !== label)
  );
}

/**
 * Toggle a tag between inclusion (`tags`) and exclusion (`excludedTags`).
 * Called when the user clicks the pill body (not the `x` remove button).
 * A pill can be in three states: absent, included (default), or excluded
 * (NOT-filter, rendered with a bold `NOT:` prefix). Clicking the pill body
 * flips included <-> excluded; the `x` button removes it entirely.
 */
function toggleTagNegation(tag: string): void {
  if (props.filter.tags.includes(tag)) {
    // included -> excluded
    updateField(
      'tags',
      props.filter.tags.filter((t) => t !== tag)
    );
    updateField('excludedTags', [...props.filter.excludedTags, tag]);
  } else if (props.filter.excludedTags.includes(tag)) {
    // excluded -> included
    updateField(
      'excludedTags',
      props.filter.excludedTags.filter((t) => t !== tag)
    );
    updateField('tags', [...props.filter.tags, tag]);
  }
}

/** Mirror of {@link toggleTagNegation} for labels. */
function toggleLabelNegation(label: string): void {
  if (props.filter.labels.includes(label)) {
    updateField(
      'labels',
      props.filter.labels.filter((l) => l !== label)
    );
    updateField('excludedLabels', [...props.filter.excludedLabels, label]);
  } else if (props.filter.excludedLabels.includes(label)) {
    updateField(
      'excludedLabels',
      props.filter.excludedLabels.filter((l) => l !== label)
    );
    updateField('labels', [...props.filter.labels, label]);
  }
}

/** Color scheme for a tag (custom or hash-derived). */
function tagColor(name: string): ColorScheme {
  return getColorScheme(name, tagsStore.tags.find((t) => t.name === name)?.color);
}

/** Color scheme for a label (custom or hash-derived). */
function labelColor(name: string): ColorScheme {
  return getColorScheme(name, labelsStore.labels.find((l) => l.name === name)?.color);
}

const showAuthorDropdown = ref(false);
const tagInputValue = ref('');
const labelInputValue = ref('');

/**
 * Tags not yet active in any filter role (drives the add combobox dropdown).
 * A name already present in `tags` (inclusion) OR `excludedTags` (exclusion)
 * is hidden so the dropdown never offers a duplicate.
 */
const availableTags = computed((): string[] =>
  props.allTags.filter(
    (t) => !props.filter.tags.includes(t) && !props.filter.excludedTags.includes(t)
  )
);

/**
 * Labels not yet active in any filter role (drives the add combobox dropdown).
 * See {@link availableTags}.
 */
const availableLabels = computed((): string[] =>
  props.allLabels.filter(
    (l) => !props.filter.labels.includes(l) && !props.filter.excludedLabels.includes(l)
  )
);

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
      <div class="flex items-center gap-2">
        <button
          class="text-xs text-slate-500 hover:text-indigo-600 transition-colors font-medium"
          @click="emit('clear')"
        >
          Clear All
        </button>
        <button
          class="p-1 rounded-md text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors"
          title="Close filters"
          @click="emit('close')"
        >
          ×
        </button>
      </div>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
      <!-- Title -->
      <div class="min-w-0">
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Title</label>
        <div class="flex items-center gap-2 min-w-0">
          <select
            class="shrink-0 w-32 bg-slate-50 border border-slate-200 rounded-lg px-2 py-1.5 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
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
            class="flex-1 min-w-0 bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :value="filter.titleText"
            @input="updateField('titleText', ($event.target as HTMLInputElement).value)"
          />
        </div>
      </div>

      <!-- Author -->
      <div class="min-w-0">
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Author</label>
        <div class="relative">
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
            min="1850"
            placeholder="From"
            class="no-spinner flex-1 w-full bg-slate-50 border rounded-lg px-3 py-2 text-sm font-mono text-center outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :class="yearRangeInvalid ? 'border-red-300' : 'border-slate-200'"
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
            max="2100"
            placeholder="To"
            class="no-spinner flex-1 w-full bg-slate-50 border rounded-lg px-3 py-2 text-sm font-mono text-center outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
            :class="yearRangeInvalid ? 'border-red-300' : 'border-slate-200'"
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
        <p v-if="yearRangeInvalid" class="mt-1.5 text-xs text-red-500">
          Year must be between 1850–2100 and From ≤ To
        </p>
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
        <div class="flex items-baseline justify-between mb-2">
          <label class="block text-label-caps text-slate-500 uppercase">Tags</label>
          <span class="text-[10px] text-slate-400 italic">Click tag to toggle exclude.</span>
        </div>
        <!-- Active removable pills (bounded height so many tags never grow the panel).
             A pill can be included (default) or excluded (NOT-filter, bold "NOT:"
             prefix). Clicking the pill body flips its role; the "x" button removes
             it entirely. -->
        <div
          v-if="filter.tags.length > 0 || filter.excludedTags.length > 0"
          class="flex flex-wrap gap-1.5 mb-2 max-h-32 overflow-y-auto"
        >
          <span
            v-for="tag in filter.tags"
            :key="tag"
            class="afp-pill inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] font-medium border cursor-pointer select-none"
            :style="{
              backgroundColor: tagColor(tag).bg,
              color: tagColor(tag).text,
              borderColor: tagColor(tag).base,
            }"
            title="Click to toggle NOT (exclude this tag)"
            @click="toggleTagNegation(tag)"
          >
            {{ tag }}
            <button
              type="button"
              class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
              :title="`Remove ${tag}`"
              @click.stop="toggleTag(tag)"
            >
              ×
            </button>
          </span>
          <span
            v-for="tag in filter.excludedTags"
            :key="`not-${tag}`"
            class="afp-pill afp-pill--excluded inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] border cursor-pointer select-none"
            :style="{
              backgroundColor: tagColor(tag).bg,
              color: tagColor(tag).text,
              borderColor: tagColor(tag).base,
            }"
            title="Click to remove NOT (include this tag)"
            @click="toggleTagNegation(tag)"
          >
            <span class="font-bold afp-pill__not">NOT:</span>
            <span class="afp-pill__name--excluded">{{ tag }}</span>
            <button
              type="button"
              class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
              :title="`Remove ${tag}`"
              @click.stop="removeExcludedTag(tag)"
            >
              ×
            </button>
          </span>
        </div>
        <!-- Search-and-add combobox (dropdown already has its own max-h-40 scroll) -->
        <SuggestInput
          v-if="allTags.length > 0"
          v-model="tagInputValue"
          :suggestions="availableTags"
          placeholder="Search tags to add..."
          @select="toggleTag"
        />
        <span v-else class="text-[11px] text-slate-400 italic">No tags available</span>
      </div>

      <!-- Labels -->
      <div>
        <div class="flex items-baseline justify-between mb-2">
          <label class="block text-label-caps text-slate-500 uppercase">Labels</label>
          <span class="text-[10px] text-slate-400 italic">Click label to toggle exclude.</span>
        </div>
        <!-- Active removable pills. See the Tags section above for the
             included / excluded (NOT:) pill behavior. -->
        <div
          v-if="filter.labels.length > 0 || filter.excludedLabels.length > 0"
          class="flex flex-wrap gap-1.5 mb-2 max-h-32 overflow-y-auto"
        >
          <span
            v-for="label in filter.labels"
            :key="label"
            class="afp-pill inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] font-medium border cursor-pointer select-none"
            :style="{
              backgroundColor: labelColor(label).bg,
              color: labelColor(label).text,
              borderColor: labelColor(label).base,
            }"
            title="Click to toggle NOT (exclude this label)"
            @click="toggleLabelNegation(label)"
          >
            {{ label }}
            <button
              type="button"
              class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
              :title="`Remove ${label}`"
              @click.stop="toggleLabel(label)"
            >
              ×
            </button>
          </span>
          <span
            v-for="label in filter.excludedLabels"
            :key="`not-${label}`"
            class="afp-pill afp-pill--excluded inline-flex items-center gap-1 px-2 py-0.5 rounded-lg text-[11px] border cursor-pointer select-none"
            :style="{
              backgroundColor: labelColor(label).bg,
              color: labelColor(label).text,
              borderColor: labelColor(label).base,
            }"
            title="Click to remove NOT (include this label)"
            @click="toggleLabelNegation(label)"
          >
            <span class="font-bold afp-pill__not">NOT:</span>
            <span class="afp-pill__name--excluded">{{ label }}</span>
            <button
              type="button"
              class="flex items-center justify-center w-3.5 h-3.5 rounded-full hover:bg-black/10 text-[10px] leading-none transition-colors"
              :title="`Remove ${label}`"
              @click.stop="removeExcludedLabel(label)"
            >
              ×
            </button>
          </span>
        </div>
        <!-- Search-and-add combobox (dropdown already has its own max-h-40 scroll) -->
        <SuggestInput
          v-if="allLabels.length > 0"
          v-model="labelInputValue"
          :suggestions="availableLabels"
          placeholder="Search labels to add..."
          @select="toggleLabel"
        />
        <span v-else class="text-[11px] text-slate-400 italic">No labels available</span>
      </div>
    </div>

    <!-- Apply button -->
    <div class="flex justify-end mt-4 pt-4 border-t border-slate-100">
      <button
        class="px-4 py-1.5 rounded-lg text-sm font-medium transition-colors active:scale-95"
        :class="
          yearRangeInvalid
            ? 'bg-slate-300 text-slate-500 cursor-not-allowed'
            : 'bg-indigo-600 text-white hover:bg-indigo-700'
        "
        :disabled="yearRangeInvalid"
        @click="emit('apply')"
      >
        Apply Filters
      </button>
    </div>
  </div>
</template>

<style scoped>
.no-spinner::-webkit-inner-spin-button,
.no-spinner::-webkit-outer-spin-button {
  -webkit-appearance: none;
  margin: 0;
}
.no-spinner {
  appearance: textfield;
  -moz-appearance: textfield;
}

/* Excluded (NOT:) pills: the bold "NOT:" prefix signals the negation; the
   actual tag/label name gets a strike-through line so the "exclude" intent is
   unmistakable even when color alone is subtle. The line targets only the
   name span (`.afp-pill__name--excluded`), skipping the "NOT:" prefix and the
   remove ("x") button. */
.afp-pill--excluded {
  opacity: 0.85;
}
.afp-pill__name--excluded {
  text-decoration: line-through;
  text-decoration-color: rgba(0, 0, 0, 0.45);
  text-decoration-thickness: 1.5px;
}
.afp-pill:hover {
  filter: brightness(0.97);
}
</style>
