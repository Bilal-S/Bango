<script setup lang="ts">
import ChipBase from './chip-base.vue';

/* Filled tag chip (solid scheme background). Thin wrapper over the shared
 * chip scaffold; see `chip-base.vue` for the full prop contract. */
defineProps<{
  name: string;
  color?: string | null;
  /** Optional article count rendered as a muted `(N)` suffix. */
  count?: number;
  /** When true, renders an indigo match halo. */
  highlight?: boolean;
}>();
</script>

<template>
  <ChipBase :name="name" :color="color" :count="count" :highlight="highlight" variant="filled" />
</template>
