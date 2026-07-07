import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

// ── Mock stores + composables ──────────────────────────────────────────────
// Module-level refs so tests can mutate state and the mounted view re-renders.

const mockSummaryText = ref<string | null>('# Review\n\nContent');
const mockSummaryLoading = ref(false);
const mockSummaryError = ref<string | null>(null);
const mockCitationStyle = ref<'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA'>('APA');
const mockGenerateSummary = vi.fn();
const mockLoadSavedSummary = vi.fn().mockResolvedValue(undefined);
const mockFormatSummaryGeneratedAt = vi.fn().mockReturnValue(null);

const mockGapText = ref<string | null>('# Gaps\n\nGap content');
const mockGapLoading = ref(false);
const mockGapError = ref<string | null>(null);
const mockGenerateGap = vi.fn();
const mockLoadSavedGap = vi.fn().mockResolvedValue(undefined);
const mockFormatGapGeneratedAt = vi.fn().mockReturnValue(null);

const mockIncludedCount = ref(5);
const mockAims = ref([{ id: 'a1', text: 'Aim 1' }]);
const mockHasLlmConfig = ref(true);

function resetMockState(): void {
  mockSummaryText.value = '# Review\n\nContent';
  mockSummaryLoading.value = false;
  mockSummaryError.value = null;
  mockCitationStyle.value = 'APA';
  mockGapText.value = '# Gaps\n\nGap content';
  mockGapLoading.value = false;
  mockGapError.value = null;
  mockIncludedCount.value = 5;
  mockAims.value = [{ id: 'a1', text: 'Aim 1' }];
  mockHasLlmConfig.value = true;
}

vi.mock('@/composables/use-summary', () => ({
  useSummary: () => ({
    summaryText: mockSummaryText,
    loading: mockSummaryLoading,
    error: mockSummaryError,
    citationStyle: mockCitationStyle,
    generate: mockGenerateSummary,
    loadSaved: mockLoadSavedSummary,
    clearSummary: vi.fn(),
    formatGeneratedAt: mockFormatSummaryGeneratedAt,
  }),
}));

vi.mock('@/composables/use-gap-analysis', () => ({
  useGapAnalysis: () => ({
    gapText: mockGapText,
    loading: mockGapLoading,
    error: mockGapError,
    generate: mockGenerateGap,
    loadSaved: mockLoadSavedGap,
    clearGapAnalysis: vi.fn(),
    formatGeneratedAt: mockFormatGapGeneratedAt,
  }),
}));

