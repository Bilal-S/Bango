import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';

import type {
  LintReport,
  RawExportReport,
  RawFileEntry,
  IngestReport,
  WikiChatMessage,
  WikiGraph,
  WikiPageSummary,
  WikiProgress,
  WikiInitResult,
  WikiPage,
  WikiPageHit,
  WikiRootInfo,
  WikiSourceInfo,
  WikiStatus,
} from '@/types/wiki';

/**
 * Wiki composable.
 *
 * Wraps the Tauri wiki commands and exposes reactive state for the wiki view.
 * Phase 1 scope: status, root dir get/set, init. Ingest, lint, search, chat,
 * and page CRUD are added in later phases (see `.worktrees/llmwiki-plan.md`).
 */
// ── Module-level singleton state (shared by all useWiki() callers) ──
const status = ref<WikiStatus | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const initializing = ref(false);
const progress = ref<WikiProgress | null>(null);
let progressUnlisten: (() => void) | null = null;

export function useWiki() {
  /** Fetch the current wiki status from the backend. */
  async function refreshStatus(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      status.value = await tauriCommand<WikiStatus>('wiki_get_status');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  /** Get the effective wiki-root directory info. */
  async function getRootDir(): Promise<WikiRootInfo> {
    return tauriCommand<WikiRootInfo>('wiki_get_root_dir');
  }

  /**
   * Set an explicit wiki-root override. Pass empty string to reset to default.
   * Refreshes status after a successful change.
   */
  async function setRootDir(path: string): Promise<WikiRootInfo> {
    const info = await tauriCommand<WikiRootInfo>('wiki_set_root_dir', { path });
    await refreshStatus();
    return info;
  }

  /**
   * Initialize the wiki: scaffold the directory tree, write AGENTS.md, seed
   * templates. Idempotent. Refreshes status after a successful init.
   */
  async function initWiki(): Promise<WikiInitResult> {
    initializing.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<WikiInitResult>('wiki_init');
      await refreshStatus();
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      initializing.value = false;
    }
  }

  /**
   * Export raw sources: included articles from DB + process user-dropped files.
   * Returns a report of how many files were written/skipped.
   */
  async function exportRaw(): Promise<RawExportReport> {
    return tauriCommand<RawExportReport>('wiki_export_raw');
  }

  /**
   * Add a user-selected file to `raw/` and extract its companion `.md`.
   * Returns the companion `.md` path.
   */
  async function addRawFile(filePath: string): Promise<string> {
    const companionPath = await tauriCommand<string>('wiki_add_raw_file', { filePath });
    await refreshStatus();
    return companionPath;
  }

  /** List all `.md` raw sources with their parsed metadata. */
  async function listRawFiles(): Promise<RawFileEntry[]> {
    return tauriCommand<RawFileEntry[]>('wiki_list_raw_files');
  }

  /** Search the wiki FTS5 index. Returns BM25-ranked hits. */
  async function searchWiki(query: string, limit = 10): Promise<WikiPageHit[]> {
    return tauriCommand<WikiPageHit[]>('wiki_search', { query, limit });
  }

  /** Lint the wiki: detect broken links, orphans, duplicates, missing frontmatter. */
  async function lintWiki(): Promise<LintReport> {
    return tauriCommand<LintReport>('wiki_lint');
  }

  /** Get a single wiki page by slug (returns null if not found). */
  async function getPage(slug: string): Promise<WikiPage | null> {
    if (!slug) return null;
    return tauriCommand<WikiPage | null>('wiki_get_page', { slug });
  }

  /** Update a wiki page's title, summary, and body. */
  async function updatePage(
    slug: string,
    title: string,
    summary: string,
    body: string
  ): Promise<WikiPage> {
    const updated = await tauriCommand<WikiPage>('wiki_update_page', {
      slug,
      title,
      summary,
      body,
    });
    return updated;
  }

  /** Delete a single wiki page by slug. Returns true if a page was deleted. */
  async function deletePage(slug: string): Promise<boolean> {
    const deleted = await tauriCommand<boolean>('wiki_delete_page', { slug });
    if (deleted) {
      await refreshStatus();
    }
    return deleted;
  }

  /** List all raw source articles (metadata for [^art-id] reference resolution). */
  async function listSources(): Promise<WikiSourceInfo[]> {
    return tauriCommand<WikiSourceInfo[]>('wiki_list_sources');
  }

  /** List all wiki pages (metadata only, no body). Sorted by type then title. */
  async function listPages(): Promise<WikiPageSummary[]> {
    return tauriCommand<WikiPageSummary[]>('wiki_list_pages');
  }

  /** Run the LLM wiki ingest: synthesize raw sources into wiki pages. */
  async function ingestWiki(): Promise<IngestReport> {
    const report = await tauriCommand<IngestReport>('wiki_ingest');
    await refreshStatus();
    return report;
  }

  /** Get the wiki link graph (nodes + edges) for visualization. */
  async function getGraph(): Promise<WikiGraph> {
    return tauriCommand<WikiGraph>('wiki_get_graph');
  }

  /** Send a wiki-grounded chat message (FTS5 RAG). Returns the assistant response. */
  async function chatWiki(question: string, history: WikiChatMessage[]): Promise<string> {
    return tauriCommand<string>('wiki_chat', { question, history });
  }

  /** Delete the entire wiki output (keeps raw/, templates/, AGENTS.md). */
  async function deleteWiki(): Promise<void> {
    await tauriCommand<void>('wiki_delete_wiki');
    await refreshStatus();
  }

  /** Start listening for wiki:progress events. Call on mount. */
  async function startProgressListener(): Promise<void> {
    if (progressUnlisten) return;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      progressUnlisten = await listen<WikiProgress>('wiki:progress', (event) => {
        progress.value = event.payload;
        if (event.payload.step >= event.payload.totalSteps) {
          setTimeout(() => {
            progress.value = null;
          }, 1500);
        }
      });
    } catch {
      // Non-fatal: progress bar won't show.
    }
  }

  /** Stop listening for wiki:progress events. Call on unmount. */
  function stopProgressListener(): void {
    if (progressUnlisten) {
      progressUnlisten();
      progressUnlisten = null;
    }
  }

  /** Full rebuild: scaffold + export raw + ingest. Emits wiki:progress. */
  async function rebuild(): Promise<IngestReport> {
    const report = await tauriCommand<IngestReport>('wiki_rebuild');
    await refreshStatus();
    return report;
  }

  /** Export raw + ingest in one call (after Add Documents). Emits wiki:progress. */
  async function exportAndIngest(): Promise<IngestReport> {
    const report = await tauriCommand<IngestReport>('wiki_export_and_ingest');
    await refreshStatus();
    return report;
  }

  /** Reset all shared state (for tests). */
  function resetState(): void {
    status.value = null;
    loading.value = false;
    error.value = null;
    initializing.value = false;
    progress.value = null;
  }

  return {
    progress,
    startProgressListener,
    stopProgressListener,

    status,
    loading,
    error,
    initializing,
    refreshStatus,
    getRootDir,
    setRootDir,
    initWiki,
    exportRaw,
    addRawFile,
    listRawFiles,
    searchWiki,
    lintWiki,
    getPage,
    updatePage,
    deletePage,
    deleteWiki,
    chatWiki,
    getGraph,
    listPages,
    listSources,
    ingestWiki,
    rebuild,
    exportAndIngest,
    resetState,
  };
}
