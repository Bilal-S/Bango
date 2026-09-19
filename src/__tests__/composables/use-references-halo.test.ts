import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock tauriCommand so refresh() gets controlled payloads for
// get_reference_articles_of_interest. Same dispatch-on-command-name pattern as
// the use-references-search tests.
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { REFERENCES_HALO_MIN_USES, useReferencesHalo } from '@/composables/use-references-halo';
import type { ReferencePaperQuery } from '@/types';

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

/** Full ReferencePaperQuery fixture; only the use-count fields matter here. */
function makePaper(citationCount: number, referenceCount: number): ReferencePaperQuery {
  return {
    title: null,
    abstractText: null,
    authors: [],
    publicationYear: null,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: [],
    url: null,
    language: null,
    publisher: null,
    id: `paper-${citationCount}-${referenceCount}`,
    matchStatus: 'unmatched',
    matchedArticleId: null,
    citationCount,
    referenceCount,
    importSource: null,
    createdAt: '2026-01-01T00:00:00Z',
    referenceType: null,
  };
}

describe('useReferencesHalo', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fetches articles of interest and flags at the exact threshold', async () => {
    // 3 + 1 = 4 total uses - exactly REFERENCES_HALO_MIN_USES, so the halo is on.
    mockTauriCommandDispatch({
      get_reference_articles_of_interest: () => [makePaper(1, 0), makePaper(3, 1)],
    });

    const { refresh, hasHighUseReferences } = useReferencesHalo();
    await refresh();

    expect(tauriCommand).toHaveBeenCalledWith('get_reference_articles_of_interest', {});
    expect(REFERENCES_HALO_MIN_USES).toBe(4);
    expect(hasHighUseReferences.value).toBe(true);
  });

  it('stays off below the threshold (3-use boundary)', async () => {
    mockTauriCommandDispatch({
      get_reference_articles_of_interest: () => [makePaper(2, 1), makePaper(3, 0)],
    });

    const { refresh, hasHighUseReferences } = useReferencesHalo();
    await refresh();

    expect(hasHighUseReferences.value).toBe(false);
  });

  it('stays off on an empty articles-of-interest list', async () => {
    mockTauriCommandDispatch({
      get_reference_articles_of_interest: () => [],
    });

    const { refresh, hasHighUseReferences } = useReferencesHalo();
    await refresh();

    expect(hasHighUseReferences.value).toBe(false);
  });

  it('stays off and does not throw when the IPC fails', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('db locked'));

    const { refresh, hasHighUseReferences } = useReferencesHalo();
    await expect(refresh()).resolves.toBeUndefined();
    expect(hasHighUseReferences.value).toBe(false);
  });
});
