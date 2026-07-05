import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';
import type { Article, AuditEntry } from '@/types';

// Mock tauri invoke so confirmTranslation's `invoke('enqueue_article_translation', ...)` is captured.
const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Capture the `translation:complete` event listener registration so we can
// fire events in test (TC-09 event-driven refresh). Must use `vi.hoisted` so
// the `vi.mock` factory (hoisted to the top of the file) can access them.
type EventCallback = (event: { payload: unknown }) => void;
const { eventListeners, mockUnlisten } = vi.hoisted(() => {
  const eventListeners = new Map<string, EventCallback>();
  const mockUnlisten = vi.fn();
  return { eventListeners, mockUnlisten };
});
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: EventCallback) => {
    eventListeners.set(event, cb);
    return Promise.resolve(mockUnlisten);
  },
}));

// Stub the LLM config store so the component doesn't try to fetch. Includes
// the `isConfigured` getter the panel delegates to (mirrors the real store).
vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    initialized: true,
    fetchIfNeeded: vi.fn(),
    config: { apiKeyEncrypted: 'fake-key' },
    isConfigured: true,
  }),
}));

import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import { _resetTranslationStateForTests } from '@/composables/use-translation';

/** Minimal Article shape for the translation test suite. */
function makeArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'Titre français',
    abstractText: 'Résumé français détaillé.',
    authors: ['Auteur Un'],
    publicationYear: 2024,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: [],
    url: null,
    language: 'French',
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    eissn: null,
    journalIndexId: null,
    referenceType: 'JOUR',
    date: null,
    authorAddress: null,
    affiliation: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    tags: [],
    labels: [],
    manualOverride: false,
    importSource: null,
    importedAt: '',
    changedAt: '',
    screenedAt: null,
    dataLength: null,
    tokenEstimate: null,
    actualTokens: null,
    fullText: null,
    fullTextAiSummary: null,
    numCited: null,
    numReferences: null,
    hasCitationDetails: false,
    hasReferenceDetails: false,
    hasFullText: false,
    fullTextFileName: null,
    hasFiguresOrTables: false,
    isTranslated: false,
    translationStatus: 'none',
    translationError: null,
    translatedAt: null,
    ...overrides,
  } as Article;
}

const emptyAudit: AuditEntry[] = [];

const baseProps = {
  article: makeArticle(),
  auditTrail: emptyAudit,
  hasPrevious: false,
  hasNext: false,
  hasReturnTarget: false,
  articlePosition: 1,
  articleTotal: 1,
};

function mountPanel(overrides: Record<string, unknown> = {}) {
  setActivePinia(createPinia());
  _resetTranslationStateForTests();
  mockInvoke.mockReset();
  eventListeners.clear();

  return mount(ArticleDetailPanel, {
    props: { ...baseProps, ...overrides },
    global: {
      plugins: [createPinia()],
      // Stub heavy sub-components so only the dialog + header are rendered.
      stubs: {
        AuditTimeline: true,
        AiDecisionCard: true,
        ArticleMetadata: true,
        MatchedCriteria: true,
        AbstractSummaryView: true,
        TagsSection: true,
        LabelsSection: true,
        ArticleNotes: true,
        ArticleReferences: true,
        FullTextReader: true,
      },
    },
  });
}

