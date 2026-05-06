<script setup lang="ts">
defineProps<{ title: string }>();
</script>

<template>
  <div class="placeholder">
    <h1 class="page-title">{{ title }}</h1>
    <p>Coming soon</p>
  </div>
</template>

<style scoped>
.placeholder {
  padding: var(--space-6);
  color: var(--color-on-surface-variant);
}
</style>
