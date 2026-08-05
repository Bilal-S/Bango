<script setup lang="ts">
import { provide, ref, watch } from 'vue';
import { useViewport } from '@/composables/use-viewport';
import { initialDataLoaded } from '@/composables/use-dashboard';
import { useLoadingOverlay } from '@/composables/use-loading-overlay';
import NavSidebar from './nav-sidebar.vue';
import ToastContainer from './toast-container.vue';

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

// Initial loading overlay - data is bootstrapped in main.ts;
// this overlay shows a transparent spinner until that completes.
const showOverlay = ref(true);
const fadingOut = ref(false);

function dismissOverlay(): void {
  fadingOut.value = true;
  setTimeout(() => {
    showOverlay.value = false;
  }, 300);
}

// Dismiss overlay once main.ts signals that initial data is loaded
watch(
  initialDataLoaded,
  (loaded) => {
    if (loaded && showOverlay.value) {
      dismissOverlay();
    }
  },
  { immediate: true }
);

// Global loading overlay for long-running operations (project import, demo load)
const { isVisible: isOperationOverlayVisible, message: operationMessage } = useLoadingOverlay();
</script>

<template>
  <div class="app-shell">
    <!-- Initial Loading Overlay -->
    <Transition name="loading-fade">
      <div
        v-if="showOverlay"
        class="loading-overlay"
        :class="{ 'loading-overlay--fading': fadingOut }"
      >
        <div class="loading-content">
          <div class="loading-spinner"></div>
          <p class="loading-text">Loading Project Data</p>
        </div>
      </div>
    </Transition>

    <!-- Mobile backdrop -->
    <div v-if="sidebarMobileOpen" class="app-shell__backdrop" @click="closeMobileSidebar" />

    <NavSidebar
      :collapsed="sidebarCollapsed && !isBelowMd"
      :mobile-open="sidebarMobileOpen"
      @close-mobile="closeMobileSidebar"
      @toggle-collapse="toggleSidebar"
    />
    <main class="app-shell__main">
      <header v-if="isBelowMd" class="app-shell__mobile-bar">
        <button class="app-shell__hamburger" @click="toggleSidebar">
          <span class="material-symbols-outlined">menu</span>
        </button>
        <span class="app-shell__mobile-title">Bango</span>
      </header>
      <div class="app-shell__content">
        <!-- Keep-alive caches WikiView and ArticleList so UI state (filters,
             search, detail panel, fullscreen) survives navigation. Both views
             re-fetch underlying data in `onActivated`. -->
        <router-view v-slot="{ Component }">
          <keep-alive :include="['WikiView', 'ArticleList']">
            <component :is="Component" />
          </keep-alive>
        </router-view>
      </div>
    </main>
    <!-- Operation Loading Overlay (project import, demo load, etc.) -->
    <Transition name="loading-fade">
      <div v-if="isOperationOverlayVisible" class="loading-overlay">
        <div class="loading-content">
          <div class="loading-spinner"></div>
          <p class="loading-text">{{ operationMessage }}</p>
        </div>
      </div>
    </Transition>

    <ToastContainer />
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

/* Loading Overlay - semi-transparent so app shell is faintly visible */
.loading-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: rgba(252, 248, 255, 0.85);
  backdrop-filter: blur(4px);
  transition: opacity 0.3s ease;
}

.loading-overlay--fading {
  opacity: 0;
}

.loading-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1.25rem;
}

.loading-text {
  font-size: 1.125rem;
  font-weight: 500;
  color: var(--color-on-surface-variant);
  margin: 0;
}

.loading-spinner {
  width: 2.5rem;
  height: 2.5rem;
  border: 3px solid var(--color-outline-variant);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* Transition */
.loading-fade-leave-active {
  transition: opacity 0.3s ease;
}

.loading-fade-leave-to {
  opacity: 0;
}
</style>