describe('article-detail-panel.vue - translation (language-plan-v2)', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    _resetTranslationStateForTests();
    mockInvoke.mockReset();
    eventListeners.clear();
  });

  it('click_translate_enqueues_job_and_shows_toast', async () => {
    // TC-09: clicking "Translate to English" in the confirmation dialog
    // invokes the `enqueue_article_translation` Tauri command with the
    // article id and trigger source "manual".
    mockInvoke.mockResolvedValue(true);

    const wrapper = mountPanel({
      article: makeArticle({ language: 'French', isTranslated: false }),
    });

    // The DetailHeader should emit @request-translate; since it's not stubbed,
    // look for the translate button. But the fastest path to opening the dialog
    // is to emit the event directly (the header's translate button is tested
    // in detail-header-translation.test.ts).
    const detailHeader = wrapper.findComponent({ name: 'DetailHeader' });
    expect(detailHeader.exists()).toBe(true);

    // Fire the request-translate event from the header.
    detailHeader.vm.$emit('request-translate');

    await flushPromises();

    // The confirmation dialog is teleported to document.body (escapes the
    // detail panel's transform), so query the document rather than the
    // component wrapper.
    const dialogInBody = document.body.querySelector('.dialog--danger');
    expect(dialogInBody).not.toBeNull();
    expect(dialogInBody!.textContent).toContain('Translate Article');
    expect(dialogInBody!.textContent).toContain('Titre français');

    // Click the "Translate to English" button.
    const confirmBtn = document.body.querySelector('.btn--danger') as HTMLButtonElement;
    expect(confirmBtn).not.toBeNull();
    confirmBtn.click();
    await flushPromises();

    // Must have invoked the Tauri enqueue command.
    expect(mockInvoke).toHaveBeenCalledWith('enqueue_article_translation', {
      articleId: 'a1',
      triggerSource: 'manual',
    });

    // The dialog must be closed after confirmation.
    expect(document.body.querySelector('.dialog--danger')).toBeNull();
  });

  it('refreshes_article_on_enqueue_before_completion', async () => {
    // Regression: when the user confirms translation, the article must be
    // refreshed immediately (so the header badge flips to the "Translation
    // Queued" spinner chip) rather than waiting for the `translation:complete`
    // event, which can take minutes for full-text jobs.
    mockInvoke.mockResolvedValue(true);

    const wrapper = mountPanel({
      article: makeArticle({ language: 'French', isTranslated: false }),
    });

    // Open and confirm translation. The dialog is teleported to document.body.
    const detailHeader = wrapper.findComponent({ name: 'DetailHeader' });
    detailHeader.vm.$emit('request-translate');
    await flushPromises();
    const confirmBtn = document.body.querySelector('.btn--danger') as HTMLButtonElement;
    expect(confirmBtn).not.toBeNull();
    confirmBtn.click();
    await flushPromises();

    // The enqueue invoke must have been called.
    expect(mockInvoke).toHaveBeenCalledWith('enqueue_article_translation', {
      articleId: 'a1',
      triggerSource: 'manual',
    });

    // refreshArticle must fire immediately after the successful enqueue,
    // BEFORE any `translation:complete` event. The parent uses this to
    // re-fetch the article (now with `translation_status = 'queued'`) so the
    // badge flips to "Translation Queued" right away.
    const refreshEvents = wrapper.emitted('refreshArticle');
    expect(refreshEvents).toBeTruthy();
    expect(refreshEvents!.length).toBeGreaterThanOrEqual(1);
    expect(refreshEvents![0]).toEqual(['a1']);
  });

  it('refreshes_article_on_translation_complete_event', async () => {
    // TC-09: when the `translation:complete` event fires with success=true,
    // the article is refreshed so the header chip flips to "Translated".
    mockInvoke.mockResolvedValue(true);

    const wrapper = mountPanel({
      article: makeArticle({ language: 'French', isTranslated: false }),
    });

    // Open and confirm translation so the global listener is registered. The
    // dialog is teleported to document.body, so query the confirm button there.
    const detailHeader = wrapper.findComponent({ name: 'DetailHeader' });
    detailHeader.vm.$emit('request-translate');
    await flushPromises();
    const confirmBtn = document.body.querySelector('.btn--danger') as HTMLButtonElement;
    expect(confirmBtn).not.toBeNull();
    confirmBtn.click();
    await flushPromises();

    // The listener for `translation:complete` must be registered.
    expect(eventListeners.has('translation:complete')).toBe(true);

    // Fire the translation:complete event (simulating the Rust worker).
    const listener = eventListeners.get('translation:complete')!;
    listener({ payload: { articleId: 'a1', success: true } });
    await flushPromises();

    // The component should emit refreshArticle for the translated article.
    const refreshEvents = wrapper.emitted('refreshArticle');
    expect(refreshEvents).toBeTruthy();
    expect(refreshEvents!.length).toBeGreaterThanOrEqual(1);
    expect(refreshEvents![0]).toEqual(['a1']);
  });
});
