<script setup lang="ts">
import { ref, watch, onBeforeUnmount } from 'vue';
import { useRoute } from 'vue-router';
import appIcon from '@/assets/app-icon.png';
import { initialDataLoaded } from '@/composables/use-dashboard';
import { useArticlesStore } from '@/stores/articles';
import ShareDialog from './share-dialog.vue';

const props = defineProps<{
  collapsed?: boolean;
  mobileOpen?: boolean;
}>();

const emit = defineEmits<{
  closeMobile: [];
  toggleCollapse: [];
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
  { label: 'Import', icon: 'upload_file', route: '/import' },
  { label: 'Tags & Labels', icon: 'sell', route: '/tags' },
  { label: 'Deduplicate', icon: 'science', route: '/dedup' },
  { label: 'Screening', icon: 'analytics', route: '/screening' },
  { label: 'Articles', icon: 'description', route: '/articles' },
  { label: 'PRISMA', icon: 'account_tree', route: '/prisma' },
  { label: 'AI Summary', icon: 'auto_awesome', route: '/summary' },
  { label: 'Bibliometrics', icon: 'graph_3', route: '/bibliometrics' },
  { label: 'Chat / Citations', icon: 'chat_add_on', route: '/chat' },
  { label: 'Wiki', icon: 'local_library', route: '/wiki' },
  { label: 'Settings', icon: 'settings', route: '/settings' },
];

const helpItem: NavItem = { label: 'Help Guide', icon: 'help', route: '/help' };

const showShareDialog = ref(false);

// --- Share Bango startup attention animation ---
// Two-stage sequence, both fire at most once per app session:
//   Stage 1 (6s after data-ready): zoom pulse (Option B)
//   Stage 2 (2 minutes after data-ready): soft glow pulse (Option C)
// Both no-op when prefers-reduced-motion: reduce is set.
// Stage 1 is delayed so it does not compete with other startup activity
// (loading overlay dismiss, store pre-warming, wiki drift check, etc.).
const STAGE_1_DELAY_MS = 3 * 1000; // 6 seconds
const ZOOM_PULSE_MS = 700;
const GLOW_PULSE_MS = 1400;
const STAGE_2_DELAY_MS = 2 * 60 * 1000; // 2 minutes

const shareZoom = ref(false);
const shareGlow = ref(false);

let stage1DelayTimer: ReturnType<typeof setTimeout> | null = null;
let stage1Timer: ReturnType<typeof setTimeout> | null = null;
let stage2Timer: ReturnType<typeof setTimeout> | null = null;

/** True once across the whole app session so the attention animation never
 *  repeats on remount / navigation. Module-scoped, not component-scoped. */
let attentionPlayedThisSession = false;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

function runStage1(): void {
  shareZoom.value = true;
  stage1Timer = setTimeout(() => {
    shareZoom.value = false;
    stage1Timer = null;
  }, ZOOM_PULSE_MS);
}

function runStage2(): void {
  shareGlow.value = true;
  stage2Timer = setTimeout(() => {
    shareGlow.value = false;
    stage2Timer = null;
  }, GLOW_PULSE_MS);
}

watch(
  initialDataLoaded,
  (loaded) => {
    if (!loaded || attentionPlayedThisSession) return;
    // Only draw attention when there is real data (not a fresh database).
    if (useArticlesStore().articles.length === 0) return;
    attentionPlayedThisSession = true;

    if (prefersReducedMotion()) return;

    // Stage 1: zoom pulse after a short delay so it does not compete with
    // other startup activity (loading overlay dismiss, store pre-warming,
    // wiki drift check, etc.).
    stage1DelayTimer = setTimeout(() => {
      stage1DelayTimer = null;
      runStage1();
    }, STAGE_1_DELAY_MS);
    // Stage 2: soft glow pulse 2 minutes later.
    stage2Timer = setTimeout(runStage2, STAGE_2_DELAY_MS);
  },
  { immediate: true }
);

