import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { openPath } from '@tauri-apps/plugin-opener';
import { tauriCommand } from './use-tauri-command';
import { useLoadingOverlay } from './use-loading-overlay';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { useChatStore } from '@/stores/chat';
import { useSummary } from './use-summary';
import { useWiki } from './use-wiki';
import { generateWikiExport, zipWikiExport } from '@/utils/wiki-site-export';
import type { GenerateExportResult } from '@/types/wiki';

/** Get the module-level wiki export result (survives remount). */
const wikiExportResult = ref<GenerateExportResult | null>(null);

/** Shared save-dialog filter set for RIS exports. */
const RIS_FILTERS = [{ name: 'RIS File', extensions: ['ris'] }];

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);
  const { withOverlay } = useLoadingOverlay();

  function invalidateAllStores(): void {
    useArticlesStore().invalidate();
    useCriteriaStore().invalidate();
    useTagsStore().invalidate();
    useLabelsStore().invalidate();
    useLlmConfigStore().invalidate();
    useAuditStore().invalidate();
    useScreeningStore().invalidate();
  }

  /** Invalidate all stores and then proactively re-fetch so data is immediately available. */
  async function refreshAllStores(): Promise<void> {
    invalidateAllStores();
    await Promise.all([
      useArticlesStore().fetchIfNeeded(),
      useCriteriaStore().fetchIfNeeded(),
      useTagsStore().fetchIfNeeded(),
      useLabelsStore().fetchIfNeeded(),
      useLlmConfigStore().fetchIfNeeded(),
      useAuditStore().fetchIfNeeded(),
      useScreeningStore().fetchIfNeeded(),
    ]);
  }

  /* Shared file-export scaffold: OS save dialog -> IPC command, with the
   * `exporting`/`error` flags managed around it. Returns false on dialog
   * cancel or invoke error. IPC command names are frozen (backend contract). */
  async function runExport(
    command: string,
    args: Record<string, unknown>,
    defaultPath: string,
    filters: { name: string; extensions: string[] }[] = RIS_FILTERS
  ): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({ defaultPath, filters });
      if (filePath) {
        await tauriCommand(command, { ...args, path: filePath });
        return true;
      }
      return false;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  /** Export the Included list to RIS. */
  function exportRis(): Promise<boolean> {
    return runExport('export_ris_to_file', {}, 'included-articles.ris');
  }

  /** Export one status tab to RIS (`screeningErrorsOnly` for the Error tab). */
  function exportRisForTab(
    status: string,
    screeningErrorsOnly: boolean,
    label: string
  ): Promise<boolean> {
    const slug = label.toLowerCase().replace(/\s+/g, '-');
    return runExport(
      'export_ris_for_tab_to_file',
      { status, screeningErrorsOnly },
      `${slug}-articles.ris`
    );
  }

  /** Export a specific set of articles (by UUID) to RIS. */
  function exportRisForIds(ids: string[]): Promise<boolean> {
    return runExport('export_ris_for_ids_to_file', { ids }, 'selected-articles.ris');
  }

  /** Export the whole project as a `.bango.json` backup. */
  function exportProject(): Promise<boolean> {
    return runExport('export_project_to_file', {}, 'bango-project.bango.json', [
      { name: 'Bango Backup', extensions: ['bango.json'] },
      { name: 'JSON', extensions: ['json'] },
    ]);
  }

  async function importProject(file: File): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const content = await file.text();
      await withOverlay('Importing Project Backup...', async () => {
        await tauriCommand('import_project_backup', {
          request: { jsonContent: content },
        });
        await refreshAllStores();
        useSummary().clearSummary();
      });
      /* Land on Dashboard, then reload to wipe all cached view state
      (keep-alive components + module-level singletons). Same-process
      reload keeps the Rust backend alive. */
      window.location.hash = '#/';
      window.location.reload();
      return true;
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error('[import] Import failed:', msg, e);
      error.value = msg;
      return false;
    } finally {
      exporting.value = false;
    }
  }

  async function resetProject(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      await tauriCommand('reset_project');
      invalidateAllStores();
      useSummary().clearSummary();
      // Wipe wiki singleton state + chat store's wiki readiness flag.
      useWiki().resetState();
      useChatStore().setWikiReady(false);
      // Land on Dashboard, then reload to wipe cached view state.
      window.location.hash = '#/';
      window.location.reload();
      return true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  /** Default project title for wiki export: first aim text, or fallback. */
  function defaultWikiTitle(): string {
    const aims = useCriteriaStore().aims;
    if (aims.length > 0 && aims[0]?.text) {
      return aims[0].text;
    }
    return 'Bango Wiki';
  }

  /** Step 1: Generate the wiki static site to `wiki-root/wiki-export/`.
   *  Returns true on success and populates `wikiExportResult`. */
  async function generateWikiSite(projectTitle: string): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const result = await withOverlay('Generating Wiki website...', () =>
        generateWikiExport(projectTitle)
      );
      wikiExportResult.value = result as GenerateExportResult;
      return true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  /** Open the generated index.html in the OS default browser. */
  async function openWikiExport(): Promise<boolean> {
    if (!wikiExportResult.value) return false;
    try {
      await openPath(wikiExportResult.value.indexPath);
      return true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    }
  }

  /** Zip the wiki-export directory into a user-chosen .zip file. */
  async function downloadWikiZip(projectTitle: string): Promise<string | null> {
    exporting.value = true;
    error.value = null;
    try {
      const result = await withOverlay('Zipping Wiki website...', () =>
        zipWikiExport(projectTitle)
      );
      return result as string | null;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      exporting.value = false;
    }
  }

  return {
    exporting,
    error,
    exportRis,
    exportRisForTab,
    exportRisForIds,
    exportProject,
    importProject,
    resetProject,
    generateWikiSite,
    openWikiExport,
    downloadWikiZip,
    wikiExportResult,
    defaultWikiTitle,
  };
}
