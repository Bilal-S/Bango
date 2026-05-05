<script setup lang="ts">
import { useRoute } from 'vue-router';

const route = useRoute();

interface NavItem {
  label: string;
  icon: string;
  route: string;
}

const navItems: NavItem[] = [
  { label: 'Dashboard', icon: 'dashboard', route: '/' },
  { label: 'Articles', icon: 'description', route: '/articles' },
  { label: 'Import RIS', icon: 'upload_file', route: '/import' },
  { label: 'Deduplicate', icon: 'science', route: '/dedup' },
  { label: 'Criteria', icon: 'rule', route: '/criteria' },
  { label: 'Screening', icon: 'analytics', route: '/screening' },
  { label: 'Tags & Labels', icon: 'sell', route: '/tags' },
  { label: 'PRISMA', icon: 'account_tree', route: '/prisma' },
  { label: 'Settings', icon: 'settings', route: '/settings' },
];
</script>

<template>
  <nav class="sidebar">
    <div class="sidebar__header">
      <span class="sidebar__logo">B</span>
      <span class="sidebar__title">Bango</span>
    </div>
    <ul class="sidebar__nav">
      <li v-for="item in navItems" :key="item.route">
        <router-link
          :to="item.route"
          class="sidebar__link"
          :class="{ 'sidebar__link--active': route.path === item.route }"
        >
          <span class="material-symbols-outlined sidebar__icon">{{ item.icon }}</span>
          <span class="sidebar__label">{{ item.label }}</span>
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
}

.sidebar__header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-4);
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
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
}

.sidebar__title {
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-h2);
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
}

.sidebar__link:hover {
  background-color: var(--color-sidebar-hover);
}

.sidebar__link--active {
  background-color: var(--color-sidebar-hover);
  color: #ffffff;
}
</style>
