<script setup lang="ts">
import { ref, computed } from 'vue';
import type { ArticleFilter } from '@/composables/use-article-search';
import type { TitleMatchType } from '@/composables/use-article-search';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { getColorScheme, type ColorScheme } from '@/utils/color';
import SuggestInput from '@/components/suggest-input.vue';
import ClearableInput from '@/components/clearable-input.vue';

const tagsStore = useTagsStore();
const labelsStore = useLabelsStore();

const props = defineProps<{
  filter: ArticleFilter;
  allAuthors: string[];
  allTags: string[];
  allLabels: string[];
  /** Article count from last applied query. Drives "Filter active: n article(s) found."
   *  Undefined before first apply. */
  resultCount?: number;
  /** True when any filter dimension is currently active. */
  isFiltered?: boolean;
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

/** Year-range bounds. Both fields must be in [1850, 2100]; From <= To when both set. */

const YEAR_MIN = 1850;
const YEAR_MAX = 2100;

/** True when the From year is individually out of range OR (both set) From > To. */
const yearFromInvalid = computed((): boolean => {
  const from = props.filter.yearFrom;
  const to = props.filter.yearTo;
  if (from !== null && (from < YEAR_MIN || from > YEAR_MAX)) return true;
  return from !== null && to !== null && from > to;
});

/** True when the To year is individually out of range OR (both set) From > To. */
const yearToInvalid = computed((): boolean => {
  const from = props.filter.yearFrom;
  const to = props.filter.yearTo;
  if (to !== null && (to < YEAR_MIN || to > YEAR_MAX)) return true;
  return from !== null && to !== null && from > to;
});

/** Union flag: disables Apply/Enter while either year field is invalid. */
const yearRangeInvalid = computed((): boolean => yearFromInvalid.value || yearToInvalid.value);

/** Field-aware validation hint naming the specific problem + which field to fix. */
const yearHint = computed((): string => {
  const from = props.filter.yearFrom;
  const to = props.filter.yearTo;
  const fromOutOfRange = from !== null && (from < YEAR_MIN || from > YEAR_MAX);
  const toOutOfRange = to !== null && (to < YEAR_MIN || to > YEAR_MAX);
  const rangeFlipped = from !== null && to !== null && from > to;
  if (fromOutOfRange && toOutOfRange) {
    return `Both years must be between ${YEAR_MIN}-${YEAR_MAX}.`;
  }
  if (fromOutOfRange) {
    return `From year must be between ${YEAR_MIN}-${YEAR_MAX}.`;
  }
  if (toOutOfRange) {
    return `To year must be between ${YEAR_MIN}-${YEAR_MAX}.`;
  }
  if (rangeFlipped) {
    return 'From year must be less than or equal to To year.';
  }
  return '';
});

function updateField(key: keyof ArticleFilter, value: unknown): void {
  emit('update:filter', key, value);
}

/** Apply filter on Enter. Skipped while year range invalid (user sees hint).
 *  Wired on every text input; match-type `<select>` excluded. */
function onEnterApply(): void {
  if (yearRangeInvalid.value) return;
  emit('apply');
}

/** Clear single filter field and re-submit query. Used by clearable-input "x".
 *  `emptyValue`: `''` for text, `null` for numeric year fields. */
function clearField(key: keyof ArticleFilter, emptyValue: unknown): void {
  emit('update:filter', key, emptyValue);
  emit('apply');
}

/** Close the Author autocomplete dropdown (used when clearing the Author field). */
function closeAuthorDropdown(): void {
  showAuthorDropdown.value = false;
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

/** Remove excluded tag entirely (NOT: pill `x` handler). Separate from toggleTag
    which would wrongly re-add the name to `tags`. */
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

/** Toggle tag between inclusion/exclusion. Pill body click: absent → included
    → excluded → absent. `x` button removes entirely. */
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
      <button
        class="afp-close-btn flex items-center justify-center w-7 h-7 rounded-md text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors"
        title="Close filters"
        aria-label="Close filters"
        @click="emit('close')"
      >
        <span class="material-symbols-outlined text-[20px]">close</span>
      </button>
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
          <ClearableInput
            :model-value="filter.titleText"
            placeholder="Filter by title..."
            input-class="flex-1 min-w-0"
            @update:model-value="updateField('titleText', $event)"
            @clear="clearField('titleText', '')"
            @enter="onEnterApply"
          />
        </div>
      </div>

      <!-- Author -->
      <div class="min-w-0">
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Author</label>
        <div class="relative">
          <ClearableInput
            :model-value="filter.authorText"
            placeholder="Filter by author..."
            @update:model-value="updateField('authorText', $event)"
            @input="showAuthorDropdown = true"
            @focus="showAuthorDropdown = true"
            @blur="hideAuthorDropdown()"
            @clear="
              clearField('authorText', '');
              closeAuthorDropdown();
            "
            @enter="onEnterApply"
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
          <ClearableInput
            :model-value="filter.yearFrom !== null ? String(filter.yearFrom) : ''"
            type="number"
            min="1850"
            max="2100"
            placeholder="From"
            :input-class="`no-spinner flex-1 font-mono text-center ${yearFromInvalid ? 'border-red-300' : 'border-slate-200'}`"
            @update:model-value="updateField('yearFrom', $event === '' ? null : Number($event))"
            @clear="clearField('yearFrom', null)"
            @enter="onEnterApply"
          />
          <span class="text-slate-400 text-sm">&ndash;</span>
          <ClearableInput
            :model-value="filter.yearTo !== null ? String(filter.yearTo) : ''"
            type="number"
            min="1850"
            max="2100"
            placeholder="To"
            :input-class="`no-spinner flex-1 font-mono text-center ${yearToInvalid ? 'border-red-300' : 'border-slate-200'}`"
            @update:model-value="updateField('yearTo', $event === '' ? null : Number($event))"
            @clear="clearField('yearTo', null)"
            @enter="onEnterApply"
          />
        </div>
        <p v-if="yearRangeInvalid" class="mt-1.5 text-xs text-red-500">
          {{ yearHint }}
        </p>
      </div>

      <!-- Journal -->
      <div>
        <label class="block text-label-caps text-slate-500 uppercase mb-2">Journal</label>
        <ClearableInput
          :model-value="filter.journal"
          placeholder="Filter by journal..."
          @update:model-value="updateField('journal', $event)"
          @clear="clearField('journal', '')"
          @enter="onEnterApply"
        />
      </div>

      <!-- DOI -->
      <div class="min-w-0">
        <label class="block text-label-caps text-slate-500 uppercase mb-2">DOI</label>
        <div class="flex items-center gap-2 min-w-0">
          <ClearableInput
            :model-value="filter.doiText"
            placeholder="Filter by DOI..."
            input-class="flex-1 min-w-0"
            :disabled="filter.doiEmpty"
            :title="filter.doiEmpty ? 'Clear the Only-no-DOI checkbox to search by DOI text' : ''"
            @update:model-value="updateField('doiText', $event)"
            @clear="clearField('doiText', '')"
            @enter="onEnterApply"
          />
          <label
            class="flex items-center gap-1.5 shrink-0 text-xs text-slate-600 cursor-pointer select-none whitespace-nowrap"
            title="Show only articles with no DOI (useful for data cleanup before export)"
          >
            <input
              type="checkbox"
              class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500"
              :checked="filter.doiEmpty"
              @change="updateField('doiEmpty', ($event.target as HTMLInputElement).checked)"
            />
            Only no DOI
          </label>
        </div>
      </div>
    </div>
    <!-- /3-column metadata grid (Title · Author · Year · Journal · DOI) -->

    <!-- Tags + Labels: dedicated 2-column grid so they always sit side-by-side
         on the same level, regardless of how the other 5 fields wrap above. -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6 mt-6">
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
    <!-- /Tags + Labels 2-column grid -->

    <!-- Action row: Clear Filter (left) + Apply Filters (right). Sits on the
         same level so the two complementary actions read as a pair. -->
    <div class="flex items-center justify-between gap-2 mt-4 pt-4 border-t border-slate-100">
      <button
        class="afp-clear-btn inline-flex shrink-0 items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium border border-slate-300 text-slate-600 hover:bg-slate-50 hover:text-slate-900 transition-colors active:scale-95"
        title="Clear all filters"
        @click="emit('clear')"
      >
        <span class="material-symbols-outlined text-[18px]">filter_alt_off</span>
        Clear Filter
      </button>
      <!-- Centered count notice: shown only while a filter is active so the
           user sees they are operating on a filtered list. `flex-1` claims the
           middle space so the two buttons stay pinned to the edges. -->
      <span
        v-if="isFiltered && resultCount !== undefined"
        class="afp-result-count flex-1 text-center text-xs text-slate-500 font-medium px-2"
      >
        Filter active: {{ resultCount }} article{{ resultCount === 1 ? '' : '(s)' }} found.
      </span>
      <button
        class="afp-apply-btn inline-flex shrink-0 items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-medium transition-colors active:scale-95"
        :class="
          yearRangeInvalid
            ? 'bg-slate-300 text-slate-500 cursor-not-allowed'
            : 'bg-indigo-600 text-white hover:bg-indigo-700'
        "
        :disabled="yearRangeInvalid"
        @click="emit('apply')"
      >
        <span class="material-symbols-outlined text-[18px]">filter_alt</span>
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
