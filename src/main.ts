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

// Bootstrap entry point. Runs the silent legacy upgrade FIRST (if the backend
// detected an outdated schema), then pre-warms all Pinia stores so navigating
// to any view is instant.
async function bootstrap(): Promise<void> {
  // 1. Startup schema upgrade (silent). Must complete before any store reads
  //    from the DB, otherwise they would load from the pre-upgrade schema.
  //
  //    Loop-guard: a webview `window.location.reload()` runs in the SAME Rust
  //    process, so managed backend state is not recomputed. The backend now
  //    re-probes the live schema on each `get_startup_status` call (and
  //    updates its snapshot after a successful upgrade), which alone breaks
  //    the loop. The `decideUpgrade` + sessionStorage check below is the
  //    defense-in-depth safety net: if both backend layers were ever
  //    bypassed, we still refuse to re-run the upgrade in the same session
  //    and instead surface a restart-required error.
  const needsUpgrade = await getStartupStatus();
  const decision = decideUpgrade(needsUpgrade, getUpgradeAttempted());

  if (decision === 'stale') {
    // Loop guard tripped: backend still reports Legacy after we already
    // attempted the upgrade this session. Do NOT reload. Surface a clear
    // error so the user can restart the app (which starts a fresh session
    // and re-probes the schema cleanly).
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
    // Persistent "in progress" toast (duration=0) so it stays visible during
    // the upgrade; it is dismissed explicitly on completion.
    toast.show('Database upgrade in progress...', 'info', 0);
    // Record the attempt BEFORE awaiting so a concurrent bootstrap cannot
    // double-run the upgrade. Session-scoped, so a real app restart clears it.
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
      // On failure, reload so the user lands in a clean state (the backend
      // keeps the backup file safe regardless). The loop-guard token is
      // already set, so even if the failure left a stale Legacy signal the
      // next bootstrap will take the `'stale'` branch and refuse to loop.
      window.location.reload();
      return;
    }
    // Reload the webview so the app re-bootstraps against the freshly rebuilt
    // schema. The loop-guard token is set; if the backend live-probe layer
    // works as expected, getStartupStatus() will now return false (Current)
    // and bootstrap will proceed normally. If it somehow still returns true,
    // the guard takes the `'stale'` branch and stops.
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
