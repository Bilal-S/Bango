<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    suggestions: string[];
    placeholder: string;
    /**
     * Controls post-selection behavior.
     *
     * - `true` (default): the input is cleared and the dropdown stays open so
     *   the user can immediately add another entry. Intended for multi-add
     *   consumers (`tags-section`, `labels-section`, `article-filter-panel`)
     *   whose `@select` handler applies each pick right away.
     * - `false`: the selected value populates the input and the dropdown
     *   closes. Intended for single-select consumers (e.g. the bulk
     *   add-tag/add-label dialogs in `article-list.vue`) where the user picks
     *   exactly one value and then confirms via a separate action button.
     */
    clearOnSelect?: boolean;
    /**
     * Values that are rendered but greyed out and unselectable. Used by the
     * article-detail tag/label sections to surface already-assigned items as
     * disabled (instead of hiding them) so the user can see they exist while
     * typing. Selecting a disabled row is a no-op (no `select` emit). The
     * values are still subject to the normal substring filter.
     */
    disabledSuggestions?: string[];
  }>(),
  { clearOnSelect: true, disabledSuggestions: () => [] }
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  select: [name: string];
  enter: [text: string];
}>();

const isOpen = ref(false);
const containerRef = ref<HTMLDivElement | null>(null);

const filteredSuggestions = computed(() => {
  const query = props.modelValue.trim().toLowerCase();
  if (!query) return props.suggestions;
  return props.suggestions.filter((s) => s.toLowerCase().includes(query));
});

/**
 * Case-insensitive membership lookup for `disabledSuggestions`. Lowercases the
 * values once per render so the per-row `isDisabled` check is O(1) instead of
 * an O(n) `.includes` over the array for every rendered row.
 */
const disabledSet = computed(
  (): Set<string> => new Set(props.disabledSuggestions.map((s) => s.toLowerCase()))
);

/** True when this row value is already assigned and must render grey + unselectable. */
function isDisabled(suggestion: string): boolean {
  return disabledSet.value.has(suggestion.toLowerCase());
}

/**
 * The current trimmed query used to drive the matched-substring `<mark>`.
 * Reactively re-derived so the highlight tracks every keystroke.
 */
const trimmedQuery = computed((): string => props.modelValue.trim().toLowerCase());

/**
 * Split a suggestion into `[before, match, after]` around the first
 * case-insensitive occurrence of the query. Returns `null` when there is no
 * query or no match, so the caller can render the plain text unchanged. The
 * match is performed on the lowercased query so the user sees highlighting for
 * *any* substring they type, reinforcing that matching is not prefix-only.
 */
function highlightParts(suggestion: string, query: string): [string, string, string] | null {
  if (!query) return null;
  const idx = suggestion.toLowerCase().indexOf(query);
  if (idx === -1) return null;
  return [
    suggestion.slice(0, idx),
    suggestion.slice(idx, idx + query.length),
    suggestion.slice(idx + query.length),
  ];
}

/** True when the current input value matches a disabled (already-assigned) row exactly (case-insensitive). */
const enteredValueMatchesDisabled = computed((): boolean => {
  const q = trimmedQuery.value;
  if (!q) return false;
  return disabledSet.value.has(q);
});

function onInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value;
  emit('update:modelValue', value);
  isOpen.value = true;
}

function onFocus(): void {
  isOpen.value = true;
}

function selectSuggestion(name: string): void {
  // Disabled rows are visually present but must never fire a selection. The
  // `@mousedown` handler in the template guards the call site too, but this
  // is the authoritative gate (defense-in-depth if a future caller invokes
  // the method directly).
  if (isDisabled(name)) return;
  emit('select', name);
  if (props.clearOnSelect) {
    // Clear the input and keep the dropdown open so the user can immediately
    // add another entry. The parent's @select handler updates the article +
    // refreshes the suggestions list, so the dropdown re-populates with the
    // remaining (un-assigned) entries. This matches the "revert to initial
    // state with dropdown open" UX requested for the tags/labels flow.
    emit('update:modelValue', '');
    isOpen.value = true;
  } else {
    // Single-select mode: populate the input with the chosen value so the
    // user can review it, then close the dropdown. The parent reads the
    // value via v-model on a subsequent confirm action (e.g. a button), so
    // it must survive the selection.
    emit('update:modelValue', name);
    isOpen.value = false;
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault();
    const val = props.modelValue.trim();
    if (val) {
      emit('enter', val);
      if (props.clearOnSelect) {
        // Clear the input and keep the dropdown open (same rationale as
        // selectSuggestion above) instead of closing + blurring.
        emit('update:modelValue', '');
        isOpen.value = true;
      }
      // In single-select mode the parent's @enter handler is expected to
      // close the dialog (or otherwise consume the value), so we leave the
      // input populated and the dropdown state untouched.
    }
  } else if (event.key === 'Escape') {
    isOpen.value = false;
  }
}

function handleClickOutside(event: MouseEvent): void {
  if (containerRef.value && !containerRef.value.contains(event.target as Node)) {
    isOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside);
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
});
</script>

<template>
  <div ref="containerRef" class="relative">
    <input
      :value="modelValue"
      type="text"
      :placeholder="placeholder"
      class="flex-1 w-full text-xs border border-slate-200 rounded-lg px-2.5 py-1.5 focus:outline-none focus:ring-1 focus:ring-indigo-400"
      @input="onInput"
      @focus="onFocus"
      @click="isOpen = true"
      @keydown="onKeydown"
    />
    <ul
      v-if="isOpen && filteredSuggestions.length > 0"
      class="absolute z-50 left-0 right-0 mt-1 bg-white border border-slate-200 rounded-lg shadow-lg max-h-40 overflow-y-auto"
    >
      <li
        v-for="suggestion in filteredSuggestions"
        :key="suggestion"
        class="flex items-center justify-between gap-2 px-3 py-1.5 text-xs transition-colors"
        :class="
          isDisabled(suggestion)
            ? 'text-slate-400 cursor-not-allowed bg-slate-50'
            : 'text-slate-700 hover:bg-indigo-50 hover:text-indigo-700 cursor-pointer'
        "
        :title="isDisabled(suggestion) ? 'Already added' : ''"
        @mousedown.prevent="!isDisabled(suggestion) && selectSuggestion(suggestion)"
      >
        <span>
          <template v-if="highlightParts(suggestion, trimmedQuery)">
            {{ highlightParts(suggestion, trimmedQuery)![0]
            }}<mark class="bg-indigo-200 text-indigo-900 rounded px-0.5">{{
              highlightParts(suggestion, trimmedQuery)![1]
            }}</mark
            >{{ highlightParts(suggestion, trimmedQuery)![2] }}
          </template>
          <template v-else>{{ suggestion }}</template>
        </span>
        <span
          v-if="isDisabled(suggestion)"
          class="material-symbols-outlined text-[14px] text-slate-400 leading-none"
          >check</span
        >
      </li>
    </ul>
    <!-- Hint shown when the typed value exactly matches an already-assigned item,
         so the user understands why the dropdown row is disabled. -->
    <p v-if="enteredValueMatchesDisabled" class="mt-1 text-[10px] text-slate-400 italic">
      Already added.
    </p>
  </div>
</template>
