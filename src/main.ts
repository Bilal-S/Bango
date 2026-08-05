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
import {
  decideUpgrade,
  getStartupStatus,
  getUpgradeAttempted,
  markUpgradeAttempted,
  performLegacyUpgrade,
} from './composables/use-startup-upgrade';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#app');

/* Runs silent legacy upgrade FIRST (if backend detected outdated schema),
 * then pre-warms all Pinia stores. Loop-guard: `window.location.reload()`
 * runs in the SAME Rust process, so managed state is not recomputed. Backend
 * re-probes live schema on each `get_startup_status` call + updates its
 * snapshot after success. `decideUpgrade` + sessionStorage is defense-in-depth:
 * if both backend layers are bypassed, refuse to re-run upgrade. */
async function bootstrap(): Promise<void> {
  // Startup schema upgrade (silent). Must complete before any store reads.
  const needsUpgrade = await getStartupStatus();
  const decision = decideUpgrade(needsUpgrade, getUpgradeAttempted());

  if (decision === 'stale') {
    /* Loop guard tripped: backend still reports Legacy after upgrade already
     * attempted this session. Surface error so user can restart. */
    console.error(
      '[startup_upgrade] stale signal: upgrade still reported as needed after an attempt this session. Aborting to prevent a reload loop; a full app restart is required.'
    );
    useToast().show(
      'Database upgrade could not be verified. Please quit and restart Bango to continue.',
      'error',
      0
    );
    return;
  }

  if (decision === 'run') {
    const toast = useToast();
    toast.show('Database upgrade in progress...', 'info', 0);
    // Record attempt BEFORE awaiting so concurrent bootstrap cannot double-run.
    markUpgradeAttempted();
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
      // Loop-guard token is already set; reload for clean state.
      window.location.reload();
      return;
    }
    /* Reload to re-bootstrap against freshly rebuilt schema. Loop-guard
     * token is set; `getStartupStatus()` should return false now. */
    window.location.reload();
    return;
  }

  // Pre-warm all stores in parallel; signal loading overlay to dismiss on complete.
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
