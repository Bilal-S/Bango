import { createApp } from 'vue';
import { createPinia } from 'pinia';
import router from './router';
import App from './App.vue';
import './styles/base.css';
import { useArticlesStore } from './stores/articles';
import { useCriteriaStore } from './stores/criteria';
import { useTagsStore } from './stores/tags';
import { useLabelsStore } from './stores/labels';
import { useLlmConfigStore } from './stores/llm-config';
import { useAuditStore } from './stores/audit';
import { useScreeningStore } from './stores/screening';
import { initialDataLoaded } from './composables/use-dashboard';
import { initFeatureFlags, useFeatureFlags } from './composables/use-feature-flags';
import { useToast } from './composables/use-toast';
import { getStartupStatus, performLegacyUpgrade } from './composables/use-startup-upgrade';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#app');

// Bootstrap entry point. Runs the silent legacy upgrade FIRST (if the backend
// detected an outdated schema), then pre-warms all Pinia stores so navigating
// to any view is instant.
async function bootstrap(): Promise<void> {
  // 1. Startup schema upgrade (silent). Must complete before any store reads
  //    from the DB, otherwise they would load from the pre-upgrade schema.
  const needsUpgrade = await getStartupStatus();
  if (needsUpgrade) {
    const toast = useToast();
    // Persistent "in progress" toast (duration=0) so it stays visible during
    // the upgrade; it is dismissed explicitly on completion.
    toast.show('Database upgrade in progress...', 'info', 0);
    try {
      const result = await performLegacyUpgrade();
      console.warn(
        `[startup_upgrade] completed: ${result.articleCount} articles restored; backup at ${result.backupPath}`
      );
      toast.show('Database upgrade completed.', 'success', 4000);
    } catch (e) {
      console.error('[startup_upgrade] failed:', e);
      toast.show(
        `Database upgrade failed: ${e instanceof Error ? e.message : String(e)}`,
        'error',
        0
      );
      // On failure, reload so the user lands in a clean state (the backend
      // keeps the backup file safe regardless).
      window.location.reload();
      return;
    }
    // Reload the webview so the app re-bootstraps against the freshly rebuilt
    // schema. This mirrors a "restart normally" without a process-level relaunch.
    window.location.reload();
    return;
  }

  // 2. Pre-warm all stores in parallel.
  // Once complete, signal the loading overlay to dismiss.
  void Promise.all([
    useArticlesStore().fetchIfNeeded(),
    useCriteriaStore().fetchIfNeeded(),
    useTagsStore().fetchIfNeeded(),
    useLabelsStore().fetchIfNeeded(),
    useLlmConfigStore().fetchIfNeeded(),
    useAuditStore().fetchIfNeeded(),
    useScreeningStore().fetchIfNeeded(),
    initFeatureFlags(),
  ]).finally(() => {
    initialDataLoaded.value = true;
    if (useFeatureFlags().isPremium.value) {
      useToast().show('Premium Mode Enabled.', 'info');
    }
  });
}

void bootstrap();
