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

/** The result of the last wiki generation (export dir + index path).
 *  Module-level so it survives component re-mounts when the user navigates
 *  away from the wiki toolbar and back. Used to enable the "Open in Browser"
 *  and "Download as Zip" actions after generation. */
const wikiExportResult = ref<GenerateExportResult | null>(null);

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

  async function exportRis(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'included-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_to_file', { path: filePath });
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

  async function exportRisForTab(
    status: string,
    screeningErrorsOnly: boolean,
    label: string
  ): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const slug = label.toLowerCase().replace(/\s+/g, '-');
      const filePath = await save({
        defaultPath: `${slug}-articles.ris`,
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_for_tab_to_file', {
          path: filePath,
          status,
          screeningErrorsOnly,
        });
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

  /** Export a specific set of articles (by UUID) to an RIS file. The sole
   *  entry point for the "Export Selected" bulk action in the Article list
   *  bulk action bar — distinct from the toolbar Export, which exports by
   *  tab/status. Mirrors `exportRisForTab` (save dialog -> IPC -> error/loading
   *  handling). The RIS bytes are produced by the same backend pipeline as the
   *  tab export, so the output is byte-identical for the same article set. */
  async function exportRisForIds(ids: string[]): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'selected-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_for_ids_to_file', { path: filePath, ids });
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

  async function exportProject(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'bango-project.bango.json',
        filters: [
          { name: 'Bango Backup', extensions: ['bango.json'] },
          { name: 'JSON', extensions: ['json'] },
        ],
      });
      if (filePath) {
        await tauriCommand('export_project_to_file', { path: filePath });
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
      // Land on the Dashboard so the user sees a clean "fresh start" view of
      // the freshly imported project, then reload to wipe ALL cached view state
      // (keep-alive components + module-level singletons like useArticleSearch,
      // useBibliometrics, useDashboard, the network graphs, useReferencesSearch,
      // etc.). The app re-bootstraps via main.ts (store pre-warm). Same-process
      // reload keeps the Rust backend (and the just-written DB) alive.
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
      // Delete All Data also wipes the on-disk Wiki (backend deletes the
      // wiki-root directory). Reset the wiki singleton state and the chat
      // store's wiki readiness flag so the UI reflects the empty state
      // (wiki toggle hidden, wiki view shows the first-visit gate).
      useWiki().resetState();
      useChatStore().setWikiReady(false);
      // Land on the Dashboard so the user sees a clean "fresh start" view of
      // the empty project, then reload to wipe ALL cached view state
      // (keep-alive components + module-level singletons like useArticleSearch,
      // useBibliometrics, useDashboard, the network graphs, useReferencesSearch,
      // etc.). The app re-bootstraps via main.ts (store pre-warm). Same-process
      // reload keeps the Rust backend (and the just-reset DB) alive.
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

  /** Default project title for the wiki export: the first research aim, or a
   *  fallback when no aims exist yet. */
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

  /** Open the generated `index.html` in the OS default browser for testing. */
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

  /** Step 2: Zip the `wiki-export/` directory into a user-chosen `.zip` file.
   *  Returns the destination path on success, or null when the user cancels
   *  the save dialog.
   *  @param projectTitle Used to derive the default zip filename:
   *  `bango-wiki-{normalized-title}.zip`. */
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
