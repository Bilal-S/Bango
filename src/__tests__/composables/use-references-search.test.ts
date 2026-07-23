import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock tauriCommand so we can assert which backend commands fire and in what
// order. The mock dispatches on the command name so each Tauri call
// (`promote_reference_to_article`, `query_reference_papers`,
// `get_reference_articles_of_interest`) returns the right shape.
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useReferencesSearch } from '@/composables/use-references-search';

/** Build a mock implementation that dispatches on the command name. */
function mockTauriCommandDispatch(
  handlers: Record<string, (args: Record<string, unknown>) => unknown>
): void {
  vi.mocked(tauriCommand).mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      const handler = handlers[cmd];
      if (handler) return handler(args ?? {});
      throw new Error(`Unexpected command in test: ${cmd}`);
    }
  );
}

describe('useReferencesSearch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('promotePaper', () => {
    it('calls promote_reference_to_article and returns the new articleId', async () => {
      mockTauriCommandDispatch({
        promote_reference_to_article: () => ({ articleId: 'art-1', articleTitle: 'Title' }),
        // loadPage() refresh
        query_reference_papers: () => ({ papers: [], total: 0 }),
        // loadArticlesOfInterest() refresh (default mode)
        get_reference_articles_of_interest: () => [],
      });

      const { promotePaper } = useReferencesSearch();
      const result = await promotePaper('paper-1');

      expect(result).toBe('art-1');
      expect(tauriCommand).toHaveBeenCalledWith('promote_reference_to_article', {
        referencePaperId: 'paper-1',
      });
    });

    it('refreshes both papers and articles-of-interest by default', async () => {
      const callOrder: string[] = [];
      mockTauriCommandDispatch({
        promote_reference_to_article: () => {
          callOrder.push('promote');
          return { articleId: 'art-1', articleTitle: 'Title' };
        },
        query_reference_papers: () => {
          callOrder.push('loadPage');
          return { papers: [], total: 0 };
        },
        get_reference_articles_of_interest: () => {
          callOrder.push('loadArticlesOfInterest');
          return [];
        },
      });

      const { promotePaper } = useReferencesSearch();
      await promotePaper('paper-1');

      // All three refreshes fire when refreshArticlesOfInterest defaults to true.
      expect(callOrder).toContain('loadPage');
      expect(callOrder).toContain('loadArticlesOfInterest');
    });

    it('skips loadArticlesOfInterest when refreshArticlesOfInterest is false', async () => {
      const callOrder: string[] = [];
      mockTauriCommandDispatch({
        promote_reference_to_article: () => {
          callOrder.push('promote');
          return { articleId: 'art-1', articleTitle: 'Title' };
        },
        query_reference_papers: () => {
          callOrder.push('loadPage');
          return { papers: [], total: 0 };
        },
        get_reference_articles_of_interest: () => {
          callOrder.push('loadArticlesOfInterest');
          return [];
        },
      });

      const { promotePaper, articlesOfInterest } = useReferencesSearch();
      await promotePaper('paper-1', { refreshArticlesOfInterest: false });

      // loadPage still fires (main papers list), but the articles-of-interest
      // refresh is skipped so the caller can animate the card out locally.
      expect(callOrder).toContain('loadPage');
      expect(callOrder).not.toContain('loadArticlesOfInterest');
      // articlesOfInterest is untouched by promotePaper in skip mode.
      expect(articlesOfInterest.value).toEqual([]);
    });

    it('returns null and sets error when the promote command fails', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Promotion failed'));

      const { promotePaper, error } = useReferencesSearch();
      const result = await promotePaper('paper-1');

      expect(result).toBeNull();
      expect(error.value).toBe('Promotion failed');
    });

    it('passes the referencePaperId through unchanged', async () => {
      mockTauriCommandDispatch({
        promote_reference_to_article: () => ({ articleId: 'art-2', articleTitle: 'T' }),
        query_reference_papers: () => ({ papers: [], total: 0 }),
      });

      const { promotePaper } = useReferencesSearch();
      await promotePaper('paper-abc', { refreshArticlesOfInterest: false });

      expect(tauriCommand).toHaveBeenCalledWith('promote_reference_to_article', {
        referencePaperId: 'paper-abc',
      });
    });
  });
});
