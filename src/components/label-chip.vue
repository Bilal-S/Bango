<script setup lang="ts">
import { computed } from 'vue';

const LABEL_COLORS = [
  { border: 'border-red-300', text: 'text-red-700', dot: 'bg-red-500' },
  { border: 'border-orange-300', text: 'text-orange-700', dot: 'bg-orange-500' },
  { border: 'border-amber-300', text: 'text-amber-700', dot: 'bg-amber-500' },
  { border: 'border-slate-300', text: 'text-slate-700', dot: 'bg-slate-400' },
  { border: 'border-blue-300', text: 'text-blue-700', dot: 'bg-blue-500' },
  { border: 'border-violet-300', text: 'text-violet-700', dot: 'bg-violet-500' },
] as const;

const props = defineProps<{ name: string }>();

const colorClass = computed(() => {
  let hash = 0;
  for (let i = 0; i < props.name.length; i++) {
    hash = (hash * 31 + props.name.charCodeAt(i)) | 0;
  }
  return LABEL_COLORS[Math.abs(hash) % LABEL_COLORS.length]!;
});
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border bg-transparent font-mono text-mono"
    :class="[colorClass.border, colorClass.text]"
  >
    <span class="w-1.5 h-1.5 rounded-full" :class="colorClass.dot"></span>
    {{ name }}
  </span>
</template>
