import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock nextPaint so we don't need real rAF in the test
vi.mock('@/utils/next-paint', () => ({
  nextPaint: vi.fn().mockResolvedValue(undefined),
}));

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { nextPaint } from '@/utils/next-paint';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useDedup } from '@/composables/use-dedup';
import type { DedupResult, DuplicatePair } from '@/composables/use-dedup';

const pair1: DuplicatePair = {
  articleAId: 'a1',
  articleBId: 'a2',
  articleATitle: 'Article A',
  articleBTitle: 'Article B',
  articleAAuthors: ['Author 1'],
  articleBAuthors: ['Author 2'],
  articleAYear: 2023,
  articleBYear: 2023,
  similarity: 1.0,
  matchType: 'exactDuplicate',
  strategy: 'doi',
};

const pair2: DuplicatePair = {
  articleAId: 'a3',
  articleBId: 'a4',
  articleATitle: 'Article C',
  articleBTitle: 'Article D',
  articleAAuthors: ['Author 3'],
  articleBAuthors: ['Author 4'],
  articleAYear: 2022,
  articleBYear: 2022,
  similarity: 0.85,
  matchType: 'fuzzyMatch',
  strategy: 'title_similarity',
};

const pair3: DuplicatePair = {
  articleAId: 'a5',
  articleBId: 'a6',
  articleATitle: 'Article E',
  articleBTitle: 'Article F',
  articleAAuthors: ['Author 5'],
  articleBAuthors: ['Author 6'],
  articleAYear: null,
  articleBYear: null,
  similarity: 0.78,
  matchType: 'fuzzyMatch',
  strategy: 'title_similarity',
};

const mockDedupResult: DedupResult = {
  exactDuplicates: [pair1],
  fuzzyMatches: [pair2, pair3],
  autoMergedCount: 0,
  needsReviewCount: 2,
};

