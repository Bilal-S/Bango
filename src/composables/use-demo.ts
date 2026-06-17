import { ref } from 'vue';
import type { Router } from 'vue-router';
import { tauriCommand, isTauri } from '@/composables/use-tauri-command';
import { useLoadingOverlay } from '@/composables/use-loading-overlay';
import { ask } from '@tauri-apps/plugin-dialog';
import demoProjectJson from '@/assets/demo-project.bango.json?raw';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';

/**
 * Shared composable for loading the demo project.
 * Used by both dashboard.vue and help-guide.vue.
 */
export function useDemo(router: Router) {
  const demoLoading = ref(false);
  const demoError = ref<string | null>(null);
  const { withOverlay } = useLoadingOverlay();

  async function loadDemo(): Promise<void> {
    if (demoLoading.value) return;
    if (!isTauri()) {
      demoError.value = 'Demo requires the desktop app.';
      return;
    }

    // Always confirm via native dialog - this is destructive (replaces all project data)
    const confirmed = await ask(
      'Loading the demo project will replace all your current data ' +
        '(articles, criteria, tags, labels). This cannot be undone.',
      { title: 'Load Demo Project', kind: 'warning', okLabel: 'Load Demo', cancelLabel: 'Cancel' }
    );
    if (!confirmed) return;

    demoLoading.value = true;
    demoError.value = null;
    try {
      await withOverlay('Loading Demo Project...', async () => {
        await tauriCommand('import_project_backup', {
          request: { jsonContent: demoProjectJson },
        });
        // Invalidate and re-fetch all stores
        const stores = [
          useArticlesStore(),
          useCriteriaStore(),
          useTagsStore(),
          useLabelsStore(),
          useLlmConfigStore(),
          useAuditStore(),
          useScreeningStore(),
        ];
        for (const store of stores) {
          store.invalidate();
        }
        await Promise.all(stores.map((s) => s.fetchIfNeeded()));
      });
      router.push('/');
    } catch (e: unknown) {
      demoError.value = e instanceof Error ? e.message : String(e);
    } finally {
      demoLoading.value = false;
    }
  }

  return { demoLoading, demoError, loadDemo };
}
