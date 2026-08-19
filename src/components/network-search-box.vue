<template>
  <div class="relative">
    <span
      class="material-symbols-outlined absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400 text-base z-10"
      >search</span
    >
    <input
      :value="modelValue"
      type="text"
      :placeholder="placeholder"
      :class="
        clearable
          ? 'w-full pl-8 pr-8 py-1.5 text-sm bg-slate-50 border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-transparent'
          : 'w-full pl-8 pr-3 py-1.5 text-sm bg-slate-50 border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-transparent'
      "
      @input="onInput"
      @keydown.enter="onEnter"
      @keydown.escape="onEscape"
      @focus="showSuggestions = true"
    />
    <!-- Clear (x) button -->
    <button
      v-if="clearable && modelValue"
      type="button"
      class="absolute right-2 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600 z-10 cursor-pointer"
      title="Clear search"
      @click="onClear"
    >
      <span class="material-symbols-outlined text-base">close</span>
    </button>
    <!-- Autocomplete dropdown -->
    <ul
      v-if="showSuggestions && suggestions.length > 0"
      class="absolute z-20 left-0 right-0 top-full mt-1 max-h-40 overflow-y-auto bg-white border border-slate-200 rounded-lg shadow-lg"
    >
      <li
        v-for="s in suggestions"
        :key="s.key"
        class="px-3 py-1.5 text-sm cursor-pointer hover:bg-indigo-50 text-slate-700 truncate"
        @mousedown.prevent="onSelect(s)"
      >
        {{ s.display }}
        <span v-if="s.detail" class="text-xs text-slate-400 ml-1">({{ s.detail }})</span>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import type { NetworkSearchSuggestion } from '../types/network-graph';

/**
 * Shared search box + autocomplete for the bibliometric controls sidebars.
 *
 * Emits `input` on every keystroke (parents re-emit their filter payloads),
 * `select` / `select-first` on suggestion click / Enter (parents locate the
 * node; some deliberately skip the filter emit there - that logic stays in
 * the parent), and `clear` on the x button. Escape only closes the dropdown
 * internally (no parent consumes it).
 */
const props = defineProps<{
  modelValue: string;
  placeholder: string;
  suggestions: NetworkSearchSuggestion[];
  /** Show the clear (x) affordance (citation + cocitation sidebars). */
  clearable?: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'input', value: string): void;
  (e: 'select', suggestion: NetworkSearchSuggestion): void;
  (e: 'select-first', suggestion: NetworkSearchSuggestion): void;
  (e: 'clear'): void;
}>();

const showSuggestions = ref(false);

function onInput(event: Event) {
  showSuggestions.value = true;
  const value = (event.target as HTMLInputElement).value;
  emit('update:modelValue', value);
  emit('input', value);
}

function onEnter() {
  const first = props.suggestions[0];
  if (!first) return;
  showSuggestions.value = false;
  emit('update:modelValue', first.display);
  emit('select-first', first);
}

function onSelect(s: NetworkSearchSuggestion) {
  showSuggestions.value = false;
  emit('update:modelValue', s.display);
  emit('select', s);
}

function onClear() {
  showSuggestions.value = false;
  emit('update:modelValue', '');
  emit('clear');
}

function onEscape() {
  showSuggestions.value = false;
}
</script>
