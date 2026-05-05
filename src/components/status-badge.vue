<script setup lang="ts">
import { computed } from 'vue';
import type { ArticleStatus } from '@/types';

const props = defineProps<{ status: ArticleStatus }>();

const statusConfig = computed(() => {
  switch (props.status) {
    case 'imported':
      return { label: 'Imported', classes: 'bg-blue-100 text-blue-800' };
    case 'working':
      return { label: 'Working', classes: 'bg-amber-100 text-amber-800' };
    case 'included':
      return { label: 'Included', classes: 'bg-emerald-100 text-emerald-800' };
    case 'rejected':
      return { label: 'Rejected', classes: 'bg-rose-100 text-rose-800' };
    default:
      return { label: 'Unknown', classes: 'bg-slate-100 text-slate-600' };
  }
});
</script>

<template>
  <span
    class="px-2.5 py-0.5 rounded-full text-[11px] font-semibold uppercase tracking-tight"
    :class="statusConfig.classes"
  >
    {{ statusConfig.label }}
  </span>
</template>
