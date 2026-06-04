<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ confidence: number | null }>();

const percentage = computed(() =>
  props.confidence !== null ? Math.round(props.confidence * 100) : 0
);

/** Number of filled segments out of 10 */
const filledCount = computed(() =>
  props.confidence !== null ? Math.round(props.confidence * 10) : 0
);

const fillClass = computed(() => {
  if (props.confidence === null) return 'bg-slate-200';
  if (props.confidence >= 0.8) return 'bg-indigo-500';
  if (props.confidence >= 0.5) return 'bg-indigo-300';
  return 'bg-slate-300';
});
</script>

<template>
  <div class="flex flex-col items-center w-16 gap-0.5">
    <!-- Percentage label on top -->
    <span class="text-[9px] font-mono text-slate-500 leading-none">
      {{ confidence !== null ? `${percentage}%` : '---' }}
    </span>
    <!-- 10-segment dot bar -->
    <div class="flex items-center gap-[2px]">
      <span
        v-for="i in 10"
        :key="i"
        class="inline-block w-[4px] h-[4px] rounded-[1px]"
        :class="i <= filledCount ? fillClass : 'bg-slate-100'"
      />
    </div>
  </div>
</template>
