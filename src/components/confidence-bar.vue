<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ confidence: number | null }>();

const percentage = computed(() =>
  props.confidence !== null ? Math.round(props.confidence * 100) : 0
);

const fillClass = computed(() => {
  if (props.confidence === null) return 'bg-slate-200';
  if (props.confidence >= 0.8) return 'bg-indigo-500';
  if (props.confidence >= 0.5) return 'bg-indigo-300';
  return 'bg-slate-300';
});
</script>

<template>
  <div class="flex items-center gap-2 w-32">
    <div class="w-full bg-slate-100 h-1.5 rounded-full overflow-hidden">
      <div
        class="h-full rounded-full transition-all duration-300"
        :class="fillClass"
        :style="{ width: `${percentage}%` }"
      />
    </div>
    <span class="text-[11px] font-mono text-slate-500 min-w-[32px] text-right">
      {{ confidence !== null ? `${percentage}%` : '---' }}
    </span>
  </div>
</template>
