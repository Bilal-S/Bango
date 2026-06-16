<script setup lang="ts">
import SettingsProviderCard from '@/components/settings/settings-provider-card.vue';
import SettingsProjectManagement from '@/components/settings/settings-project-management.vue';
import SettingsScreeningPreferences from '@/components/settings/settings-screening-preferences.vue';
import SettingsFullTextStorage from '@/components/settings/settings-full-text-storage.vue';
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
      <SettingsProjectManagement />
      <SettingsScreeningPreferences />
      <SettingsFullTextStorage />
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
