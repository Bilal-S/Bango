<script setup lang="ts">
import { computed } from 'vue';
import { getColorScheme } from '@/utils/color';

/* Shared chip scaffold for `tag-chip.vue` (filled) and `label-chip.vue`
 * (dot). Owns the color scheme, the optional highlight halo, and the
 * optional `(N)` count suffix; wrappers only pick a variant and forward
 * props. */
const props = defineProps<{
  name: string;
  color?: string | null;
  /** Optional article count rendered as a muted `(N)` suffix inside the chip. */
  count?: number;
  /** When true, renders a strong indigo halo (ring + glow) around the chip. */
  highlight?: boolean;
  /** `filled` = solid scheme background; `dot` = transparent with leading color dot. */
  variant: 'filled' | 'dot';
}>();

const scheme = computed(() => getColorScheme(props.name, props.color));
const filled = computed(() => props.variant === 'filled');

/** Indigo ring + glow applied when `highlight` is true; reads as a deliberate
 * "match" signal, consistent with the indigo `<mark>` in the SuggestInput
 * dropdown. */
const HIGHLIGHT_GLOW = {
  boxShadow: '0 0 0 2px rgba(99, 102, 241, 0.35), 0 0 8px 2px rgba(99, 102, 241, 0.25)',
};

/** Base utility classes (byte-identical per variant to the pre-extraction chips). */
const classes = computed(() =>
  filled.value
    ? 'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg font-mono text-mono border transition-shadow'
    : 'inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border bg-transparent font-mono text-mono transition-shadow'
);

/** Filled paints the scheme background; dot leaves it transparent (the dot carries the base color). */
const style = computed(() =>
  filled.value
    ? {
        backgroundColor: scheme.value.bg,
        color: scheme.value.text,
        borderColor: scheme.value.border,
        ...(props.highlight ? HIGHLIGHT_GLOW : {}),
      }
    : {
        borderColor: scheme.value.border,
        color: scheme.value.text,
        ...(props.highlight ? HIGHLIGHT_GLOW : {}),
      }
);
</script>

<template>
  <span :class="[classes, highlight ? 'ring-2 ring-indigo-500 ring-offset-1' : '']" :style="style">
    <span
      v-if="!filled"
      class="w-1.5 h-1.5 rounded-full"
      :style="{ backgroundColor: scheme.base }"
    ></span>
    {{ name
    }}<span v-if="count !== undefined" class="opacity-70" :class="{ 'font-bold': count > 0 }">
      ({{ count }})</span
    >
  </span>
</template>
