import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';
import type { Article, AuditEntry } from '@/types';

/* Stub the stores + IPC so the panel mounts without backend interaction. */
const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: () => Promise.resolve(() => undefined),
}));
vi.mock('@/stores/screening', () => ({
  useScreeningStore: () => ({
    progress: null,
    readiness: null,
    loading: false,
    percentage: 0,
    estimatedTimeRemaining: null,
    fetchReadiness: vi.fn(),
    refreshProgress: vi.fn(),
    setProgress: vi.fn(),
    startListening: vi.fn(),
    stopListening: vi.fn(),
  }),
}));
vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    initialized: true,
    fetchIfNeeded: vi.fn(),
    config: {},
    isConfigured: false,
  }),
}));

import ArticleDetailPanel from '@/components/article-detail-panel.vue';

/** Build a minimal non-fullscreen article so the resizer renders. */
function makeArticle(): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'Test Article',
    abstractText: 'Abstract.',
    authors: ['Author A'],
    publicationYear: 2024,
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
  } as Article;
}

const emptyAudit: AuditEntry[] = [];

function mountPanel() {
  setActivePinia(createPinia());
  return mount(ArticleDetailPanel, {
    props: {
      article: makeArticle(),
      auditTrail: emptyAudit,
      hasPrevious: false,
      hasNext: false,
      hasReturnTarget: false,
      fullScreen: false,
      articlePosition: 1,
      articleTotal: 1,
    },
    global: {
      plugins: [createPinia()],
      stubs: {
        AuditTimeline: true,
        DetailHeader: true,
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

/** Dispatch a synthetic MouseEvent on `window`. jsdom/happy-dom support the
 *  `clientX` + `buttons` init props. */
function dispatchWindowMouse(
  type: string,
  init: { clientX?: number; buttons?: number } = {}
): void {
  window.dispatchEvent(new MouseEvent(type, { bubbles: true, ...init }));
}

describe('article-detail-panel.vue - resize drag shield + stuck-drag guard', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    mockInvoke.mockReset();
  });

  it('renders the drag shield when a resize starts (mousedown on the resizer)', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    /* No shield before the drag starts. */
    expect(document.querySelector('[data-testid="drag-shield"]')).toBeNull();

    /* The resizer is the only element with the `resizer` class. Trigger
       mousedown to begin the drag. */
    const resizer = wrapper.find('.resizer');
    expect(resizer.exists()).toBe(true);
    await resizer.trigger('mousedown', { clientX: 500, buttons: 1 });
    await flushPromises();

    /* The shield is teleported to <body>, so query the document. */
    const shield = document.querySelector('[data-testid="drag-shield"]');
    expect(shield).not.toBeNull();
  });

  it('ends the resize (removes the shield) on mouseup', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const resizer = wrapper.find('.resizer');
    await resizer.trigger('mousedown', { clientX: 500, buttons: 1 });
    await flushPromises();
    expect(document.querySelector('[data-testid="drag-shield"]')).not.toBeNull();

    /* Simulate releasing the mouse button. */
    dispatchWindowMouse('mouseup');
    await flushPromises();

    expect(document.querySelector('[data-testid="drag-shield"]')).toBeNull();
    /* Body cursor restored. */
    expect(document.body.style.cursor).toBe('');
  });

  it('ends the resize when a mousemove arrives with no button pressed (lost-mouseup safety net)', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const resizer = wrapper.find('.resizer');
    await resizer.trigger('mousedown', { clientX: 500, buttons: 1 });
    await flushPromises();
    expect(document.querySelector('[data-testid="drag-shield"]')).not.toBeNull();

    /* Simulate a mousemove with buttons === 0 (no button held). This is the
       "lost mouseup" edge case: the cursor left the window mid-drag, the
       OS never delivered a mouseup, and the user moved the mouse back into
       the window without holding the button. The safety net must end the
       resize so the listener does not stay permanently active. */
    dispatchWindowMouse('mousemove', { clientX: 450, buttons: 0 });
    await flushPromises();

    expect(document.querySelector('[data-testid="drag-shield"]')).toBeNull();
    expect(document.body.style.cursor).toBe('');
  });

  it('resizes the panel while the button is held (mousemove with buttons=1)', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const resizer = wrapper.find('.resizer');
    await resizer.trigger('mousedown', { clientX: 500, buttons: 1 });
    await flushPromises();

    /* Drag left by 100px (delta = startX - clientX = 500 - 400 = 100, panel
       grows wider). The shield stays up while the button is held. */
    dispatchWindowMouse('mousemove', { clientX: 400, buttons: 1 });
    await flushPromises();

    expect(document.querySelector('[data-testid="drag-shield"]')).not.toBeNull();
    /* The persisted width should have grown from the default 480 toward 580. */
    const persisted = window.localStorage.getItem('bango-detail-panel-width');
    expect(persisted).not.toBeNull();
    expect(parseInt(persisted as string)).toBeGreaterThan(480);

    /* End the drag cleanly. */
    dispatchWindowMouse('mouseup');
    await flushPromises();
    expect(document.querySelector('[data-testid="drag-shield"]')).toBeNull();
  });

  it('does not render the resizer or shield in fullscreen mode', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    await wrapper.setProps({ fullScreen: true });
    await flushPromises();

    expect(wrapper.find('.resizer').exists()).toBe(false);
    expect(document.querySelector('[data-testid="drag-shield"]')).toBeNull();
  });
});
