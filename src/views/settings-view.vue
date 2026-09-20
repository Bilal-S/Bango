<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, nextTick, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useFeatureFlags } from '@/composables/use-feature-flags';
import SettingsAiSection from '@/components/settings/settings-ai-section.vue';
import SettingsEmbeddings from '@/components/settings/settings-embeddings.vue';
import SettingsAiSummaries from '@/components/settings/settings-ai-summaries.vue';
import SettingsScreeningPreferences from '@/components/settings/settings-screening-preferences.vue';
import SettingsStorage from '@/components/settings/settings-storage.vue';
import SettingsReprocessing from '@/components/settings/settings-reprocessing.vue';
import SettingsProjectManagement from '@/components/settings/settings-project-management.vue';
import SettingsOpenAlex from '@/components/settings/settings-openalex.vue';
import SettingsNotificationHistory from '@/components/settings/settings-notification-history.vue';
import SettingsDiagnostics from '@/components/settings/settings-diagnostics.vue';
import {
  SETTINGS_SECTIONS,
  SETTINGS_SECTION_TRIGGER_PX,
  activeSectionFromTops,
  resolveSettingsSectionId,
  type SettingsSection,
} from '@/utils/settings-sections';

const appVersion = __APP_VERSION__;
const route = useRoute();
const router = useRouter();
const { dbVersion, dbMaxVersion } = useFeatureFlags();
const showVersion = computed(() => dbMaxVersion.value > 0);

/** Longer than a smooth scroll: the spy must not fight the click animation. */
const MANUAL_SCROLL_GUARD_MS = 1000;
/** How long a deep link keeps re-aligning while async card content settles. */
const DEEP_LINK_SETTLE_MS = 2000;

/** Active rail entry (scroll-spy; clicks and deep links set it immediately). */
const activeSection = ref(SETTINGS_SECTIONS[0]?.id ?? '');
let manualScrollUntil = 0;
/** The single scrolling element (`.app-shell__content`); window fallback. */
let scrollElement: HTMLElement | null = null;
let scrollTarget: EventTarget | null = null;

/** Deep-link re-alignment state (see `trackDeepLinkSettle`). */
let deepLinkObserver: ResizeObserver | null = null;
let deepLinkStopTimer: number | null = null;

/** Smooth-scroll a card to the top of the scroll container and mark it active. */
function scrollToSection(id: string): void {
  const element = document.getElementById(id);
  if (!element) return;
  activeSection.value = id;
  manualScrollUntil = performance.now() + MANUAL_SCROLL_GUARD_MS;
  element.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

/** Stop following the deep-link target (settled, user took over, or unmount). */
function stopDeepLinkTracking(): void {
  deepLinkObserver?.disconnect();
  deepLinkObserver = null;
  if (deepLinkStopTimer !== null) {
    window.clearTimeout(deepLinkStopTimer);
    deepLinkStopTimer = null;
  }
  window.removeEventListener('wheel', stopDeepLinkTracking);
  window.removeEventListener('touchstart', stopDeepLinkTracking);
}

/**
 * Card heights above the target change as the AI/Embeddings status loads, so
 * keep re-aligning the deep-linked card until the column settles, the user
 * takes over (wheel/touch), or the 2s cap expires.
 */
function trackDeepLinkSettle(id: string): void {
  stopDeepLinkTracking();
  const cards = document.querySelector<HTMLElement>('.settings-view__cards');
  if (cards && typeof ResizeObserver !== 'undefined') {
    deepLinkObserver = new ResizeObserver(() => {
      document.getElementById(id)?.scrollIntoView({ behavior: 'auto', block: 'start' });
    });
    deepLinkObserver.observe(cards);
  }
  window.addEventListener('wheel', stopDeepLinkTracking, { passive: true, once: true });
  window.addEventListener('touchstart', stopDeepLinkTracking, { passive: true, once: true });
  deepLinkStopTimer = window.setTimeout(stopDeepLinkTracking, DEEP_LINK_SETTLE_MS);
}

/** Rail click: jump, then persist the target as a shareable `?focus=` value. */
function onSelectSection(section: SettingsSection): void {
  scrollToSection(section.id);
  void router.replace({ query: { ...route.query, focus: section.id } });
}

/** Scroll-spy: the last card whose top crossed the trigger line wins. */
function updateActiveSection(): void {
  if (performance.now() < manualScrollUntil) return;
  const containerTop = scrollElement?.getBoundingClientRect().top ?? 0;
  const tops = SETTINGS_SECTIONS.map((section) => {
    const element = document.getElementById(section.id);
    return element ? element.getBoundingClientRect().top - containerTop : Number.POSITIVE_INFINITY;
  });
  activeSection.value = activeSectionFromTops(tops, SETTINGS_SECTION_TRIGGER_PX);
}

onMounted(() => {
  scrollElement = document.querySelector<HTMLElement>('.app-shell__content');
  scrollTarget = scrollElement ?? window;
  scrollTarget.addEventListener('scroll', updateActiveSection, { passive: true });

  // `?focus=<card-id>` deep link (legacy `project-management` included).
  const focusId = resolveSettingsSectionId(route.query.focus);
  if (!focusId) return;
  // The card nodes exist after paint; same timing as the Dashboard deep link.
  void nextTick(() => {
    requestAnimationFrame(() => {
      scrollToSection(focusId);
      trackDeepLinkSettle(focusId);
    });
  });
});

onBeforeUnmount(() => {
  scrollTarget?.removeEventListener('scroll', updateActiveSection);
  stopDeepLinkTracking();
});
</script>

<template>
  <div class="settings-view">
    <!-- Header -->
    <div class="settings-view__header">
      <h1 class="page-title">
        Settings
        <span v-if="showVersion" class="settings-view__version">
          (v{{ appVersion }} / {{ dbVersion }}-{{ dbMaxVersion }})
        </span>
      </h1>
      <p class="settings-view__subtitle">
        Configure AI provider parameters (note: AI models can make mistakes!), set preferences,
        manage backups.
      </p>
    </div>

    <div class="settings-view__body">
      <!-- Jump nav: sticky right rail on desktop, sticky chip row on mobile. -->
      <nav class="settings-view__nav" aria-label="Settings sections">
        <button
          v-for="section in SETTINGS_SECTIONS"
          :key="section.id"
          type="button"
          class="settings-view__nav-link"
          :class="{ 'is-active': activeSection === section.id }"
          :aria-current="activeSection === section.id ? 'true' : undefined"
          @click="onSelectSection(section)"
        >
          {{ section.label }}
        </button>
      </nav>

      <!-- All settings cards share one 1rem vertical rhythm (LLM + app cards).
           Card ids come from the section registry above via attribute
           fallthrough; the rail and `?focus=` resolve the same ids. -->
      <div class="settings-view__cards">
        <SettingsAiSection id="settings-ai" />
        <SettingsEmbeddings id="settings-embeddings" />
        <SettingsProjectManagement id="settings-project-management" />
        <SettingsAiSummaries id="settings-summaries" />
        <SettingsScreeningPreferences id="settings-screening" />
        <SettingsOpenAlex id="settings-search" />
        <SettingsStorage id="settings-storage" />
        <SettingsReprocessing id="settings-batch" />
        <SettingsNotificationHistory id="settings-history" />
        <SettingsDiagnostics id="settings-diagnostics" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  padding: var(--container-padding);
  max-width: 72rem;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .settings-view {
    padding: var(--container-padding-sm);
  }
}

