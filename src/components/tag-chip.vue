<script setup lang="ts">
import { computed } from 'vue';

const TAG_COLORS = [
  { bg: 'bg-blue-100', text: 'text-blue-800', border: 'border-blue-200' },
  { bg: 'bg-green-100', text: 'text-green-800', border: 'border-green-200' },
  { bg: 'bg-purple-100', text: 'text-purple-800', border: 'border-purple-200' },
  { bg: 'bg-amber-100', text: 'text-amber-800', border: 'border-amber-200' },
  { bg: 'bg-cyan-100', text: 'text-cyan-800', border: 'border-cyan-200' },
  { bg: 'bg-rose-100', text: 'text-rose-800', border: 'border-rose-200' },
] as const;

const props = defineProps<{ name: string }>();

const colorClass = computed(() => {
  let hash = 0;
  for (let i = 0; i < props.name.length; i++) {
    hash = (hash * 31 + props.name.charCodeAt(i)) | 0;
  }
  return TAG_COLORS[Math.abs(hash) % TAG_COLORS.length]!;
});
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg font-mono text-mono border"
    :class="[colorClass.bg, colorClass.text, colorClass.border]"
  >
    {{ name }}
  </span>
</template>
