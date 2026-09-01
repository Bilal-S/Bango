<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import type { SuggestOption } from '@/types';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    /**
     * Legacy flat-string suggestions. Ignored when `options` is provided.
     * Optional now so options-only consumers (e.g. the journal autocomplete)
     * can omit it.
     */
    suggestions?: string[];
    /**
     * Structured suggestions. When provided, takes precedence over
     * `suggestions`. Powers the article-metadata journal autocomplete.
     */
    options?: SuggestOption[];
    placeholder: string;
    /**
     * Controls post-selection behavior.
     *
     * - `true` (default): the input is cleared and the dropdown collapses
     *   (reset), so it never obscures the surrounding UI. Focus stays in the
     *   input - typing or clicking it reopens the dropdown for the next pick.
     *   Intended for multi-add consumers (`tags-section`, `labels-section`,
     *   `article-filter-panel`, `criteria-edit-dialog`) whose `@select`
     *   handler applies each pick right away.
     * - `false`: the selected value populates the input and the dropdown
     *   closes. Intended for single-select consumers (e.g. the bulk
     *   add-tag/add-label dialogs in `article-list.vue`, and the journal
     *   autocomplete) where the user picks exactly one value.
     */
    clearOnSelect?: boolean;
    /**
     * Values that are rendered but greyed out and unselectable. Used by the
     * article-detail tag/label sections to surface already-assigned items as
     * disabled (instead of hiding them) so the user can see they exist while
     * typing. Selecting a disabled row is a no-op (no `select` emit). The
     * values are still subject to the normal substring filter. Compared by
     * label (case-insensitive).
     */
    disabledSuggestions?: string[];
  }>(),
  { clearOnSelect: true, suggestions: () => [], options: () => [], disabledSuggestions: () => [] }
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  /**
   * Emitted on row click/Enter. The first argument is always the label string
   * (backward-compatible with string-mode consumers). When `options` is in
   * use, the matching `SuggestOption` object is emitted as the second argument
   * so the parent can read the `id` (e.g. to link an article to a journal).
   */
  select: [name: string, option?: SuggestOption];
  enter: [text: string];
  /**
   * Emitted on Escape. The component closes its own dropdown, then bubbles the
   * Escape so a single-select parent (e.g. the journal autocomplete in
   * `article-metadata.vue`) can cancel the whole edit. Without this the parent
   * has no way to react to Escape because the keystroke is consumed here.
   */
  escape: [];
}>();

const isOpen = ref(false);
const openUpward = ref(false);
const containerRef = ref<HTMLDivElement | null>(null);

/** Dropdown footprint: max-h-40 (10rem) + 4px gap + slack. */
const DROPDOWN_FOOTPRINT = 176;

/**
 * Open the dropdown, flipping upward when the space below the input is
 * insufficient. The clipping edge is the nearest scrollable ancestor's bottom
 * (e.g. a dialog body with overflow-y-auto), capped by the viewport; degenerate
 * measurements (zero-size rects, e.g. test environments without real layout)
 * fall back to the downward default.
 */
function openDropdown(): void {
  const container = containerRef.value;
  const input = container?.querySelector('input');
  if (container && input) {
    const inputRect = input.getBoundingClientRect();
    if (inputRect.height !== 0 || inputRect.bottom !== 0) {
      let clipTop = 0;
      let clipBottom = window.innerHeight;
      let node: HTMLElement | null = container.parentElement;
      while (node) {
        const overflowY = window.getComputedStyle(node).overflowY;
        if (overflowY === 'auto' || overflowY === 'scroll') {
          const rect = node.getBoundingClientRect();
          if (rect.height > 0) {
            clipTop = Math.max(clipTop, rect.top);
            clipBottom = Math.min(clipBottom, rect.bottom);
            break;
          }
        }
        node = node.parentElement;
      }
      const spaceBelow = clipBottom - inputRect.bottom;
      const spaceAbove = inputRect.top - clipTop;
      openUpward.value = spaceBelow < DROPDOWN_FOOTPRINT && spaceAbove > spaceBelow;
    } else {
      openUpward.value = false;
    }
  } else {
    openUpward.value = false;
  }
  isOpen.value = true;
}

/** True when the structured `options` mode is active (takes precedence). */
const useOptions = computed((): boolean => props.options.length > 0);

/**
 * Unified filtered list. In string mode, filters `suggestions` by substring on
 * the trimmed query. In options mode, filters `options` by substring on label
 * or sublabel. An empty query returns the full list (same as string mode).
 */