.settings-view__header {
  max-width: 56rem;
  margin-bottom: 1.5rem;
}

.settings-view__subtitle {
  font-size: 14px;
  line-height: 20px;
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.5rem;
}

.settings-view__version {
  font-size: 14px;
  font-weight: 400;
  color: var(--color-on-surface-variant);
  white-space: nowrap;
}

.settings-view__body {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 1rem;
}

/* Mobile: sticky horizontally scrollable chip row above the cards. */
.settings-view__nav {
  position: sticky;
  top: 0;
  z-index: 5;
  display: flex;
  gap: 0.25rem;
  overflow-x: auto;
  padding: 0.25rem 0;
  background-color: var(--color-background, #fcf8ff);
}

.settings-view__nav-link {
  flex: 0 0 auto;
  padding: 0.25rem 0.75rem;
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 9999px;
  background-color: var(--color-surface-container-lowest, #ffffff);
  color: var(--color-on-surface-variant, #464555);
  font-family: inherit;
  font-size: 12.5px;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
}

.settings-view__nav-link.is-active {
  border-color: var(--color-primary, #3525cd);
  color: var(--color-primary, #3525cd);
}

.settings-view__cards {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  max-width: 56rem;
}

/* Jumped-to cards clear the sticky chip row (mobile) / top edge (desktop). */
.settings-view__body [id^='settings-'] {
  scroll-margin-top: 4rem;
}

/* Desktop: sticky right rail, card column unchanged at 56rem. */
@media (min-width: 1024px) {
  .settings-view__body {
    grid-template-columns: minmax(0, 1fr) 11rem;
    gap: 2rem;
  }

  .settings-view__cards {
    grid-column: 1;
    grid-row: 1;
  }

  .settings-view__nav {
    grid-column: 2;
    grid-row: 1;
    align-self: start;
    position: sticky;
    top: 1rem;
    flex-direction: column;
    gap: 0.125rem;
    overflow: visible;
    padding: 0;
    background-color: transparent;
  }

  .settings-view__nav-link {
    padding: 0.25rem 0.5rem;
    border: none;
    border-radius: 0.375rem;
    background-color: transparent;
    text-align: left;
  }

  .settings-view__nav-link:hover {
    background-color: rgba(228, 225, 238, 0.5);
  }

  .settings-view__nav-link.is-active {
    background-color: var(--color-primary-fixed, #e2dfff);
  }

  .settings-view__body [id^='settings-'] {
    scroll-margin-top: 1rem;
  }
}
</style>
