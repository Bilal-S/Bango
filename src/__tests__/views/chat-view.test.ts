import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises, enableAutoUnmount, type VueWrapper } from '@vue/test-utils';
import { ref } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import { makeArticle } from '../helpers/fixtures';
import type {
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationMatch,
  CitationResult,
  EmbeddingModelMismatch,
} from '@/types/citation-finder';
import type { Article } from '@/types';
import type { WikiStatus } from '@/types/wiki';

/* ── Module-level mock state (summary-view.test.ts pattern) ─────────────────
 * Refs are read lazily by the mock factories, so tests mutate them and the
 * mounted view re-renders without re-mounting. */

/* vi.fn mocks referenced directly inside vi.mock factory bodies must live in
 * vi.hoisted: the factories run during the hoisted import of chat-view (which
 * imports the chat store, which imports use-citation-finder), before ordinary
 * top-level consts initialize. */
const { mockRegenerate, mockSelectBackend, mockInstall, mockLocalLoad, mockScrollAnchor } =
  vi.hoisted(() => ({
    mockRegenerate: vi.fn().mockResolvedValue(undefined),
    mockSelectBackend: vi.fn().mockResolvedValue(undefined),
    mockInstall: vi.fn().mockResolvedValue(undefined),
    mockLocalLoad: vi.fn().mockResolvedValue(undefined),
    mockScrollAnchor: vi.fn(),
  }));

const mockIsConfigured = ref(true);
const mockLlmInitialized = ref(true);
const mockLlmConfig = ref<Record<string, unknown> | null>({ provider: 'openai' });

const mockArticles = ref<Article[]>([]);
function makeWikiStatus(overrides: Partial<WikiStatus> = {}): WikiStatus {
  return {
    configured: true,
    rootDir: '/tmp/wiki',
    isCustom: false,
    defaultPath: '/tmp/wiki',
    rawCount: 2,
    pageCount: 2,
    needsRefresh: false,
    includedArticleCount: 5,
    initialized: true,
    ...overrides,
  };
}
const mockWikiStatus = ref<WikiStatus>(makeWikiStatus());

/** Readiness returned by `getReadiness`. Default: enabled cloud backend. */
const mockReadiness = ref<CitationFinderReadiness>(makeReadiness());
/** Mismatch returned by `getModelMismatch`. `null` = no mismatch. */
const mockMismatch = ref<EmbeddingModelMismatch | null>(null);

/** Captured `findCitations` invocation (the store passes event callbacks). */
let findCall: {
  text: string;
  mode: string;
  statusFilter: string[];
  onProgress?: (p: CitationFinderProgress) => void;
  onDone?: (results: CitationResult[]) => void;
  onError?: (msg: string) => void;
} | null = null;

function makeReadiness(overrides: Partial<CitationFinderReadiness> = {}): CitationFinderReadiness {
  return {
    totalArticles: 3,
    embeddedCount: 3,
    coveragePct: 100,
    providerSupportsEmbeddings: true,
    statuses: ['working', 'included'],
    embeddingStatus: 'enabled',
    embeddingModel: 'text-embedding-3-small',
    embeddingBackend: 'configured_provider',
    localReady: false,
    chatProviderSupportsEmbeddings: true,
    ...overrides,
  };
}

function makeMatch(overrides: Partial<CitationMatch> = {}): CitationMatch {
  return {
    articleId: 'a1',
    title: 'Sugar tax reduces obesity',
    authors: ['Smith, J.'],
    publicationYear: 2021,
    journal: 'Journal of Taxes',
    doi: '10.1000/demo',
    matchedPassage: 'The levy reduced consumption by 20 percent.',
    sectionOrigin: 'Abstract',
    classification: 'validating',
    relevanceExplanation: 'Directly supports the claim.',
    misrepresentsSource: false,
    highlightedSentences: ['The levy reduced consumption by 20 percent.'],
    confidence: 0.92,
    ...overrides,
  };
}

function makeProgress(overrides: Partial<CitationFinderProgress> = {}): CitationFinderProgress {
  return {
    phase: 'searching',
    done: 0,
    total: 0,
    overallPercent: 10,
    message: 'Searching literature...',
    isRunning: true,
    isCancelled: false,
    ...overrides,
  };
}

