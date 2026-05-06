<script setup lang="ts">
import { provide, ref, watch } from 'vue';
import { useViewport } from '@/composables/use-viewport';
import NavSidebar from './nav-sidebar.vue';

const { isBelowMd } = useViewport();

// Sidebar state: collapsed (icon-only) or expanded
const sidebarCollapsed = ref(false);
const sidebarMobileOpen = ref(false);

// Auto-collapse sidebar below md breakpoint
watch(
  isBelowMd,
  (below) => {
    if (below) {
      sidebarCollapsed.value = true;
      sidebarMobileOpen.value = false;
    } else {
      sidebarCollapsed.value = false;
      sidebarMobileOpen.value = false;
    }
  },
  { immediate: true }
);

function toggleSidebar(): void {
  if (isBelowMd.value) {
    sidebarMobileOpen.value = !sidebarMobileOpen.value;
  } else {
    sidebarCollapsed.value = !sidebarCollapsed.value;
  }
}

function closeMobileSidebar(): void {
  sidebarMobileOpen.value = false;
}

provide('sidebarCollapsed', sidebarCollapsed);
provide('toggleSidebar', toggleSidebar);
</script>

<template>
  <div class="app-shell">
    <!-- Mobile backdrop -->
    <div v-if="sidebarMobileOpen" class="app-shell__backdrop" @click="closeMobileSidebar" />

    <!-- Mobile hamburger button -->
    <button v-if="isBelowMd" class="app-shell__hamburger" @click="toggleSidebar">
      <span class="material-symbols-outlined">menu</span>
    </button>

    <NavSidebar
      :collapsed="sidebarCollapsed && !isBelowMd"
      :mobile-open="sidebarMobileOpen"
      @close-mobile="closeMobileSidebar"
    />
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
  position: relative;
}

.app-shell__main {
  flex: 1;
  overflow-y: auto;
  background-color: var(--color-surface);
  min-width: 0;
}

.app-shell__backdrop {
  position: fixed;
  inset: 0;
  background-color: rgba(0, 0, 0, 0.4);
  z-index: 40;
  transition: opacity 0.2s;
}

.app-shell__hamburger {
  position: fixed;
  top: var(--space-3);
  left: var(--space-3);
  z-index: 30;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-sidebar);
  color: var(--color-sidebar-text);
  border: none;
  border-radius: var(--radius-default);
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  transition: background-color 0.15s;
}

.app-shell__hamburger:hover {
  background-color: var(--color-sidebar-hover);
}

@media (min-width: 768px) {
  .app-shell__hamburger {
    display: none;
  }

  .app-shell__backdrop {
    display: none;
  }
}
</style>
