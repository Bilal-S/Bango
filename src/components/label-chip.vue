<script setup lang="ts">
import { computed } from 'vue';
import { getColorScheme } from '@/utils/color';

const props = defineProps<{ name: string; color?: string | null }>();

const scheme = computed(() => getColorScheme(props.name, props.color));
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border bg-transparent font-mono text-mono"
    :style="{
      borderColor: scheme.border,
      color: scheme.text,
    }"
  >
    <span class="w-1.5 h-1.5 rounded-full" :style="{ backgroundColor: scheme.base }"></span>
    {{ name }}
  </span>
</template>