function resetMockState(): void {
  mockIsConfigured.value = true;
  mockLlmInitialized.value = true;
  mockLlmConfig.value = { provider: 'openai' };
  mockArticles.value = [];
  mockWikiStatus.value = makeWikiStatus();
  mockReadiness.value = makeReadiness();
  mockMismatch.value = null;
  findCall = null;
  mockRegenerate.mockClear();
  mockSelectBackend.mockClear();
  mockInstall.mockClear();
  mockLocalLoad.mockClear();
  mockScrollAnchor.mockClear();
}

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn((cmd: string) => {
    switch (cmd) {
      case 'get_articles':
        return Promise.resolve(mockArticles.value);
      case 'wiki_get_status':
        return Promise.resolve(mockWikiStatus.value);
      case 'send_chat_message':
        return Promise.resolve('Assistant answer.');
      case 'wiki_chat':
        return Promise.resolve('Wiki answer.');
      case 'find_citations':
        return Promise.resolve(makeProgress());
      default:
        return Promise.resolve(null);
    }
  }),
  isTauri: () => false,
}));

vi.mock('@/composables/use-citation-finder', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/composables/use-citation-finder')>();
  return {
    ...actual,
    getReadiness: vi.fn(() => Promise.resolve(mockReadiness.value)),
    getModelMismatch: vi.fn(() => Promise.resolve(mockMismatch.value)),
    regenerateEmbeddings: mockRegenerate,
    regenerateEmbeddingsWithProgress: mockRegenerate,
    stopCitationListeners: vi.fn(),
    cancelSearch: vi.fn().mockResolvedValue(undefined),
    findCitations: vi.fn(
      (args: {
        text: string;
        mode: string;
        statusFilter: string[];
        onProgress?: (p: CitationFinderProgress) => void;
        onDone?: (results: CitationResult[]) => void;
        onError?: (msg: string) => void;
      }) => {
        findCall = args;
        return Promise.resolve(makeProgress());
      }
    ),
  };
});

vi.mock('@/composables/use-wiki', () => ({
  useWiki: () => ({
    listPages: vi.fn(() =>
      Promise.resolve([
        { slug: 'synthesis-1', title: 'Synthesis 1' },
        { slug: 'synthesis-2', title: 'Synthesis 2' },
      ])
    ),
    checkForUpdates: vi.fn(() => Promise.resolve({ rebuilt: false })),
  }),
}));

vi.mock('@/composables/use-article-search', () => ({
  useArticleSearch: () => ({
    selectedArticle: ref<Article | null>(null),
    auditTrail: ref([]),
    selectArticle: vi.fn(),
    refreshArticle: vi.fn(),
    updateNotes: vi.fn(),
    updateTags: vi.fn(),
    updateLabels: vi.fn(),
    updateCriteria: vi.fn(),
    updateMetadata: vi.fn(),
    moveArticle: vi.fn(),
    deleteArticle: vi.fn(),
    clearAiReasoning: vi.fn(),
    attachFullText: vi.fn(),
    deleteFullTextAttachment: vi.fn(),
  }),
}));

vi.mock('@/composables/use-screening', () => ({
  useScreening: () => ({ screenArticle: vi.fn() }),
}));

vi.mock('@/composables/use-full-text-attachment', () => ({
  useFullTextAttachment: () => ({ handleAttachFullText: vi.fn() }),
}));

vi.mock('@/composables/use-article-delete', () => ({
  useArticleDelete: () => ({ handleDeleteArticle: vi.fn() }),
}));

vi.mock('@/composables/use-clear-ai-reasoning', () => ({
  useClearAiReasoning: () => ({ handleClearAiReasoning: vi.fn() }),
}));

vi.mock('@/composables/use-llm-configured', () => ({
  useLlmConfigured: () => mockIsConfigured,
}));

vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    get initialized() {
      return mockLlmInitialized.value;
    },
    get config() {
      return mockLlmConfig.value;
    },
    get isConfigured() {
      return mockIsConfigured.value;
    },
  }),
}));

