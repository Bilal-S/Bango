<script setup lang="ts">
import SettingsProviderCard from '@/components/settings/settings-provider-card.vue';
import SettingsAiSummaries from '@/components/settings/settings-ai-summaries.vue';
import SettingsScreeningPreferences from '@/components/settings/settings-screening-preferences.vue';
import SettingsStorage from '@/components/settings/settings-storage.vue';
import SettingsReprocessing from '@/components/settings/settings-reprocessing.vue';
import SettingsProjectManagement from '@/components/settings/settings-project-management.vue';
import SettingsNotificationHistory from '@/components/settings/settings-notification-history.vue';
import SettingsDiagnostics from '@/components/settings/settings-diagnostics.vue';
</script>

<template>
  <div class="settings-view">
    <!-- Header -->
    <div class="settings-view__header">
      <h1 class="page-title">Settings</h1>
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
      <SettingsProjectManagement />
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

.settings-view__cards {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-top: 2rem;
}
</style>
