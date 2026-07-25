<script setup lang="ts">
import { computed } from 'vue';
import { getColorScheme } from '@/utils/color';

const props = defineProps<{
  name: string;
  color?: string | null;
  /**
   * When true, renders a strong indigo halo (ring + glow) around the chip.
   * Used by `labels-section` to surface already-assigned chips whose name
   * contains the substring typed into the add input. Mirrors `tag-chip.vue`.
   */
  highlight?: boolean;
}>();

const scheme = computed(() => getColorScheme(props.name, props.color));

/**
 * Inline styles applied only when `highlight` is true. See `tag-chip.vue` for
 * the rationale; the indigo glow matches the SuggestInput `<mark>` highlight.
 */
const highlightStyle = computed(() =>
  props.highlight
    ? {
        boxShadow: '0 0 0 2px rgba(99, 102, 241, 0.35), 0 0 8px 2px rgba(99, 102, 241, 0.25)',
      }
    : {}
);
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border bg-transparent font-mono text-mono transition-shadow"
    :class="highlight ? 'ring-2 ring-indigo-500 ring-offset-1' : ''"
    :style="{
      borderColor: scheme.border,
      color: scheme.text,
      ...highlightStyle,
    }"
  >
    <span class="w-1.5 h-1.5 rounded-full" :style="{ backgroundColor: scheme.base }"></span>
    {{ name }}
  </span>
</template>