vi.mock('@/composables/use-local-embeddings', () => ({
  useLocalEmbeddings: () => ({
    backend: ref<'configured_provider' | 'bango_local'>('configured_provider'),
    status: ref<unknown>(null),
    progress: ref<unknown>(null),
    verifyResult: ref<unknown>(null),
    error: ref<string | null>(null),
    loading: ref(false),
    installing: ref(false),
    verifying: ref(false),
    removing: ref(false),
    load: mockLocalLoad,
    loadBackend: vi.fn().mockResolvedValue(undefined),
    loadStatus: vi.fn().mockResolvedValue(undefined),
    selectBackend: mockSelectBackend,
    install: mockInstall,
    cancelInstall: vi.fn().mockResolvedValue(undefined),
    verify: vi.fn(),
    remove: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('@/utils/chat-scroll', () => ({
  scrollAnchorToContainerTop: mockScrollAnchor,
}));

/* The real Tauri event API throws outside the webview; transitively imported
 * modules (e.g. `use-ai-summary` via the detail panel) register listeners at
 * import time (article-detail-panel test pattern). */
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

import ChatView from '@/views/chat-view.vue';
import { useChatStore } from '@/stores/chat';
import { tauriCommand } from '@/composables/use-tauri-command';

const mockTauriCommand = vi.mocked(tauriCommand);

function mountChatView(): VueWrapper {
  const pinia = createPinia();
  setActivePinia(pinia);
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  });
  return mount(ChatView, {
    global: {
      plugins: [pinia, router],
      /* Heavy slide-over panels are stubbed: they render conditionally and
       * are not under characterization here. */
      stubs: { ArticleDetailPanel: true, WikiPageViewer: true },
    },
  });
}

/** Mount + settle the onMounted IPC burst (articles, wiki status, readiness). */
async function mountAndSettle(): Promise<VueWrapper> {
  const wrapper = mountChatView();
  await flushPromises();
  return wrapper;
}

/** Enter citation-finder mode via the chat-bar toggle. */
async function enterCitationMode(wrapper: VueWrapper): Promise<void> {
  await wrapper.find('.citation-toggle').trigger('click');
  await flushPromises();
}

/** Type prose + click Find Citations. */
async function submitCitationSearch(wrapper: VueWrapper, text: string): Promise<void> {
  const textarea = wrapper.find('.citation-input-area__textarea');
  await textarea.setValue(text);
  await wrapper.find('.citation-input-area__find-btn').trigger('click');
  await flushPromises();
}

/** Deliver results into the transcript the way the `citation:done` listener does. */
async function deliverResults(results: CitationResult[]): Promise<void> {
  findCall!.onDone!(results);
  await flushPromises();
}

describe('chat-view.vue - LLM gate', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('shows the checking spinner while the LLM store bootstraps', async () => {
    mockLlmInitialized.value = false;
    const wrapper = await mountAndSettle();
    expect(wrapper.text()).toContain('Checking LLM configuration...');
  });

  it('shows the unconfigured card with no citation toggle when no LLM is configured', async () => {
    mockIsConfigured.value = false;
    const wrapper = await mountAndSettle();
    expect(wrapper.text()).toContain('LLM Provider Not Configured');
    expect(wrapper.find('.citation-toggle').exists()).toBe(false);
  });

  it('shows the welcome cards when configured with an empty transcript', async () => {
    const wrapper = await mountAndSettle();
    const titles = wrapper.findAll('.chat-welcome-card__title').map((t) => t.text());
    expect(titles).toEqual(['Academic Research Chat', 'Wiki Chat', 'Citation Finder']);
  });
});

describe('chat-view.vue - article context selector', () => {
  beforeEach(() => {
    resetMockState();
    mockArticles.value = [
      makeArticle({ id: 'a1', title: 'Alpha Study' }),
      makeArticle({ id: 'a2', title: 'Beta Study' }),
      makeArticle({ id: 'a3', status: 'duplicate', duplicateOf: 'a1' }),
    ];
  });

  it('opens the selector from the plus button, lists non-duplicate articles, and Done closes it', async () => {
    const wrapper = await mountAndSettle();
    await wrapper.find('button[title="Add article context"]').trigger('click');
    await flushPromises();
    /* The selector modal is teleported to document.body; its checkboxes are
     * the only ones teleported there. Duplicates are filtered out of the list. */
    const boxes = document.body.querySelectorAll('input[type="checkbox"]');
    expect(boxes.length).toBe(2);
    const done = document.body.querySelector('button.bg-indigo-600');
    expect(done).not.toBeNull();
    (done as HTMLButtonElement).click();
    await flushPromises();
    expect(document.body.querySelector('.max-w-2xl')).toBeNull();
  });

  it('selects an article from the selector and renders a context pill', async () => {
    const wrapper = await mountAndSettle();
    await wrapper.find('button[title="Add article context"]').trigger('click');
    await flushPromises();
    const firstBox = document.body.querySelector('input[type="checkbox"]') as HTMLInputElement;
    firstBox.click();
    await flushPromises();
    const done = document.body.querySelector('button.bg-indigo-600');
    (done as HTMLButtonElement).click();
    await flushPromises();
    expect(wrapper.text()).toContain('Selected Context (1)');
  });
});

describe('chat-view.vue - article + wiki chat send', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('sends an article-mode message through send_chat_message and renders both bubbles', async () => {
    const wrapper = await mountAndSettle();
    const input = wrapper.find('input[type="text"]');
    await input.setValue('What does the evidence say?');
    await input.trigger('keydown.enter');
    await flushPromises();
    expect(mockTauriCommand).toHaveBeenCalledWith('send_chat_message', {
      articleIds: [],
      history: [],
      newMessage: 'What does the evidence say?',
    });
    expect(wrapper.text()).toContain('What does the evidence say?');
    expect(wrapper.text()).toContain('Assistant answer.');
  });

  it('switches to wiki mode via the toggle and sends through wiki_chat', async () => {
    const wrapper = await mountAndSettle();
    await wrapper.find('.wiki-toggle').trigger('click');
    await flushPromises();
    const input = wrapper.find('input[type="text"]');
    expect(input.attributes('placeholder')).toBe('Ask a question about your wiki...');
    await input.setValue('What is known?');
    await input.trigger('keydown.enter');
    await flushPromises();
    expect(mockTauriCommand).toHaveBeenCalledWith('wiki_chat', {
      question: 'What is known?',
      history: [],
    });
    expect(wrapper.text()).toContain('Wiki answer.');
  });
});

describe('chat-view.vue - citation finder toggle states', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('hides the toggle when readiness fails to load', async () => {
    const { getReadiness } = await import('@/composables/use-citation-finder');
    vi.mocked(getReadiness).mockRejectedValueOnce(new Error('no provider'));
    const wrapper = await mountAndSettle();
    expect(wrapper.find('.citation-toggle').exists()).toBe(false);
  });

  it('renders the toggle disabled with the provider hint when embeddings are disabled', async () => {
    mockReadiness.value = makeReadiness({ embeddingStatus: 'disabled' });
    const wrapper = await mountAndSettle();
    const toggle = wrapper.find('.citation-toggle');
    expect(toggle.attributes('disabled')).toBeDefined();
    expect(toggle.attributes('title')).toContain('does not support embeddings');
  });

  it('keeps the toggle clickable when bango_local is selected but not installed', async () => {
    mockReadiness.value = makeReadiness({
      embeddingBackend: 'bango_local',
      localReady: false,
      embeddingStatus: 'disabled',
    });
    const wrapper = await mountAndSettle();
    const toggle = wrapper.find('.citation-toggle');
    expect(toggle.attributes('disabled')).toBeUndefined();
    expect(toggle.attributes('title')).toContain('not downloaded yet');
  });
});