const filteredSuggestions = computed<(string | SuggestOption)[]>(() => {
  const query = props.modelValue.trim().toLowerCase();
  if (useOptions.value) {
    if (!query) return props.options;
    return props.options.filter(
      (o) =>
        o.label.toLowerCase().includes(query) ||
        (o.sublabel ? o.sublabel.toLowerCase().includes(query) : false)
    );
  }
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

/** Extract the display label from either a string or a `SuggestOption` row. */
function rowLabel(row: string | SuggestOption): string {
  return typeof row === 'string' ? row : row.label;
}

/** Stable Vue `:key` for either row shape (string label or option id). */
function rowKey(row: string | SuggestOption): string {
  return typeof row === 'string' ? row : row.id;
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
  openDropdown();
}

function onFocus(): void {
  openDropdown();
}

function selectSuggestion(row: string | SuggestOption): void {
  const name = rowLabel(row);
  /* Disabled rows must never fire a selection. `@mousedown` in template also
     guards, but this is the authoritative gate (defense-in-depth). */
  if (isDisabled(name)) return;
  // In options mode, pass the full object so the parent can read the id.
  const option = typeof row === 'string' ? undefined : row;
  emit('select', name, option);
  if (props.clearOnSelect) {
    /* Reset: clear the input and collapse the dropdown so it never obscures
       the surrounding UI. Focus stays in the input; typing or clicking it
       reopens for the next pick. */
    emit('update:modelValue', '');
    isOpen.value = false;
  } else {
    /* Single-select: populate input with chosen value for review, close
       dropdown. Parent reads via v-model on subsequent confirm action. */
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
        // Reset like selectSuggestion above: clear + collapse (no blur).
        emit('update:modelValue', '');
        isOpen.value = false;
      }
      /* Single-select: parent's @enter handler is expected to close the dialog
         (or consume the value). We leave the input populated, dropdown untouched. */
    }
  } else if (event.key === 'Escape') {
    isOpen.value = false;
    /* Bubble Escape so single-select parent can cancel the edit. Multi-add
       consumers ignore this (selections already collapse the dropdown). */
    emit('escape');
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
      @click="openDropdown"
      @keydown="onKeydown"
    />
    <ul
      v-if="isOpen && filteredSuggestions.length > 0"
      class="absolute z-50 left-0 right-0 mt-1 bg-white border border-slate-200 rounded-lg shadow-lg max-h-40 overflow-y-auto"
      :class="openUpward ? 'bottom-[calc(100%+4px)]' : ''"
    >
      <li
        v-for="row in filteredSuggestions"
        :key="rowKey(row)"
        class="flex items-center justify-between gap-2 px-3 py-1.5 text-xs transition-colors"
        :class="
          isDisabled(rowLabel(row))
            ? 'text-slate-400 cursor-not-allowed bg-slate-50'
            : 'text-slate-700 hover:bg-indigo-50 hover:text-indigo-700 cursor-pointer'
        "
        :title="isDisabled(rowLabel(row)) ? 'Already added' : ''"
        @mousedown.prevent="!isDisabled(rowLabel(row)) && selectSuggestion(row)"
      >
        <!-- Structured-options mode: label + optional publisher sublabel + ISSN badge -->
        <template v-if="typeof row !== 'string'">
          <span class="flex flex-col min-w-0">
            <span class="truncate font-medium">
              <template v-if="highlightParts(row.label, trimmedQuery)">
                {{ highlightParts(row.label, trimmedQuery)![0]
                }}<mark class="bg-indigo-200 text-indigo-900 rounded px-0.5">{{
                  highlightParts(row.label, trimmedQuery)![1]
                }}</mark
                >{{ highlightParts(row.label, trimmedQuery)![2] }}
              </template>
              <template v-else>{{ row.label }}</template>
            </span>
            <span v-if="row.sublabel" class="truncate text-[10px] text-slate-400">{{
              row.sublabel
            }}</span>
          </span>
          <span
            v-if="row.badge"
            class="shrink-0 rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-mono text-slate-500"
            >{{ row.badge }}</span
          >
        </template>
        <!-- Legacy string mode -->
        <template v-else>
          <span>
            <template v-if="highlightParts(row, trimmedQuery)">
              {{ highlightParts(row, trimmedQuery)![0]
              }}<mark class="bg-indigo-200 text-indigo-900 rounded px-0.5">{{
                highlightParts(row, trimmedQuery)![1]
              }}</mark
              >{{ highlightParts(row, trimmedQuery)![2] }}
            </template>
            <template v-else>{{ row }}</template>
          </span>
        </template>
        <span
          v-if="isDisabled(rowLabel(row))"
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