describe('useDedup', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Initial state ──────────────────────────────────────────────

  describe('initial state', () => {
    it('has no result', () => {
      const { result } = useDedup();
      expect(result.value).toBeNull();
    });

    it('is not loading', () => {
      const { loading } = useDedup();
      expect(loading.value).toBe(false);
    });

    it('has no error', () => {
      const { error } = useDedup();
      expect(error.value).toBeNull();
    });

    it('has zero resolved count', () => {
      const { resolvedCount } = useDedup();
      expect(resolvedCount.value).toBe(0);
    });

    it('has zero merged count', () => {
      const { mergedCount } = useDedup();
      expect(mergedCount.value).toBe(0);
    });
  });

  // ── checkDuplicates ────────────────────────────────────────────

  describe('checkDuplicates', () => {
    it('calls nextPaint before backend to ensure spinner paints', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockDedupResult);

      const dedup = useDedup();
      await dedup.checkDuplicates();

      expect(nextPaint).toHaveBeenCalled();

      // nextPaint must be called BEFORE tauriCommand
      const npOrder = vi.mocked(nextPaint).mock.invocationCallOrder;
      const tcOrder = vi.mocked(tauriCommand).mock.invocationCallOrder;
      expect(npOrder.length).toBeGreaterThan(0);
      expect(tcOrder.length).toBeGreaterThan(0);
      expect(npOrder[0] as number).toBeLessThan(tcOrder[0] as number);
    });

    it('sets loading=true immediately before nextPaint', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockDedupResult);

      const dedup = useDedup();

      const loadingStates: boolean[] = [];
      vi.mocked(nextPaint).mockImplementationOnce(async () => {
        loadingStates.push(dedup.loading.value);
      });

      await dedup.checkDuplicates();
      expect(loadingStates).toContain(true);
    });

    it('calls check_duplicates command', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockDedupResult);

      const dedup = useDedup();
      const res = await dedup.checkDuplicates();

      expect(tauriCommand).toHaveBeenCalledWith('check_duplicates');
      expect(res).toEqual(mockDedupResult);
      expect(dedup.result.value).toEqual(mockDedupResult);
    });

    it('sets loading=false after success', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockDedupResult);

      const dedup = useDedup();
      await dedup.checkDuplicates();
      expect(dedup.loading.value).toBe(false);
    });

    it('sets error and returns null on failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Dedup error'));

      const dedup = useDedup();
      const res = await dedup.checkDuplicates();

      expect(res).toBeNull();
      expect(dedup.error.value).toBe('Dedup error');
      expect(dedup.loading.value).toBe(false);
    });

    it('handles non-Error exceptions', async () => {
      vi.mocked(tauriCommand).mockRejectedValue('string error');

      const dedup = useDedup();
      const res = await dedup.checkDuplicates();

      expect(res).toBeNull();
      expect(dedup.error.value).toBe('string error');
    });
  });

  // ── mergeAllExact ──────────────────────────────────────────────

  describe('mergeAllExact', () => {
    it('returns 0 if no exact duplicates', async () => {
      const dedup = useDedup();
      // result is null, so no exact duplicates
      const count = await dedup.mergeAllExact();
      expect(count).toBe(0);
      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('merges exact duplicates and re-checks', async () => {
      vi.mocked(tauriCommand)
        .mockResolvedValueOnce(1) // merge_exact_duplicates returns count
        .mockResolvedValueOnce({
          // check_duplicates after merge
          exactDuplicates: [],
          fuzzyMatches: [pair2, pair3],
          autoMergedCount: 1,
          needsReviewCount: 2,
        });

      const dedup = useDedup();
      // Set up result with exact duplicates
      dedup.result.value = { ...mockDedupResult };

      const count = await dedup.mergeAllExact();

      expect(count).toBe(1);
      expect(dedup.mergedCount.value).toBe(1);
      expect(tauriCommand).toHaveBeenCalledWith('merge_exact_duplicates', {
        request: { pairs: [pair1] },
      });
      // Should re-check after merge
      expect(tauriCommand).toHaveBeenCalledWith('check_duplicates');
    });

    it('sets error on merge failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Merge failed'));

      const dedup = useDedup();
      dedup.result.value = { ...mockDedupResult };

      const count = await dedup.mergeAllExact();

      expect(count).toBe(0);
      expect(dedup.error.value).toBe('Merge failed');
      expect(dedup.loading.value).toBe(false);
    });
  });

  // ── resolveFuzzy ───────────────────────────────────────────────

  describe('resolveFuzzy', () => {
    it('resolves a fuzzy match and removes it from list', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const dedup = useDedup();
      dedup.result.value = { ...mockDedupResult };

      await dedup.resolveFuzzy(pair2, 'keepA');

      expect(tauriCommand).toHaveBeenCalledWith('resolve_fuzzy_match', {
        request: {
          pairIndex: 0,
          resolution: 'keepA',
          articleAId: pair2.articleAId,
          articleBId: pair2.articleBId,
        },
      });

      expect(dedup.resolvedCount.value).toBe(1);
      // pair2 should be removed from fuzzyMatches
      expect(dedup.result.value!.fuzzyMatches.length).toBe(1);
      const remaining = dedup.result.value!.fuzzyMatches;
      expect(remaining[0]!.articleAId).toBe('a5');
      // needsReviewCount should be updated
      expect(dedup.result.value!.needsReviewCount).toBe(1);
    });

    it('resolves multiple fuzzy matches sequentially', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const dedup = useDedup();
      dedup.result.value = { ...mockDedupResult };

      await dedup.resolveFuzzy(pair2, 'keepA');
      await dedup.resolveFuzzy(pair3, 'keepBoth');

      expect(dedup.resolvedCount.value).toBe(2);
      expect(dedup.result.value!.fuzzyMatches.length).toBe(0);
      expect(dedup.result.value!.needsReviewCount).toBe(0);
    });

    it('sets error on resolve failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Resolve failed'));

      const dedup = useDedup();
      dedup.result.value = { ...mockDedupResult };

      await dedup.resolveFuzzy(pair2, 'keepB');

      expect(dedup.error.value).toBe('Resolve failed');
      expect(dedup.resolvedCount.value).toBe(0); // should NOT increment on error
      // pair should NOT be removed on error
      expect(dedup.result.value!.fuzzyMatches.length).toBe(2);
      expect(dedup.loading.value).toBe(false);
    });

    it('sets loading=false in finally block on success', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const dedup = useDedup();
      dedup.result.value = { ...mockDedupResult };

      await dedup.resolveFuzzy(pair2, 'keepBoth');
      expect(dedup.loading.value).toBe(false);
    });
  });
});
