<script setup lang="ts">
import { computed } from 'vue';
import { getColorScheme } from '@/utils/color';

const props = defineProps<{
  name: string;
  color?: string | null;
  /**
   * When true, renders a strong indigo halo (ring + glow) around the chip.
   * Used by `tags-section`/`labels-section` to surface already-assigned
   * chips whose name contains the substring typed into the add input, so the
   * user sees the existing match instead of having to scan the list.
   */
  highlight?: boolean;
}>();

const scheme = computed(() => getColorScheme(props.name, props.color));

/**
 * Inline styles applied only when `highlight` is true. Combining the ring
 * (Tailwind `ring-*` utilities) with an indigo box-shadow glow makes the
 * halo read as a deliberate "match" signal, consistent with the indigo
 * `<mark>` highlight used in the SuggestInput dropdown.
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
    class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg font-mono text-mono border transition-shadow"
    :class="highlight ? 'ring-2 ring-indigo-500 ring-offset-1' : ''"
    :style="{
      backgroundColor: scheme.bg,
      color: scheme.text,
      borderColor: scheme.border,
      ...highlightStyle,
    }"
  >
    {{ name }}
  </span>
</template>
