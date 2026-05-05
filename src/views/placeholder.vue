<script setup lang="ts">
defineProps<{ title: string }>();
</script>

<template>
  <div class="placeholder">
    <h1>{{ title }}</h1>
    <p>Coming soon</p>
  </div>
</template>

<style scoped>
.placeholder {
  padding: var(--space-6);
  color: var(--color-on-surface-variant);
}

.placeholder h1 {
  color: var(--color-on-surface);
}
</style>
