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
  }>(),
  { clearOnSelect: true }
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

function onInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value;
  emit('update:modelValue', value);
  isOpen.value = true;
}

function onFocus(): void {
  isOpen.value = true;
}

function selectSuggestion(name: string): void {
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
        class="px-3 py-1.5 text-xs text-slate-700 hover:bg-indigo-50 hover:text-indigo-700 cursor-pointer transition-colors"
        @mousedown.prevent="selectSuggestion(suggestion)"
      >
        {{ suggestion }}
      </li>
    </ul>
  </div>
</template>
