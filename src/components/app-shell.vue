<script setup lang="ts">
import NavSidebar from './nav-sidebar.vue';
</script>

<template>
  <div class="app-shell">
    <NavSidebar />
    <main class="app-shell__main">
      <router-view />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.app-shell__main {
  flex: 1;
  overflow-y: auto;
  background-color: var(--color-surface);
}
</style>
