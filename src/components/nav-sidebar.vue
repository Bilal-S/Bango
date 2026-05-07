<script setup lang="ts">
import { useRoute } from 'vue-router';

const props = defineProps<{
  collapsed?: boolean;
  mobileOpen?: boolean;
}>();

const emit = defineEmits<{
  closeMobile: [];
}>();

const route = useRoute();
const appVersion = __APP_VERSION__;

interface NavItem {
  label: string;
  icon: string;
  route: string;
}

const navItems: NavItem[] = [
  { label: 'Dashboard', icon: 'dashboard', route: '/' },
  { label: 'Criteria', icon: 'rule', route: '/criteria' },
  { label: 'Import RIS', icon: 'upload_file', route: '/import' },
  { label: 'Tags & Labels', icon: 'sell', route: '/tags' },
  { label: 'Deduplicate', icon: 'science', route: '/dedup' },
  { label: 'Screening', icon: 'analytics', route: '/screening' },
  { label: 'Articles', icon: 'description', route: '/articles' },
  { label: 'PRISMA', icon: 'account_tree', route: '/prisma' },
  { label: 'Settings', icon: 'settings', route: '/settings' },
];

function handleNavClick(): void {
  if (props.mobileOpen) {
    emit('closeMobile');
  }
}
</script>

<template>
  <nav
    class="sidebar"
    :class="{
      'sidebar--collapsed': collapsed,
      'sidebar--mobile-open': mobileOpen,
    }"
  >
    <div class="sidebar__header">
      <span class="sidebar__logo">B</span>
      <span v-if="!collapsed" class="sidebar__title"
        >Bango <span class="sidebar__version">v{{ appVersion }}</span></span
      >
    </div>
    <ul class="sidebar__nav">
      <li v-for="item in navItems" :key="item.route">
        <router-link
          :to="item.route"
          class="sidebar__link"
          :class="{
            'sidebar__link--active': route.path === item.route,
            'sidebar__link--collapsed': collapsed,
          }"
          :title="collapsed ? item.label : undefined"
          @click="handleNavClick"
        >
          <span class="material-symbols-outlined sidebar__icon">{{ item.icon }}</span>
          <span v-if="!collapsed" class="sidebar__label">{{ item.label }}</span>
        </router-link>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  height: 100vh;
  background-color: var(--color-sidebar);
  color: var(--color-sidebar-text);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  overflow-y: auto;
  overflow-x: hidden;
  transition: width 0.2s ease;
}

/* Collapsed state (md breakpoint - icon-only sidebar) */
.sidebar--collapsed {
  width: var(--sidebar-collapsed-width);
}

/* Mobile: hidden by default, shown as overlay when open */
@media (max-width: 767px) {
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 50;
    transform: translateX(-100%);
    transition: transform 0.25s ease;
    box-shadow: none;
  }

  .sidebar--mobile-open {
    transform: translateX(0);
    box-shadow: 4px 0 24px rgba(0, 0, 0, 0.2);
  }
}

.sidebar__header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-4);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  overflow: hidden;
  white-space: nowrap;
}

.sidebar--collapsed .sidebar__header {
  justify-content: center;
  padding: var(--space-6) var(--space-2);
}

.sidebar__logo {
  width: 32px;
  height: 32px;
  background-color: var(--color-primary);
  border-radius: var(--radius-default);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-h1);
  color: var(--color-on-primary);
  flex-shrink: 0;
}

.sidebar__title {
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-h2);
}

.sidebar__version {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-regular);
  color: rgba(255, 255, 255, 0.5);
}

.sidebar__nav {
  list-style: none;
  padding: var(--space-2) 0;
}

.sidebar__link {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  color: var(--color-sidebar-text);
  text-decoration: none;
  font-size: var(--font-size-caption);
  transition: background-color 0.15s;
  overflow: hidden;
  white-space: nowrap;
}

.sidebar__link:hover {
  background-color: rgba(255, 255, 255, 0.08);
}

.sidebar__link--active {
  background-color: rgba(255, 255, 255, 0.15);
  color: #ffffff;
  font-weight: var(--font-weight-semibold);
  border-left: 3px solid var(--color-primary);
  padding-left: calc(var(--space-4) - 3px);
}

.sidebar__link--active .sidebar__icon {
  font-variation-settings:
    'FILL' 1,
    'wght' 400,
    'GRAD' 0,
    'opsz' 24;
}

/* Collapsed active: use left border as padding offset doesn't apply */
.sidebar__link--collapsed.sidebar__link--active {
  padding-left: var(--space-2);
  border-left: none;
  border-radius: var(--radius-default);
  background-color: rgba(255, 255, 255, 0.18);
}

/* Collapsed link: center the icon */
.sidebar__link--collapsed {
  justify-content: center;
  padding: var(--space-2);
}

.sidebar__icon {
  font-size: 20px;
  flex-shrink: 0;
}

.sidebar__label {
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
