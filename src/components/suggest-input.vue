<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';

const props = defineProps<{
  modelValue: string;
  suggestions: string[];
  placeholder: string;
}>();

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
  emit('update:modelValue', '');
  isOpen.value = false;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault();
    const val = props.modelValue.trim();
    if (val) {
      emit('enter', val);
      emit('update:modelValue', '');
      isOpen.value = false;
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
