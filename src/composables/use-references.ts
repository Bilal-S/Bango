import { ref, type Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useToast } from './use-toast';
import type { ArticleReference, ReferenceType } from '../types';

export interface ExtractResult {
  papersCreated: number;
  linksCreated: number;
  errors: string[];
}

export interface PreviewPaper {
  title: string | null;
  authors: string[];
  publicationYear: number | null;
  doi: string | null;
  journal: string | null;
}

export interface PreviewResult {
  papers: PreviewPaper[];
  totalCount: number;
  errors: string[];
}

/** Result from scrape_citation_chaser_cmd (camelCase via serde) */
interface ScrapeCitationChaserResult {
  referencesRis: string | null;
  citationsRis: string | null;
}

// ── Module-level reactive state for auto-download (persists across component lifecycles) ──
const autoDownloadMap = ref(new Map<string, boolean>());

/**
 * Check whether an auto-download operation is currently in progress for an article.
 * Reactive — safe to use in computed/template.
 */
export function isAutoDownloading(articleId: string): boolean {
  return autoDownloadMap.value.get(articleId) === true;
}

/**
 * Auto-download references/citations via Citation Chaser and import them.
 *
 * Runs in the background — fire-and-forget from the caller's perspective.
 * Double-submit is prevented by the module-level reactive `autoDownloadMap`.
 *
 * @param reloadFn  Called after successful import so the caller can refresh its list.
 * @param onComplete  Called with `true` on success or `false` on error (after toast).
 */
export function autoDownloadReferences(
  articleId: string,
  doi: string,
  articleTitle: string,
  needRefs: boolean,
  needCites: boolean,
  reloadFn: () => Promise<void>,
  onComplete?: (success: boolean) => void
): void {
  // Double-submit prevention
  if (autoDownloadMap.value.get(articleId)) return;
  autoDownloadMap.value.set(articleId, true);
  // Trigger reactivity
  autoDownloadMap.value = new Map(autoDownloadMap.value);

  const toast = useToast();

  // Fire-and-forget async chain
  void (async () => {
    try {
      // 1. Scrape Citation Chaser
      const scrapeResult = await tauriCommand<ScrapeCitationChaserResult>(
        'scrape_citation_chaser_cmd',
        { doi, getReferences: needRefs, getCitations: needCites }
      );

      // 2. Import each non-null RIS result
      if (scrapeResult.referencesRis) {
        await tauriCommand<ExtractResult>('import_references_for_article', {
          payload: { articleId, filePath: scrapeResult.referencesRis, refType: 'reference' },
        });
      }
      if (scrapeResult.citationsRis) {
        await tauriCommand<ExtractResult>('import_references_for_article', {
          payload: { articleId, filePath: scrapeResult.citationsRis, refType: 'citation' },
        });
      }

      // 3. Reload references list
      await reloadFn();

      toast.show(`References imported for ${articleTitle}`, 'success');
      onComplete?.(true);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.show(`Error: ${msg}`, 'error');
      onComplete?.(false);
    } finally {
      autoDownloadMap.value.delete(articleId);
      // Trigger reactivity
      autoDownloadMap.value = new Map(autoDownloadMap.value);
    }
  })();
}

/**
 * Composable for managing article references (citations & cited references).
 */
export function useReferences() {
  const loading = ref(false);
  const error: Ref<string | null> = ref(null);

  /**
   * Get all reference papers linked to an article.
   */
  async function getArticleReferences(
    articleId: string,
    refType?: ReferenceType
  ): Promise<ArticleReference[]> {
    loading.value = true;
    error.value = null;
    try {
      const args: Record<string, unknown> = { articleId };
      if (refType) args.refType = refType;
      return await tauriCommand<ArticleReference[]>('get_article_references', args);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      return [];
    } finally {
      loading.value = false;
    }
  }

  /**
   * Extract CR (Cited References) from an article's RIS extras.
   */
  async function extractCrReferences(
    articleId: string,
    risExtras: Record<string, unknown> | null
  ): Promise<ExtractResult | null> {
    loading.value = true;
    error.value = null;
    try {
      return await tauriCommand<ExtractResult>('extract_cr_references', {
        payload: { articleId, risExtras },
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Manually link a reference paper to an article.
   */
  async function linkReferenceToArticle(
    articleId: string,
    referencePaperId: string,
    refType: ReferenceType
  ): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await tauriCommand('link_reference_to_article', {
        articleId,
        referencePaperId,
        refType,
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Delete all references for an article.
   */
  async function deleteArticleReferences(articleId: string): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await tauriCommand('delete_article_references', { articleId });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Preview references/citations from a file without importing.
   * Parses the file and returns what would be imported (no DB writes).
   */
  async function previewReferencesImport(filePath: string): Promise<PreviewResult | null> {
    loading.value = true;
    error.value = null;
    try {
      return await tauriCommand<PreviewResult>('preview_references_import', { filePath });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Import references/citations from an RIS file for a specific article.
   * refType: "reference" = Backward Citations, "citation" = Forward Citations
   */
  async function importReferencesForArticle(
    articleId: string,
    filePath: string,
    refType: 'reference' | 'citation'
  ): Promise<ExtractResult | null> {
    loading.value = true;
    error.value = null;
    try {
      return await tauriCommand<ExtractResult>('import_references_for_article', {
        payload: { articleId, filePath, refType },
      });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      return null;
    } finally {
      loading.value = false;
    }
  }

  /**
   * Promote a reference paper to a full article in the library.
   * Returns the new article ID on success.
   */
  async function promoteReferenceToArticle(
    referencePaperId: string
  ): Promise<{ articleId: string; articleTitle: string; wasLinked: boolean } | null> {
    loading.value = true;
    error.value = null;
    try {
      return await tauriCommand<{ articleId: string; articleTitle: string; wasLinked: boolean }>(
        'promote_reference_to_article',
        { referencePaperId }
      );
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      error.value = msg;
      return null;
    } finally {
      loading.value = false;
    }
  }

  return {
    loading,
    error,
    getArticleReferences,
    extractCrReferences,
    linkReferenceToArticle,
    deleteArticleReferences,
    previewReferencesImport,
    importReferencesForArticle,
    promoteReferenceToArticle,
  };
}
