import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ref } from 'vue';
import { createPinia, setActivePinia } from 'pinia';

/* ── Mocks ────────────────────────────────────────────────────────────────── */

/* mockRegenerate is referenced directly inside the vi.mock factory, so it
 * must live in vi.hoisted (the factory runs during the hoisted import of
 * the composable under test, before top-level consts initialize). */
const { mockRegenerate, mockRegenerateWithProgress } = vi.hoisted(() => ({
  mockRegenerate: vi.fn().mockResolvedValue(undefined),
  mockRegenerateWithProgress: vi.fn().mockResolvedValue(undefined),
}));

const mockReadiness = ref<unknown>(null);
const mockMismatch = ref<unknown>(null);
const mockInstall = vi.fn().mockResolvedValue(undefined);
const mockSelectBackend = vi.fn().mockResolvedValue(undefined);
const mockLocalLoad = vi.fn().mockResolvedValue(undefined);
const mockRunSearch = vi.fn().mockResolvedValue(undefined);
const mockCheckReadiness = vi.fn().mockResolvedValue(undefined);

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(() => Promise.resolve(null)),
  isTauri: () => false,
}));

vi.mock('@/composables/use-citation-finder', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/composables/use-citation-finder')>();
  return {
    ...actual,
    getReadiness: vi.fn(() => Promise.resolve(mockReadiness.value)),
    getModelMismatch: vi.fn(() => Promise.resolve(mockMismatch.value)),
    regenerateEmbeddings: mockRegenerate,
    regenerateEmbeddingsWithProgress: mockRegenerateWithProgress,
  };
});

import { useCitationFinderChat } from '@/composables/use-citation-finder-chat';
import { useChatStore } from '@/stores/chat';
import type { CitationFinderProgress, CitationFinderReadiness } from '@/types/citation-finder';
import { shimLocalStorage } from '../helpers/fixtures';

function makeReadiness(overrides: Partial<CitationFinderReadiness> = {}): CitationFinderReadiness {
  return {
    totalArticles: 3,
    embeddedCount: 3,
    coveragePct: 100,
    providerSupportsEmbeddings: true,
    statuses: ['working', 'included'],
    embeddingStatus: 'enabled',
    embeddingModel: 'm',
    embeddingBackend: 'configured_provider',
    localReady: false,
    chatProviderSupportsEmbeddings: true,
    ...overrides,
  };
}

/** Build the composable against the real chat store + fake collaborators. */
function setup(overrides: { readiness?: CitationFinderReadiness } = {}) {
  const chatStore = useChatStore();
  mockReadiness.value = overrides.readiness ?? makeReadiness();
  const composable = useCitationFinderChat({
    chatStore,
    isLlmConfigured: ref(true),
    localEmbeddings: {
      installing: ref(false),
      load: mockLocalLoad,
      install: mockInstall,
      selectBackend: mockSelectBackend,
    } as unknown as Parameters<typeof useCitationFinderChat>[0]['localEmbeddings'],
    checkReadiness: mockCheckReadiness,
    runSearch: mockRunSearch,
  });
  return { chatStore, composable };
}

beforeEach(() => {
  Object.defineProperty(window, 'localStorage', {
    value: shimLocalStorage(),
    configurable: true,
  });
  setActivePinia(createPinia());
  mockReadiness.value = makeReadiness();
  mockMismatch.value = null;
  mockRegenerate.mockClear();
  mockRegenerateWithProgress.mockClear();
  mockInstall.mockClear();
  mockSelectBackend.mockClear();
  mockLocalLoad.mockClear();
  mockRunSearch.mockClear();
  mockCheckReadiness.mockClear();
});

/* ── Toggle state matrix ─────────────────────────────────────────────────── */

describe('use-citation-finder-chat - toggle state', () => {
  it('hides the toggle when the readiness IPC fails', async () => {
    const { composable } = setup();
    const { getReadiness } = await import('@/composables/use-citation-finder');
    vi.mocked(getReadiness).mockRejectedValueOnce(new Error('no provider'));
    await composable.checkCitationFinderReadiness();
    expect(composable.citationToggleState.value).toBe('hidden');
  });

  it('maps the enabled triple-state through to a clickable toggle', async () => {
    const { composable } = setup();
    await composable.checkCitationFinderReadiness();
    expect(composable.citationToggleState.value).toBe('enabled');
    expect(composable.citationToggleTitle.value).toContain('semantic search over your library');
  });

  it('reports the local tooltip branch when bango_local is not installed', async () => {
    const { composable } = setup({
      readiness: makeReadiness({
        embeddingBackend: 'bango_local',
        localReady: false,
        embeddingStatus: 'disabled',
      }),
    });
    await composable.checkCitationFinderReadiness();
    expect(composable.citationToggleState.value).toBe('unknown');
    expect(composable.citationToggleTitle.value).toContain('not downloaded yet');
  });
});

/* ── Submit pipeline ─────────────────────────────────────────────────────── */

