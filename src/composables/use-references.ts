import { ref, computed, type Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useToast } from './use-toast';
import type { ArticleReference, ReferenceType, Article, BatchRefScrapingProgress } from '../types';

interface ExtractResult {
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

interface PreviewResult {
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
 * Reactive - safe to use in computed/template.
 */
export function isAutoDownloading(articleId: string): boolean {
  return autoDownloadMap.value.get(articleId) === true;
}

/**
 * Auto-download references/citations via Citation Chaser and import them.
 *
 * Runs in the background - fire-and-forget from the caller's perspective.
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

// ── Module-level reactive state for batch reference scraping ──
const batchProgress = ref<BatchRefScrapingProgress>({
  total: 0,
  completed: 0,
  scraped: 0,
  skipped: 0,
  errors: 0,
  isRunning: false,
  currentArticleTitle: '',
});
const batchCancelled = ref(false);

/**
 * Composable for batch reference scraping across all included articles.
 *
 * Uses module-level reactive state so all components share the same
 * progress singleton. The batch loop runs entirely on the frontend,
 * reusing existing Tauri commands for scraping and importing.
 */
export function useBatchReferenceScraping() {
  const toast = useToast();

  const batchPercentage = computed(() => {
    if (!batchProgress.value.total) return 0;
    return Math.round((batchProgress.value.completed / batchProgress.value.total) * 100);
  });

  /**
   * Start batch scraping for all included articles that are missing
   * references or citations.
   *
   * @param allIncludedArticles  All articles with status 'included'
   * @param onComplete           Called after batch finishes (success or cancel)
   */
  async function startBatchScraping(
    allIncludedArticles: Article[],
    onComplete: () => Promise<void>
  ): Promise<void> {
    // Prevent double-start
    if (batchProgress.value.isRunning) return;

    batchProgress.value = {
      total: allIncludedArticles.length,
      completed: 0,
      scraped: 0,
      skipped: 0,
      errors: 0,
      isRunning: true,
      currentArticleTitle: '',
    };
    batchCancelled.value = false;

    for (const article of allIncludedArticles) {
      if (batchCancelled.value) break;

      batchProgress.value = {
        ...batchProgress.value,
        currentArticleTitle: article.title || '(untitled)',
      };

      // Check if this article needs scraping
      const needsRefs = !article.hasReferenceDetails;
      const needsCites = !article.hasCitationDetails;
      const needsScraping = !!(article.doi && (needsRefs || needsCites));

      if (!needsScraping) {
        // Article already has both or has no DOI - skip
        batchProgress.value = {
          ...batchProgress.value,
          completed: batchProgress.value.completed + 1,
          skipped: batchProgress.value.skipped + 1,
        };
        continue;
      }

      try {
        // 1. Scrape Citation Chaser (shortcuts if RIS files exist on disk)
        const scrapeResult = await tauriCommand<ScrapeCitationChaserResult>(
          'scrape_citation_chaser_cmd',
          {
            doi: article.doi!,
            getReferences: needsRefs,
            getCitations: needsCites,
          }
        );

        // 2. Import each non-null RIS result
        if (scrapeResult.referencesRis) {
          await tauriCommand<ExtractResult>('import_references_for_article', {
            payload: {
              articleId: article.id,
              filePath: scrapeResult.referencesRis,
              refType: 'reference',
            },
          });
        }
        if (scrapeResult.citationsRis) {
          await tauriCommand<ExtractResult>('import_references_for_article', {
            payload: {
              articleId: article.id,
              filePath: scrapeResult.citationsRis,
              refType: 'citation',
            },
          });
        }

        batchProgress.value = {
          ...batchProgress.value,
          completed: batchProgress.value.completed + 1,
          scraped: batchProgress.value.scraped + 1,
        };
      } catch {
        batchProgress.value = {
          ...batchProgress.value,
          completed: batchProgress.value.completed + 1,
          errors: batchProgress.value.errors + 1,
        };
      }
    }

    const wasCancelled = batchCancelled.value;
    batchProgress.value = {
      ...batchProgress.value,
      isRunning: false,
      currentArticleTitle: '',
    };

    if (wasCancelled) {
      toast.show('Batch reference import cancelled', 'info');
    } else if (batchProgress.value.errors > 0) {
      toast.show(
        `Batch complete: ${batchProgress.value.scraped} scraped, ${batchProgress.value.errors} errors`,
        'warning'
      );
    } else {
      toast.show(
        `Batch complete: ${batchProgress.value.scraped} articles scraped, ${batchProgress.value.skipped} skipped`,
        'success'
      );
    }

    await onComplete();
  }

  /** Cancel the running batch after the current article finishes. */
  function cancelBatchScraping(): void {
    batchCancelled.value = true;
  }

  /** Reset batch progress so the progress card is hidden. */
  function resetBatchProgress(): void {
    batchProgress.value = {
      completed: 0,
      total: 0,
      scraped: 0,
      skipped: 0,
      errors: 0,
      currentArticleTitle: '',
      isRunning: false,
    };
  }

  return {
    batchProgress,
    batchPercentage,
    startBatchScraping,
    cancelBatchScraping,
    resetBatchProgress,
  };
}