describe('chat-view.vue - citation finder submit pipeline', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('dispatches the search with the live status filter and swaps Find for progress', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    /* Uncheck Included so only Working remains in the filter. */
    const includedLabel = wrapper
      .findAll('label.citation-input-area__checkbox')
      .find((l) => l.text().includes('Included'));
    await includedLabel!.find('input').setValue(false);
    await submitCitationSearch(wrapper, 'Sugar taxes reduce obesity.');
    expect(findCall).not.toBeNull();
    expect(findCall!.text).toBe('Sugar taxes reduce obesity.');
    expect(findCall!.statusFilter).toEqual(['working']);
    expect(findCall!.mode).toBe('whole_block');
    /* Fire the first progress event the way `citation:progress` does. */
    findCall!.onProgress!(makeProgress());
    await flushPromises();
    /* The progress UI replaces the Find button. */
    expect(wrapper.find('.citation-input-area__find-btn').exists()).toBe(false);
    expect(wrapper.find('.citation-progress--inline').exists()).toBe(true);
  });

  it('shows the coverage notice when coverage is below 100 percent', async () => {
    mockReadiness.value = makeReadiness({ coveragePct: 50, embeddedCount: 2 });
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    expect(wrapper.text()).toContain('First run will prepare embeddings for');
  });

  it('pops the model-mismatch dialog and Continue anyway dispatches the held search', async () => {
    mockMismatch.value = {
      currentModel: 'new-model',
      storedModel: 'old-model',
      storedRowCount: 12,
    };
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Held prose.');
    /* The mismatch dialog is teleported to document.body. */
    const dialog = document.body.querySelector('.mismatch-dialog');
    expect(dialog).not.toBeNull();
    expect(dialog!.textContent).toContain('old-model');
    /* No search dispatched while the dialog holds the prose. */
    expect(findCall).toBeNull();
    (document.body.querySelector('.mismatch-dialog__btn--secondary') as HTMLButtonElement).click();
    await flushPromises();
    expect(document.body.querySelector('.mismatch-dialog')).toBeNull();
    expect(findCall!.text).toBe('Held prose.');
  });

  it('Regenerate scope-flags the regeneration, restores the draft, and records the dismissal', async () => {
    mockMismatch.value = {
      currentModel: 'new-model',
      storedModel: 'old-model',
      storedRowCount: 12,
    };
    const wrapper = await mountAndSettle();
    const store = useChatStore();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Held prose.');
    (document.body.querySelector('.mismatch-dialog__btn--primary') as HTMLButtonElement).click();
    await flushPromises();
    expect(mockRegenerate).toHaveBeenCalledWith('working,included', expect.any(Function));
    expect(store.citationDraft).toBe('Held prose.');
    expect(store.mismatchDismissedFor).toBe('old-model');
    expect(document.body.querySelector('.mismatch-dialog')).toBeNull();
    /* The dismissal suppresses the dialog on the next submit. */
    await submitCitationSearch(wrapper, 'Held prose.');
    expect(document.body.querySelector('.mismatch-dialog')).toBeNull();
    expect(findCall!.text).toBe('Held prose.');
  });

  it('Cancel on the mismatch dialog drops the search without a dismissal', async () => {
    mockMismatch.value = {
      currentModel: 'new-model',
      storedModel: 'old-model',
      storedRowCount: 12,
    };
    const wrapper = await mountAndSettle();
    const store = useChatStore();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Held prose.');
    (document.body.querySelector('.mismatch-dialog__btn--ghost') as HTMLButtonElement).click();
    await flushPromises();
    expect(document.body.querySelector('.mismatch-dialog')).toBeNull();
    expect(findCall).toBeNull();
    expect(store.mismatchDismissedFor).toBeNull();
    expect(store.citationDraft).toBe('');
  });
});

