<script setup lang="ts">
import { computed, onMounted, nextTick } from 'vue';
import { useRoute } from 'vue-router';
import { useFeatureFlags } from '@/composables/use-feature-flags';
import SettingsProviderCard from '@/components/settings/settings-provider-card.vue';
import SettingsAiSummaries from '@/components/settings/settings-ai-summaries.vue';
import SettingsScreeningPreferences from '@/components/settings/settings-screening-preferences.vue';
import SettingsStorage from '@/components/settings/settings-storage.vue';
import SettingsReprocessing from '@/components/settings/settings-reprocessing.vue';
import SettingsProjectManagement from '@/components/settings/settings-project-management.vue';
import SettingsOpenAlex from '@/components/settings/settings-openalex.vue';
import SettingsNotificationHistory from '@/components/settings/settings-notification-history.vue';
import SettingsDiagnostics from '@/components/settings/settings-diagnostics.vue';

const appVersion = __APP_VERSION__;
const route = useRoute();
const { dbVersion, dbMaxVersion } = useFeatureFlags();
const showVersion = computed(() => dbMaxVersion.value > 0);

/**
 * On mount, if `?focus=project-management`, smooth-scroll the Project
 * Management card into view. Uses `nextTick` + `requestAnimationFrame`
 * so the DOM node exists before querying it.
 */
onMounted(() => {
  if (route.query.focus !== 'project-management') return;
  void nextTick(() => {
    requestAnimationFrame(() => {
      document
        .getElementById('settings-project-management')
        ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
  });
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

    <!-- Consolidated AI Provider box (warning + connection + params + actions + feedback) -->
    <SettingsProviderCard />

    <!-- Non-LLM settings cards -->
    <div class="settings-view__cards">
      <SettingsAiSummaries />
      <SettingsScreeningPreferences />
      <SettingsStorage />
      <SettingsReprocessing />
      <!-- `id` is the scroll target for the Dashboard's "Start New Project"
           dialog (`/settings?focus=project-management`). -->
      <div id="settings-project-management">
        <SettingsProjectManagement />
      </div>
      <SettingsOpenAlex />
      <SettingsNotificationHistory />
      <SettingsDiagnostics />
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  padding: var(--container-padding);
  max-width: 56rem;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .settings-view {
    padding: var(--container-padding-sm);
  }
}

.settings-view__header {
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

.settings-view__cards {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-top: 2rem;
}

/* Scroll target for the Dashboard's "Start New Project" deep-link
   (`?focus=project-management`). `scroll-margin-top` keeps the card header
   clear of any sticky app-shell chrome when scrolled into view. */
#settings-project-management {
  scroll-margin-top: 1rem;
}
</style>
