import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { ref } from 'vue';
import { shimLocalStorage, makeArticle as makeBaseArticle } from '../helpers/fixtures';
import type { Article, AuditEntry, ScreeningProgress } from '@/types';

// Mock Tauri core (invoke) + event (listen) so the translation + AI-summary
// composables don't throw when they register global event listeners during
// the panel's setup. Mirrors the pattern in
// article-detail-panel-translation.test.ts.
const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

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

// ── Reactive screening store mock ───────────────────────────────────
// The detail panel reads `screeningStore.progress` to drive the Screen
// button spinner. We expose a reactive `progressRef` the test can mutate.
const progressRef = ref<ScreeningProgress | null>(null);

vi.mock('@/stores/screening', () => ({
  useScreeningStore: () => {
    // Use a getter so `screeningStore.progress` returns the unwrapped value
    // (real Pinia stores auto-unwrap refs; plain-object mocks do not). The
    // getter also makes watch sources like
    // `() => screeningStore.progress?.isRunning` track `progressRef`.
    const store = {
      get progress(): ScreeningProgress | null {
        return progressRef.value;
      },
      readiness: null,
      loading: false,
      percentage: 0,
      estimatedTimeRemaining: null,
      fetchReadiness: vi.fn(),
      refreshProgress: vi.fn(),
      setProgress: vi.fn((p: ScreeningProgress | null) => {
        progressRef.value = p;
      }),
      startListening: vi.fn(),
      stopListening: vi.fn(),
    };
    return store;
  },
}));

// Stub the LLM config store so the Screen button is shown (isConfigured=true).
vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    initialized: true,
    fetchIfNeeded: vi.fn(),
    config: { apiKeyEncrypted: 'fake-key' },
    isConfigured: true,
  }),
}));

import ArticleDetailPanel from '@/components/article-detail-panel.vue';

/** Build a working-list (status='working'), unscreened article. */
function makeWorkingArticle(overrides: Partial<Article> = {}): Article {
  return makeBaseArticle({
    status: 'working',
    title: 'Screening Target',
    abstractText: 'Abstract.',
    authors: ['Author A'],
    publicationYear: 2024,
    referenceType: 'JOUR',
    ...overrides,
  });
}

const emptyAudit: AuditEntry[] = [];

function mountPanel(articleOverrides: Partial<Article> = {}) {
  setActivePinia(createPinia());
  return mount(ArticleDetailPanel, {
    props: {
      article: makeWorkingArticle(articleOverrides),
      auditTrail: emptyAudit,
      hasPrevious: false,
      hasNext: false,
      hasReturnTarget: false,
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

describe('article-detail-panel.vue - single-article screening button state', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    progressRef.value = null;
    mockInvoke.mockReset();
    eventListeners.clear();
    mockUnlisten.mockReset();
  });

  it('shows the Screen button enabled (no spinner) before any click', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const screenBtn = wrapper.find('button[title*="screening pipeline"]');
    expect(screenBtn.exists()).toBe(true);
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(false);

    // No spinner visible
    const spinner = screenBtn.find('.animate-spin');
    expect(spinner.exists()).toBe(false);
  });

  it('disables the button + shows spinner immediately on click (before any progress event)', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const screenBtn = wrapper.find('button[title*="screening pipeline"]');
    expect(screenBtn.exists()).toBe(true);

    // Click the button
    await screenBtn.trigger('click');
    await flushPromises();

    // The button must now be disabled synchronously - this is the fix for
    // Issue 2 (spinner not showing). The local `isScreening` flag flips true
    // before any progress event arrives.
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(true);

    // The spinner must be visible
    const spinner = screenBtn.find('.animate-spin');
    expect(spinner.exists()).toBe(true);

    // screenArticle event must have been emitted
    const events = wrapper.emitted('screenArticle');
    expect(events).toBeTruthy();
    expect(events![0]).toEqual(['a1']);
  });

  it('keeps the button disabled through the refresh gap (isRunning false but article not yet updated)', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const screenBtn = wrapper.find('button[title*="screening pipeline"]');

    // Click to start screening
    await screenBtn.trigger('click');
    await flushPromises();

    // Simulate the backend progress event arriving with the article title
    progressRef.value = {
      total: 1,
      completed: 0,
      included: 0,
      rejected: 0,
      errors: 0,
      isRunning: true,
      currentArticleTitles: ['Screening Target'],
      elapsedMs: 100,
      estimatedRemainingMs: null,
    };
    await flushPromises();
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(true);

    // Simulate the run completing (isRunning flips false) but the article
    // prop has NOT yet been refreshed (still working + unscreened). This is
    // the refresh-gap from Issue 1. The button must stay disabled.
    progressRef.value = {
      ...progressRef.value,
      completed: 1,
      isRunning: false,
      currentArticleTitles: [],
    };
    await flushPromises();

    // The button must still be disabled because the article prop hasn't
    // updated yet (still status=working, no screenedAt).
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(true);

    // The backup completion trigger should have emitted a refreshArticle event
    // so the parent retries the fetch.
    const refreshEvents = wrapper.emitted('refreshArticle');
    expect(refreshEvents).toBeTruthy();
    expect(refreshEvents!.length).toBeGreaterThanOrEqual(1);
  });

  it('re-enables the button when the article prop is refreshed to a post-screening state', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const screenBtn = wrapper.find('button[title*="screening pipeline"]');

    // Click to start screening
    await screenBtn.trigger('click');
    await flushPromises();

    // Simulate the run completing
    progressRef.value = {
      total: 1,
      completed: 1,
      included: 1,
      rejected: 0,
      errors: 0,
      isRunning: false,
      currentArticleTitles: [],
      elapsedMs: 5000,
      estimatedRemainingMs: null,
    };
    await flushPromises();

    // Button still disabled (article prop not yet updated)
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(true);

    // Now the parent refreshes the article prop to the post-screening state
    // (status=included, screenedAt set). The Screen button should auto-hide
    // because canScreenArticle becomes false (status !== 'working').
    await wrapper.setProps({
      article: makeWorkingArticle({
        status: 'included',
        screenedAt: '2024-02-03T00:00:00Z',
        aiConfidence: 0.92,
        aiDecision: 'include',
      }),
    });
    await flushPromises();

    // The Screen button should now be gone (v-if=canScreenArticle is false)
    expect(wrapper.find('button[title*="screening pipeline"]').exists()).toBe(false);
  });

  it('re-enables the button when the article prop is refreshed to a screening error', async () => {
    const wrapper = mountPanel();
    await flushPromises();

    const screenBtn = wrapper.find('button[title*="screening pipeline"]');

    // Click to start screening
    await screenBtn.trigger('click');
    await flushPromises();

    // Simulate the run completing with an error outcome
    progressRef.value = {
      total: 1,
      completed: 1,
      included: 0,
      rejected: 0,
      errors: 1,
      isRunning: false,
      currentArticleTitles: [],
      elapsedMs: 5000,
      estimatedRemainingMs: null,
    };
    await flushPromises();

    // Button still disabled (article prop not yet updated)
    expect((screenBtn.element as HTMLButtonElement).disabled).toBe(true);

    // Parent refreshes the article prop: screeningError=true. The Screen
    // button should auto-hide (canScreenArticle is false because
    // screeningError is truthy).
    await wrapper.setProps({
      article: makeWorkingArticle({ screeningError: true }),
    });
    await flushPromises();

    // The Screen button should be gone
    expect(wrapper.find('button[title*="screening pipeline"]').exists()).toBe(false);
  });
});