describe('chat-view.vue - contextual Bango Local prompt', () => {
  beforeEach(() => {
    resetMockState();
    mockReadiness.value = makeReadiness({
      embeddingBackend: 'bango_local',
      localReady: false,
      embeddingStatus: 'disabled',
    });
  });

  it('opens the local-embeddings dialog instead of searching and Cancel restores the prose', async () => {
    const wrapper = await mountAndSettle();
    const store = useChatStore();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'On-device prose.');
    const dialog = wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' });
    expect(dialog.exists()).toBe(true);
    expect(findCall).toBeNull();
    expect(mockLocalLoad).toHaveBeenCalled();
    /* Cancel restores the held prose to the textarea. */
    const cancel = dialog.findAll('button').find((b) => b.text() === 'Cancel');
    await cancel!.trigger('click');
    await flushPromises();
    expect(store.citationDraft).toBe('On-device prose.');
    expect(wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' }).exists()).toBe(false);
  });

  it('Use Configured Provider switches the backend, refreshes readiness, and continues the search', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Switch me.');
    const dialog = wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' });
    const useCloud = dialog.findAll('button').find((b) => b.text() === 'Use Configured Provider');
    await useCloud!.trigger('click');
    await flushPromises();
    expect(mockSelectBackend).toHaveBeenCalledWith('configured_provider');
    expect(findCall!.text).toBe('Switch me.');
    expect(wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' }).exists()).toBe(false);
  });

  it('Download and Continue installs the components, then continues the held search', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Download first.');
    const dialog = wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' });
    /* The button nests a material icon span, so match on a substring. */
    const download = dialog
      .findAll('button')
      .find((b) => b.text().includes('Download and Continue'));
    await download!.trigger('click');
    await flushPromises();
    expect(mockInstall).toHaveBeenCalled();
    expect(findCall!.text).toBe('Download first.');
    expect(wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' }).exists()).toBe(false);
  });

  it('hides the Use Configured Provider option when the chat provider has no embedding API', async () => {
    mockReadiness.value = makeReadiness({
      embeddingBackend: 'bango_local',
      localReady: false,
      embeddingStatus: 'disabled',
      chatProviderSupportsEmbeddings: false,
    });
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Dead end.');
    const dialog = wrapper.findComponent({ name: 'CitationLocalEmbeddingsDialog' });
    const useCloud = dialog.findAll('button').find((b) => b.text() === 'Use Configured Provider');
    expect(useCloud).toBeUndefined();
    expect(dialog.text()).toContain('no embedding API');
  });
});