describe('use-citation-finder-chat - submit pipeline', () => {
  it('dispatches the search with the live status filter', async () => {
    const { chatStore, composable } = setup();
    await composable.checkCitationFinderReadiness();
    chatStore.citationDraft = 'Sugar taxes work.';
    await composable.handleCitationSend();
    expect(mockRunSearch).toHaveBeenCalledWith('Sugar taxes work.');
    // Working + Included default on, Rejected off.
    expect(composable.citationStatusFilter.value).toEqual(['working', 'included']);
  });

  it('opens the local prompt instead of searching when local is not installed', async () => {
    const { chatStore, composable } = setup({
      readiness: makeReadiness({
        embeddingBackend: 'bango_local',
        localReady: false,
        embeddingStatus: 'disabled',
      }),
    });
    await composable.checkCitationFinderReadiness();
    chatStore.citationDraft = 'On-device prose.';
    await composable.handleCitationSend();
    expect(composable.localPromptOpen.value).toBe(true);
    expect(mockRunSearch).not.toHaveBeenCalled();
    expect(mockLocalLoad).toHaveBeenCalled();
    // Cancel restores the draft.
    composable.cancelLocalPrompt();
    expect(composable.localPromptOpen.value).toBe(false);
    expect(chatStore.citationDraft).toBe('On-device prose.');
  });

  it('Use Configured Provider switches the backend and continues the held search', async () => {
    const { composable } = setup();
    await composable.confirmLocalUseCloud();
    expect(mockSelectBackend).toHaveBeenCalledWith('configured_provider');
    await vi.waitFor(() => expect(mockRunSearch).toHaveBeenCalled());
  });

  it('Download and Continue installs then continues', async () => {
    const { composable } = setup();
    await composable.confirmLocalDownload();
    expect(mockInstall).toHaveBeenCalled();
    await vi.waitFor(() => expect(mockRunSearch).toHaveBeenCalled());
  });

  it('pops the mismatch dialog once per stored model, then dispatches on continue', async () => {
    const { chatStore, composable } = setup();
    await composable.checkCitationFinderReadiness();
    mockMismatch.value = { currentModel: 'new', storedModel: 'old', storedRowCount: 5 };
    chatStore.citationDraft = 'Held.';
    await composable.handleCitationSend();
    expect(composable.mismatchDialog.value).not.toBeNull();
    expect(mockRunSearch).not.toHaveBeenCalled();
    await composable.continueMismatchSearch();
    expect(mockRunSearch).toHaveBeenCalledWith('Held.');
    expect(chatStore.mismatchDismissedFor).toBe('old');
    // Second submit with the same stored model does not re-fire.
    chatStore.citationDraft = 'Again.';
    await composable.handleCitationSend();
    expect(composable.mismatchDialog.value).toBeNull();
    expect(mockRunSearch).toHaveBeenCalledWith('Again.');
  });

  it('Regenerate scope-flags the statuses and restores the held prose', async () => {
    const { chatStore, composable } = setup();
    await composable.checkCitationFinderReadiness();
    mockMismatch.value = { currentModel: 'new', storedModel: 'old', storedRowCount: 5 };
    chatStore.citationDraft = 'Held.';
    await composable.handleCitationSend();
    await composable.confirmMismatchRegenerate();
    expect(mockRegenerateWithProgress).toHaveBeenCalledWith(
      'working,included',
      expect.any(Function)
    );
    expect(chatStore.citationDraft).toBe('Held.');
    expect(mockRunSearch).not.toHaveBeenCalled();
  });

  it('regenerate_reports_live_progress_until_completion', async () => {
    const { chatStore, composable } = setup();
    await composable.checkCitationFinderReadiness();
    mockMismatch.value = { currentModel: 'new', storedModel: 'old', storedRowCount: 5 };
    chatStore.citationDraft = 'Held.';
    await composable.handleCitationSend();

    let finish: () => void = () => {};
    mockRegenerateWithProgress.mockImplementationOnce(
      async (_scope: string | null, onProgress: (p: CitationFinderProgress) => void) => {
        onProgress({
          phase: 'preparing_embeddings',
          done: 3,
          total: 10,
          overallPercent: 27,
          message: 'Regenerating embeddings… 3/10 articles',
          isRunning: true,
          isCancelled: false,
        });
        await new Promise<void>((resolve) => {
          finish = resolve;
        });
      }
    );

    const running = composable.confirmMismatchRegenerate();
    await vi.waitFor(() =>
      expect(composable.regeneratingProgress.value?.message).toBe(
        'Regenerating embeddings… 3/10 articles'
      )
    );
    expect(composable.regenerating.value).toBe(true);

    finish();
    await running;
    expect(composable.regeneratingProgress.value).toBeNull();
  });

  it('Cancel on the mismatch drops the prose without a dismissal', async () => {
    const { chatStore, composable } = setup();
    await composable.checkCitationFinderReadiness();
    mockMismatch.value = { currentModel: 'new', storedModel: 'old', storedRowCount: 5 };
    chatStore.citationDraft = 'Held.';
    await composable.handleCitationSend();
    composable.cancelMismatchDialog();
    expect(composable.mismatchDialog.value).toBeNull();
    expect(mockRunSearch).not.toHaveBeenCalled();
    expect(chatStore.mismatchDismissedFor).toBeNull();
  });

  it('toggling citation mode swaps the store source', async () => {
    const { composable } = setup();
    composable.onToggleCitationFinder();
    expect(composable.isCitationMode.value).toBe(true);
    composable.onToggleCitationFinder();
    expect(composable.isCitationMode.value).toBe(false);
  });
});

/* ── Articles-to-Search persistence ──────────────────────────────────────── */

describe('use-citation-finder-chat - status selection', () => {
  it('status_change_persists_and_rechecks_readiness', () => {
    const { chatStore, composable } = setup();
    expect(composable.citationStatusFilter.value).toEqual(['working', 'included']);

    composable.setCitationStatuses({ working: true, included: false, rejected: false });

    expect(chatStore.citationStatuses).toEqual({
      working: true,
      included: false,
      rejected: false,
    });
    expect(composable.citationStatusFilter.value).toEqual(['working']);
    expect(mockCheckReadiness).toHaveBeenCalledTimes(1);
  });
});