// Use getters so the returned plain object mirrors Pinia's auto-unwrap
// behavior: the view accesses `articlesStore.byStatus.included` etc. and
// expects unwrapped values, not ComputedRef objects. Reading `mockX.value`
// inside a getter tracks the ref so view computeds stay reactive.
vi.mock('@/stores/articles', () => ({
  useArticlesStore: () => ({
    get byStatus() {
      return { included: mockIncludedCount.value };
    },
    fetchArticles: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('@/stores/criteria', () => ({
  useCriteriaStore: () => ({
    get aims() {
      return mockAims.value;
    },
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: () => ({
    get config() {
      return { apiKeyEncrypted: mockHasLlmConfig.value ? 'enc-key' : null };
    },
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));

// Stub the @tauri-apps/plugin-dialog save() so export paths don't touch the OS.
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn().mockResolvedValue(null),
}));

import SummaryView from '@/views/summary-view.vue';

function mountView() {
  setActivePinia(createPinia());
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  });
  return mount(SummaryView, {
    global: { plugins: [createPinia(), router] },
  });
}

/** Find a button inside the header actions by its label substring. */
function findHeaderButton(wrapper: ReturnType<typeof mount>, labelSubstring: string) {
  return wrapper
    .findAll('.summary-header__actions .btn')
    .find((b) => b.text().includes(labelSubstring));
}

describe('summary-view.vue - two-button UX (Research Gap Report | Summarize Findings)', () => {
  beforeEach(() => {
    mockGenerateSummary.mockReset();
    mockGenerateGap.mockReset();
    mockLoadSavedSummary.mockReset().mockResolvedValue(undefined);
    mockLoadSavedGap.mockReset().mockResolvedValue(undefined);
    mockFormatSummaryGeneratedAt.mockReset().mockReturnValue(null);
    mockFormatGapGeneratedAt.mockReset().mockReturnValue(null);
    resetMockState();
  });

  it('renders the static AI Summary title and original tagline', async () => {
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.find('.page-title').text()).toBe('AI Summary');
    expect(wrapper.find('.summary-header__tagline').text()).toContain(
      'Have AI create a summary based on included papers'
    );
  });

  it('renders both generate buttons side by side in the header actions', async () => {
    const wrapper = mountView();
    await flushPromises();

    // No segmented control.
    expect(wrapper.find('.summary-segments').exists()).toBe(false);

    const actions = wrapper.findAll('.summary-header__actions .btn');
    expect(actions).toHaveLength(2);
    // Left button is the Research Gap Report (secondary).
    expect(actions[0]!.text()).toContain('Research Gap Report');
    expect(actions[0]!.classes()).toContain('btn--secondary');
    // Right button is Summarize Findings (primary).
    expect(actions[1]!.text()).toContain('Summarize Findings');
    expect(actions[1]!.classes()).toContain('btn--primary');
  });

  it('clicking Research Gap Report calls generateGap and switches output to gap text', async () => {
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    expect(gapBtn).toBeTruthy();
    await gapBtn!.trigger('click');
    await flushPromises();

    expect(mockGenerateGap).toHaveBeenCalledWith('APA');
    expect(mockGenerateSummary).not.toHaveBeenCalled();
    // Output area now shows the gap text.
    expect(wrapper.find('.summary-view__markdown').html()).toContain('Gap content');
  });

  it('clicking Summarize Findings calls generateSummary and keeps output on review', async () => {
    mockGenerateSummary.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    const summaryBtn = findHeaderButton(wrapper, 'Summarize Findings');
    expect(summaryBtn).toBeTruthy();
    await summaryBtn!.trigger('click');
    await flushPromises();

    expect(mockGenerateSummary).toHaveBeenCalledWith('APA');
    expect(mockGenerateGap).not.toHaveBeenCalled();
    // Output area shows the summary text (default mode is review).
    expect(wrapper.find('.summary-view__markdown').html()).toContain('Review');
  });

  it('while summary is loading, the gap button is disabled (cross-disable)', async () => {
    mockSummaryLoading.value = true;
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    expect(gapBtn).toBeTruthy();
    expect(gapBtn!.attributes('disabled')).toBeDefined();
  });

  it('while gap is loading, the summary button is disabled (cross-disable)', async () => {
    mockGapLoading.value = true;
    const wrapper = mountView();
    await flushPromises();

    const summaryBtn = findHeaderButton(wrapper, 'Summarize Findings');
    expect(summaryBtn).toBeTruthy();
    expect(summaryBtn!.attributes('disabled')).toBeDefined();
  });

  it('shows its own spinner label when its own generation is in flight', async () => {
    mockGapLoading.value = true;
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Generating');
    expect(gapBtn).toBeTruthy();
    expect(gapBtn!.text()).toContain('Generating');
  });

  it('disables both generate buttons when requirements are unmet', async () => {
    mockHasLlmConfig.value = false;
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    const summaryBtn = findHeaderButton(wrapper, 'Summarize Findings');
    expect(gapBtn!.attributes('disabled')).toBeDefined();
    expect(summaryBtn!.attributes('disabled')).toBeDefined();

    // The requirements card renders.
    expect(wrapper.find('.summary-requirements').exists()).toBe(true);
  });

  it('shows the active report error banner', async () => {
    // Default mode is review -> review error shows.
    mockSummaryError.value = 'review failed';
    const wrapper = mountView();
    await flushPromises();
    expect(wrapper.find('.summary-view__error').text()).toContain('review failed');
  });

  it('shows the gap error after the gap button is clicked', async () => {
    mockGenerateGap.mockResolvedValue(undefined);
    mockGapError.value = 'gap failed';
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    expect(wrapper.find('.summary-view__error').text()).toContain('gap failed');
  });
});
