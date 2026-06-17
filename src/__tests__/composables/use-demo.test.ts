import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useDemo } from '@/composables/use-demo';
import type { Router } from 'vue-router';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: vi.fn(() => true),
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(),
}));

vi.mock('@/composables/use-loading-overlay', () => ({
  useLoadingOverlay: () => ({
    withOverlay: vi.fn(async (_msg: string, fn: () => Promise<void>) => fn()),
  }),
}));

// Mock all stores to avoid complex fetch chains
vi.mock('@/stores/articles', () => ({
  useArticlesStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/criteria', () => ({
  useCriteriaStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/tags', () => ({
  useTagsStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/labels', () => ({
  useLabelsStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/audit', () => ({
  useAuditStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock('@/stores/screening', () => ({
  useScreeningStore: () => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));

import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import { ask } from '@tauri-apps/plugin-dialog';

function mockRouter(): Router {
  return { push: vi.fn() } as unknown as Router;
}

describe('useDemo', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts not loading with no error', () => {
    const { demoLoading, demoError } = useDemo(mockRouter());
    expect(demoLoading.value).toBe(false);
    expect(demoError.value).toBeNull();
  });

  it('aborts when not in Tauri (sets error, does not ask)', async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    const { loadDemo, demoError } = useDemo(mockRouter());
    await loadDemo();
    expect(demoError.value).toBe('Demo requires the desktop app.');
    expect(ask).not.toHaveBeenCalled();
  });

  it('aborts when user cancels the confirmation dialog', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(ask).mockResolvedValue(false);
    const { loadDemo } = useDemo(mockRouter());
    await loadDemo();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('loads demo and navigates to root when confirmed', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const router = mockRouter();
    const { loadDemo, demoLoading, demoError } = useDemo(router);
    await loadDemo();

    expect(tauriCommand).toHaveBeenCalledWith(
      'import_project_backup',
      expect.objectContaining({ request: expect.any(Object) })
    );
    expect(router.push).toHaveBeenCalledWith('/');
    expect(demoLoading.value).toBe(false);
    expect(demoError.value).toBeNull();
  });

  it('sets error when import fails', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(tauriCommand).mockRejectedValue(new Error('corrupt backup'));

    const { loadDemo, demoError } = useDemo(mockRouter());
    await loadDemo();
    expect(demoError.value).toBe('corrupt backup');
  });

  it('handles non-Error exceptions', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(tauriCommand).mockRejectedValue('string fail');

    const { loadDemo, demoError } = useDemo(mockRouter());
    await loadDemo();
    expect(demoError.value).toBe('string fail');
  });

  it('guards against double-loading', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    let resolveAsk: (v: boolean) => void;
    vi.mocked(ask).mockReturnValue(
      new Promise<boolean>((r) => {
        resolveAsk = r;
      })
    );

    const { loadDemo, demoLoading } = useDemo(mockRouter());
    const first = loadDemo();
    demoLoading.value = true;
    await loadDemo();
    expect(demoLoading.value).toBe(true);
    resolveAsk!(true);
    await first;
  });
});
