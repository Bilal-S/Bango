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

    <NavSidebar
      :collapsed="sidebarCollapsed && !isBelowMd"
      :mobile-open="sidebarMobileOpen"
      @close-mobile="closeMobileSidebar"
    />
    <main class="app-shell__main">
      <header v-if="isBelowMd" class="app-shell__mobile-bar">
        <button class="app-shell__hamburger" @click="toggleSidebar">
          <span class="material-symbols-outlined">menu</span>
        </button>
        <span class="app-shell__mobile-title">Bango</span>
      </header>
      <div class="app-shell__content">
        <router-view />
      </div>
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
  display: flex;
  flex-direction: column;
  overflow: hidden;
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

.app-shell__mobile-bar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  height: 48px;
  padding: 0 var(--space-3);
  background-color: var(--color-surface-container-lowest, #ffffff);
  border-bottom: 1px solid var(--color-outline-variant, #c7c4d8);
  position: sticky;
  top: 0;
  z-index: 30;
  flex-shrink: 0;
}

.app-shell__mobile-title {
  font-size: var(--font-size-h1, 20px);
  font-weight: var(--font-weight-semibold, 600);
  color: var(--color-on-surface, #1b1b24);
}

.app-shell__hamburger {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: transparent;
  color: var(--color-on-surface, #1b1b24);
  border: none;
  border-radius: var(--radius-default);
  cursor: pointer;
  transition: background-color 0.15s;
}

.app-shell__hamburger:hover {
  background-color: var(--color-surface-container, #f0ecf9);
}

.app-shell__content {
  flex: 1;
  overflow-y: auto;
}
</style>