describe('chat-view.vue - citation results transcript', () => {
  beforeEach(() => {
    resetMockState();
  });

  it('renders per-statement claim groups and toggles claim collapse', async () => {
    const wrapper = await mountAndSettle();
    const store = useChatStore();
    store.setCitationFinderMode('per_statement');
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Two claims.');
    await deliverResults([
      { claim: 'Claim one', matches: [makeMatch()] },
      { claim: 'Claim two', matches: [makeMatch({ articleId: 'a2', title: 'Second Paper' })] },
    ]);
    const groups = wrapper.findAll('.citation-bubble__group');
    expect(groups.length).toBe(2);
    expect(wrapper.text()).toContain('Claim one');
    expect(wrapper.text()).toContain('Claim two');
    expect(wrapper.findAll('.citation-card').length).toBe(2);
    /* Collapse the first claim: its cards hide (v-show) while the other
     * claim's cards stay visible. */
    const firstGroup = groups[0]!;
    const secondGroup = groups[1]!;
    await firstGroup.find('.citation-bubble__claim-toggle').trigger('click');
    await flushPromises();
    const firstCard = firstGroup.find('.citation-card');
    expect((firstCard.element as HTMLElement).style.display).toBe('none');
    const secondCard = secondGroup.find('.citation-card');
    expect((secondCard.element as HTMLElement).style.display).toBe('');
  });

  it('renders whole-block matches as a flat card list with a summary line', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Whole block.');
    await deliverResults([
      {
        claim: null,
        matches: [makeMatch(), makeMatch({ articleId: 'a2', title: 'Second Paper' })],
      },
    ]);
    /* Whole-block mode renders no claim-group headings. */
    expect(wrapper.findAll('.citation-bubble__group').length).toBe(0);
    expect(wrapper.findAll('.citation-card').length).toBe(2);
    expect(wrapper.text()).toContain('Found 2 citation(s).');
    expect(wrapper.text()).toContain('Sugar tax reduces obesity');
    expect(wrapper.text()).toContain('Second Paper');
  });

  it('renders the empty-filter summary when every group is empty', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Nothing matches.');
    await deliverResults([{ claim: null, matches: [] }]);
    expect(wrapper.text()).toContain('No articles match the selected filters.');
  });

  it('pins the user claim entry to the top of the scroll area when results arrive', async () => {
    const wrapper = await mountAndSettle();
    await enterCitationMode(wrapper);
    await submitCitationSearch(wrapper, 'Anchor me.');
    await deliverResults([{ claim: null, matches: [makeMatch()] }]);
    expect(mockScrollAnchor).toHaveBeenCalled();
  });
});

/* Unmount every wrapper first, then clear the Teleported modal/dialog nodes:
 * wiping document.body while a Teleport is still mounted breaks Vue's
 * patcher with null-host insertBefore errors in later tests. */
enableAutoUnmount(() => {
  document.body.innerHTML = '';
});
