/**
 * Wiki-mode state for the Chat view: wiki availability (drives the toggle +
 * welcome card), the page-title map (bare UUIDs render as titled chips),
 * the derived article source-metadata map for the wiki renderer, the
 * wiki-mode toggle, and the wiki reader slide-over navigation stack.
 */

import { ref, computed, type Ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useToast } from '@/composables/use-toast';
import { useWiki } from '@/composables/use-wiki';
import type { Article } from '@/types';
import type { WikiStatus, WikiSourceInfo } from '@/types/wiki';

export function useChatWiki(args: {
  /** The chat store's retrieval source ('articles' | 'wiki' | 'citation-finder'). */
  source: Ref<string>;
  /** Whether the wiki is ready (availability flag on the chat store). */
  wikiReady: Ref<boolean>;
  /** Sets the store's wikiReady flag. */
  setWikiReady: (ready: boolean) => void;
  /** Sets the store's retrieval source. */
  setSource: (next: 'articles' | 'wiki' | 'citation-finder') => void;
  /** The store's toggleWikiMode action (flips + returns the next source). */
  toggleWikiMode: () => string;
  /** Loaded articles (feeds the derived wiki source-metadata map). */
  articles: Ref<Article[]>;
  /** Called when the reader opens so the host can close the article panel. */
  onOpenWikiReader: () => void;
}) {
  const toast = useToast();
  const { listPages: wikiListPages, checkForUpdates: wikiCheckForUpdates } = useWiki();

  /** True until the first wiki status check settles. */
  const checkingWiki = ref(true);

  /** Wiki reader slide-over nav stack. Last entry is visible page; popping
   * back to empty closes the panel. Stack lets [[wikilink]] clicks chain. */
  const wikiNavStack = ref<string[]>([]);
  const wikiPanelOpen = computed(() => wikiNavStack.value.length > 0);
  const wikiSlug = computed(() => wikiNavStack.value[wikiNavStack.value.length - 1] ?? null);

  /** Wiki page slug-to-title map. Loaded once so bare UUIDs in wiki-sourced
   * chat bubbles render with human-readable titles instead of raw UUIDs. */
  const wikiPageTitles = ref<Map<string, string>>(new Map());

  /** Derived source-metadata map (article id -> WikiSourceInfo) built
   * reactively from the loaded articles. Passed to `renderWikiMarkdown` so
   * bare article UUIDs in wiki-sourced chat render as green `.art-ref` chips. */
  const wikiSources = computed(() => {
    const map = new Map<string, WikiSourceInfo>();
    for (const a of args.articles.value) {
      map.set(a.id, {
        id: a.id,
        title: a.title,
        authors: a.authors ?? [],
        year: a.publicationYear ?? null,
        doi: a.doi ?? null,
        abstractText: a.abstractText ?? '',
        journal: a.journal ?? null,
      });
    }
    return map;
  });

  /** Fetch wiki status, flip the store's `wikiReady` flag (drives toggle
   * visibility), load page titles, and proactively run the on-demand drift
   * check so wiki-mode chat reflects external edits. */
  async function checkWikiStatus(): Promise<void> {
    try {
      const status = await tauriCommand<WikiStatus>('wiki_get_status');
      args.setWikiReady(!!status.initialized && status.pageCount > 0);
      // If the wiki became unavailable while wiki mode was on, drop back.
      if (!args.wikiReady.value && args.source.value === 'wiki') {
        args.setSource('articles');
      }
      // Load page titles so wiki chat bubbles can render bare UUIDs as
      // synthesis-styled chips with human-readable titles.
      if (args.wikiReady.value && wikiPageTitles.value.size === 0) {
        try {
          const pages = await wikiListPages();
          const map = new Map<string, string>();
          for (const p of pages) {
            map.set(p.slug, p.title);
          }
          wikiPageTitles.value = map;
        } catch {
          // Non-fatal: bare UUIDs fall back to raw text.
        }
      }
    } catch {
      args.setWikiReady(false);
    } finally {
      checkingWiki.value = false;
    }

    /* When wiki is ready, proactively run on-demand drift check so wiki-mode
     * chat reflects external edits since last visit. Debounced 30s via
     * useWiki. */
    if (args.wikiReady.value) {
      try {
        const result = await wikiCheckForUpdates(false);
        if (result?.rebuilt) {
          toast.show(`Wiki updated: ${result.pagesReindexed} pages re-indexed.`, 'success');
        }
      } catch {
        // Non-fatal: wiki chat still works with the existing index.
      }
    }
  }

  /** Flip the wiki / article retrieval mode. */
  function onToggleWiki(): void {
    const next = args.toggleWikiMode();
    if (next === 'wiki') {
      // Entering wiki mode: article context is irrelevant, hide it.
      toast.show('Wiki mode: answers are grounded by FTS5 search over your wiki pages.', 'info');
    }
  }

  /** Open the wiki reader slide-over on a given slug (closes the article
   * panel so only one slide-over is visible at a time). */
  function openWikiPage(slug: string): void {
    args.onOpenWikiReader();
    wikiNavStack.value = [slug];
  }

  /** Inner [[wikilink]] navigation: push onto the stack so Back works. */
  function navigateWiki(slug: string): void {
    wikiNavStack.value = [...wikiNavStack.value, slug];
  }

  /** Pop the wiki reader back-stack; close when the stack empties. */
  function goBackWiki(): void {
    wikiNavStack.value = wikiNavStack.value.slice(0, -1);
  }

  /** Close the wiki reader entirely (clears history). */
  function closeWikiPanel(): void {
    wikiNavStack.value = [];
  }

  return {
    checkingWiki,
    wikiNavStack,
    wikiPanelOpen,
    wikiSlug,
    wikiPageTitles,
    wikiSources,
    checkWikiStatus,
    onToggleWiki,
    openWikiPage,
    navigateWiki,
    goBackWiki,
    closeWikiPanel,
  };
}