onBeforeUnmount(() => {
  if (stage1DelayTimer !== null) {
    clearTimeout(stage1DelayTimer);
    stage1DelayTimer = null;
  }
  if (stage1Timer !== null) {
    clearTimeout(stage1Timer);
    stage1Timer = null;
  }
  if (stage2Timer !== null) {
    clearTimeout(stage2Timer);
    stage2Timer = null;
  }
});

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
    <div class="sidebar__header" @click="emit('toggleCollapse')">
      <img class="sidebar__logo" :src="appIcon" alt="Bango" />
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
    <div class="sidebar__divider" />
    <button
      class="sidebar__link sidebar__share-btn"
      :class="{
        'sidebar__link--collapsed': collapsed,
        'sidebar__share-btn--zoom': shareZoom,
        'sidebar__share-btn--glow': shareGlow,
      }"
      :title="collapsed ? 'Share Bango' : undefined"
      @click="showShareDialog = true"
    >
      <span class="material-symbols-outlined sidebar__icon">share</span>
      <span v-if="!collapsed" class="sidebar__label">Share Bango</span>
    </button>
    <ShareDialog v-if="showShareDialog" @close="showShareDialog = false" />
    <div class="sidebar__divider" />
    <div class="sidebar__footer">
      <router-link
        :to="helpItem.route"
        class="sidebar__link"
        :class="{
          'sidebar__link--active': route.path === helpItem.route,
          'sidebar__link--collapsed': collapsed,
        }"
        :title="collapsed ? helpItem.label : undefined"
        @click="handleNavClick"
      >
        <span class="material-symbols-outlined sidebar__icon">{{ helpItem.icon }}</span>
        <span v-if="!collapsed" class="sidebar__label">{{ helpItem.label }}</span>
      </router-link>
      <div class="sidebar__divider sidebar__divider--footer" />
      <button
        class="sidebar__link sidebar__collapse-btn"
        :class="{ 'sidebar__link--collapsed': collapsed }"
        :title="collapsed ? 'Expand sidebar' : 'Collapse sidebar'"
        @click="emit('toggleCollapse')"
      >
        <span class="material-symbols-outlined sidebar__icon">
          {{ collapsed ? 'chevron_right' : 'chevron_left' }}
        </span>
        <span v-if="!collapsed" class="sidebar__label">Collapse</span>
      </button>
    </div>
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
  cursor: pointer;
  transition: background-color 0.15s;
}

.sidebar__header:hover {
  background-color: rgba(255, 255, 255, 0.08);
}

.sidebar--collapsed .sidebar__header {
  justify-content: center;
  padding: var(--space-6) var(--space-2);
}

.sidebar__logo {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-default);
  object-fit: contain;
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

.sidebar__divider {
  margin: 0 var(--space-4);
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.sidebar--collapsed .sidebar__divider {
  margin: 0 var(--space-2);
}

.sidebar__footer {
  margin-top: auto;
  padding: var(--space-2) 0 var(--space-4);
}

.sidebar__divider--footer {
  margin-top: var(--space-2);
  margin-bottom: var(--space-2);
}

.sidebar__collapse-btn {
  width: 100%;
  background: none;
  border: none;
  cursor: pointer;
  font-family: inherit;
}

.sidebar__collapse-btn:hover {
  background-color: rgba(255, 255, 255, 0.08);
}

/* Share Bango uses the shared .sidebar__link styles for an icon + label row
 * identical to Settings / Help Guide. The only addition is symmetric vertical
 * spacing so the button sits centered in its own section between the two
 * dividers (after Settings and before Help Guide). */
.sidebar__share-btn {
  width: 100%;
  margin-block: var(--space-2);
  background: none;
  border: none;
  cursor: pointer;
  font-family: inherit;
  /* Keep the button above its neighbors so the scale + glow are not clipped
   * by adjacent sidebar rows during the attention animation. */
  position: relative;
  transform-origin: center;
}

/* Stage 1: zoom pulse - scale up then settle, with a primary-color glow that
 * fades in during scale-up and out during settle. The transform-origin is
 * center so the pulse is symmetric in both collapsed (icon) and expanded
 * (icon + label) layouts. */
.sidebar__share-btn--zoom {
  animation: share-zoom-pulse 0.7s cubic-bezier(0.34, 1.56, 0.64, 1);
  z-index: 2;
}

@keyframes share-zoom-pulse {
  0% {
    transform: scale(1);
    box-shadow: 0 0 0 0 rgba(99, 102, 241, 0);
  }
  40% {
    transform: scale(1.12);
    box-shadow: 0 0 18px 4px rgba(99, 102, 241, 0.65);
  }
  100% {
    transform: scale(1);
    box-shadow: 0 0 0 0 rgba(99, 102, 241, 0);
  }
}

/* Stage 2: soft glow pulse - no scale, just two gentle primary-color glows
 * that breathe to re-attract attention without motion. */
.sidebar__share-btn--glow {
  animation: share-glow-pulse 1.4s ease-in-out;
  z-index: 2;
}

@keyframes share-glow-pulse {
  0%,
  100% {
    box-shadow: 0 0 0 0 rgba(99, 102, 241, 0);
  }
  25% {
    box-shadow: 0 0 14px 3px rgba(99, 102, 241, 0.5);
  }
  50% {
    box-shadow: 0 0 0 0 rgba(99, 102, 241, 0);
  }
  75% {
    box-shadow: 0 0 14px 3px rgba(99, 102, 241, 0.5);
  }
}

/* Respect users who prefer reduced motion: the script short-circuits before
 * adding the class, but this is a defensive guard in case the class is ever
 * applied through another path. */
@media (prefers-reduced-motion: reduce) {
  .sidebar__share-btn--zoom,
  .sidebar__share-btn--glow {
    animation: none;
  }
}
</style>
