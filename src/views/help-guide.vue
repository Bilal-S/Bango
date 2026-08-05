<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import HelpTabGuide from '@/components/help/help-tab-guide.vue';
import HelpTabBibliometrics from '@/components/help/help-tab-bibliometrics.vue';
import HelpTabTroubleshooting from '@/components/help/help-tab-troubleshooting.vue';
import HelpTabLocalAi from '@/components/help/help-tab-local-ai.vue';
import HelpTabReference from '@/components/help/help-tab-reference.vue';
import HelpTabZotero from '@/components/help/help-tab-zotero.vue';

type HelpTab = 'guide' | 'biblio' | 'troubleshoot' | 'local-ai' | 'zotero' | 'reference';

const route = useRoute();
const activeTab = ref<HelpTab>('guide');

// Current route hash, passed to Reference tab for deep-link scroll.
const routeHash = ref<string>('');

function syncHash(): void {
  routeHash.value = route.hash ?? '';
}

/** Deep-link: `/help?tab=<id>` selects tab; `#hash` forwarded to active tab. */
onMounted(() => {
  const tab = route.query.tab as string | undefined;
  if (
    tab === 'troubleshoot' ||
    tab === 'local-ai' ||
    tab === 'guide' ||
    tab === 'biblio' ||
    tab === 'zotero' ||
    tab === 'reference'
  ) {
    activeTab.value = tab as HelpTab;
  }
  syncHash();
});

// Keep routeHash in sync when route changes (e.g. programmatic navigation).
watch(
  () => route.hash,
  () => syncHash()
);

// When switching tabs via deep-link query, also reflect hash.
watch(
  () => route.query.tab,
  (newTab) => {
    if (
      newTab === 'troubleshoot' ||
      newTab === 'local-ai' ||
      newTab === 'guide' ||
      newTab === 'biblio' ||
      newTab === 'zotero' ||
      newTab === 'reference'
    ) {
      activeTab.value = newTab as HelpTab;
    }
  }
);

// Reset scroll container to top when changing tabs (unless navigating to a hash/anchor)
watch(activeTab, () => {
  if (route.hash) return;
  requestAnimationFrame(() => {
    const scrollContainer = document.querySelector('.app-shell__content');
    if (scrollContainer) {
      scrollContainer.scrollTop = 0;
    }
  });
});

/**
 * Switch to a help tab from a child component (e.g. "Bibliometrics" link
 * inside Reference tab). Updates `activeTab` directly (no route navigation),
 * so existing watcher scrolls container to top. Idempotent across repeated clicks.
 */
function handleSwitchTab(tab: string): void {
  if (
    tab === 'guide' ||
    tab === 'biblio' ||
    tab === 'troubleshoot' ||
    tab === 'local-ai' ||
    tab === 'zotero' ||
    tab === 'reference'
  ) {
    activeTab.value = tab as HelpTab;
  }
}
</script>

<template>
  <div class="help-guide">
    <!-- Page Header -->
    <section class="help-guide__header">
      <h1 class="page-title">Help & Guides</h1>
      <p class="help-guide__subtitle">
        Everything you need to get the most out of Bango - from step-by-step workflows to
        troubleshooting and local AI setup.
      </p>
    </section>

    <!-- Tab Bar -->
    <nav class="help-tabs" role="tablist">
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'guide' }"
        role="tab"
        :aria-selected="activeTab === 'guide'"
        @click="activeTab = 'guide'"
      >
        <span class="material-symbols-outlined help-tabs__icon">menu_book</span>
        User Guide
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'biblio' }"
        role="tab"
        :aria-selected="activeTab === 'biblio'"
        @click="activeTab = 'biblio'"
      >
        <span class="material-symbols-outlined help-tabs__icon">hub</span>
        Bibliometrics
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'troubleshoot' }"
        role="tab"
        :aria-selected="activeTab === 'troubleshoot'"
        @click="activeTab = 'troubleshoot'"
      >
        <span class="material-symbols-outlined help-tabs__icon">build</span>
        Troubleshooting
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'local-ai' }"
        role="tab"
        :aria-selected="activeTab === 'local-ai'"
        @click="activeTab = 'local-ai'"
      >
        <span class="material-symbols-outlined help-tabs__icon">smart_toy</span>
        Local & Free AI
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'zotero' }"
        role="tab"
        :aria-selected="activeTab === 'zotero'"
        @click="activeTab = 'zotero'"
      >
        <span class="material-symbols-outlined help-tabs__icon">book_4</span>
        Using Zotero
      </button>
      <button
        class="help-tabs__btn"
        :class="{ 'help-tabs__btn--active': activeTab === 'reference' }"
        role="tab"
        :aria-selected="activeTab === 'reference'"
        @click="activeTab = 'reference'"
      >
        <span class="material-symbols-outlined help-tabs__icon">library_books</span>
        Reference
      </button>
    </nav>

    <!-- Active tab content -->
    <HelpTabGuide v-if="activeTab === 'guide'" :initial-hash="routeHash" />
    <HelpTabBibliometrics v-else-if="activeTab === 'biblio'" />
    <HelpTabTroubleshooting v-else-if="activeTab === 'troubleshoot'" />
    <HelpTabLocalAi v-else-if="activeTab === 'local-ai'" :initial-hash="routeHash" />
    <HelpTabZotero v-else-if="activeTab === 'zotero'" />
    <HelpTabReference
      v-else-if="activeTab === 'reference'"
      :initial-hash="routeHash"
      @switch-tab="handleSwitchTab"
    />
  </div>
</template>

<style scoped>
.help-guide {
  /* Unified width across all tabs. Previously the Reference tab used a wider
     1200px modifier (`.help-guide--wide`) while the other tabs used 860px,
     which made the Reference tab "stick out" and caused a visible width jump
     on tab switch. 1200px matches the previous Reference-tab width (the
     documented max for the sidebar + scroll-spy layout) and gives the other
     tabs' grids (e.g. the Guide tab's 3-column Starting Points) more breathing
     room. */
  padding: var(--container-padding);
  max-width: 1200px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .help-guide {
    padding: var(--container-padding-sm);
  }
}

/* Header */
.help-guide__header {
  margin-bottom: var(--space-4);
}

.help-guide__subtitle {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-top: var(--space-2);
  line-height: var(--line-height-body);
}

/* ===================== TAB BAR ===================== */
.help-tabs {
  display: flex;
  gap: 0;
  border-bottom: 2px solid var(--color-outline-variant, #e0e0e0);
  margin-bottom: var(--space-6);
  position: sticky;
  top: 0;
  background-color: var(--color-surface, #ffffff);
  z-index: 10;
}

.help-tabs__btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  margin-bottom: -2px;
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface-variant);
  cursor: pointer;
  transition:
    color 0.15s,
    border-color 0.15s,
    background-color 0.15s;
  font-family: inherit;
  white-space: nowrap;
}

.help-tabs__btn:hover {
  color: var(--color-on-surface);
  background-color: rgba(79, 70, 229, 0.04);
}

.help-tabs__btn--active {
  color: #4f46e5;
  border-bottom-color: #4f46e5;
}

.help-tabs__icon {
  font-size: 20px;
}

@media (max-width: 767px) {
  .help-tabs {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
  }

  .help-tabs__btn {
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-caption);
  }

  .help-tabs__icon {
    font-size: 18px;
  }
}
</style>
