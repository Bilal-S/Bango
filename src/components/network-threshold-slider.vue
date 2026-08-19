<template>
  <div>
    <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
      <span>{{ label }}</span>
      <span class="font-semibold tabular-nums">{{ modelValue }}</span>
    </label>
    <input
      :value="modelValue"
      type="range"
      :min="min"
      :max="max"
      :step="step"
      class="w-full accent-indigo-600"
      @input="onInput"
      @change="onChange"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * Shared labeled threshold slider for the bibliometric controls sidebars.
 *
 * Emits `input` (with the numeric value) on every tick and `commit` on
 * release, so parents can choose live filtering (citation/co-author) or
 * commit-on-release semantics (keyword) by which events they bind.
 */
const props = defineProps<{
  modelValue: number;
  label: string;
  min: number;
  max: number;
  step?: number;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void;
  (e: 'input', value: number): void;
  (e: 'commit'): void;
}>();

function onInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  emit('update:modelValue', value);
  emit('input', value);
}

function onChange() {
  emit('commit');
}
</script>
