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
const mockReviewInstructions = ref('');
const mockReviewTargetWords = ref('');
const mockGenerateSummary = vi.fn();
const mockLoadSavedSummary = vi.fn().mockResolvedValue(undefined);
const mockFormatSummaryGeneratedAt = vi.fn().mockReturnValue(null);

const mockGapText = ref<string | null>('# Gaps\n\nGap content');
const mockGapLoading = ref(false);
const mockGapError = ref<string | null>(null);
const mockGapInstructions = ref('');
const mockGapTargetWords = ref('');
const mockGenerateGap = vi.fn();
const mockLoadSavedGap = vi.fn().mockResolvedValue(undefined);
const mockFormatGapGeneratedAt = vi.fn().mockReturnValue(null);

const mockIncludedCount = ref(5);
const mockAims = ref([{ id: 'a1', text: 'Aim 1' }]);
const mockHasLlmConfig = ref(true);
const mockIsPremium = ref(false);

function resetMockState(): void {
  mockSummaryText.value = '# Review\n\nContent';
  mockSummaryLoading.value = false;
  mockSummaryError.value = null;
  mockCitationStyle.value = 'APA';
  mockReviewInstructions.value = '';
  mockReviewTargetWords.value = '';
  mockGapText.value = '# Gaps\n\nGap content';
  mockGapLoading.value = false;
  mockGapError.value = null;
  mockGapInstructions.value = '';
  mockGapTargetWords.value = '';
  mockIncludedCount.value = 5;
  mockAims.value = [{ id: 'a1', text: 'Aim 1' }];
  mockHasLlmConfig.value = true;
  mockIsPremium.value = false;
}

vi.mock('@/composables/use-summary', () => ({
  useSummary: () => ({
    summaryText: mockSummaryText,
    loading: mockSummaryLoading,
    error: mockSummaryError,
    citationStyle: mockCitationStyle,
    additionalInstructions: mockReviewInstructions,
    targetWordCount: mockReviewTargetWords,
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
    additionalInstructions: mockGapInstructions,
    targetWordCount: mockGapTargetWords,
    generate: mockGenerateGap,
    loadSaved: mockLoadSavedGap,
    clearGapAnalysis: vi.fn(),
    formatGeneratedAt: mockFormatGapGeneratedAt,
  }),
}));

vi.mock('@/composables/use-feature-flags', () => ({
  useFeatureFlags: () => ({
    isPremium: mockIsPremium,
  }),
}));

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
    get isConfigured() {
      return mockHasLlmConfig.value;
    },
    config: { apiKeyEncrypted: 'enc-key' },
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  }),
}));

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

function findHeaderButton(wrapper: ReturnType<typeof mount>, labelSubstring: string) {
  return wrapper
    .findAll('.summary-header__actions .btn')
    .find((b) => b.text().includes(labelSubstring));
}

describe('summary-view.vue - two-button UX with switch-vs-regenerate dialog', () => {
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

    expect(wrapper.find('.summary-segments').exists()).toBe(false);

    const actions = wrapper.findAll('.summary-header__actions .btn');
    expect(actions).toHaveLength(2);
    expect(actions[0]!.text()).toContain('Research Gap Report');
    expect(actions[1]!.text()).toContain('Summarize Findings');
  });

  it('first generation (no existing content) calls generate directly without dialog', async () => {
    // No existing gap text -> clicking gap button generates directly.
    mockGapText.value = null;
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    expect(mockGenerateGap).toHaveBeenCalledWith({ style: 'APA' });
    // No dialog shown.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
  });

  it('existing content + clicking generate opens the switch dialog (no generate yet)', async () => {
    // Gap text exists -> dialog should open, no generate called.
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    // Dialog is open.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(true);
    expect(wrapper.find('.dialog').text()).toContain('Research Gap Report already exists');
    // Generate NOT called yet (user hasn't chosen Regenerate).
    expect(mockGenerateGap).not.toHaveBeenCalled();
  });

  it('switch dialog "View existing" switches mode without generating', async () => {
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    // Click gap button -> dialog opens.
    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    // Click "View existing".
    const viewBtn = wrapper
      .findAll('.dialog__actions .btn')
      .find((b) => b.text().includes('View existing'));
    expect(viewBtn).toBeTruthy();
    await viewBtn!.trigger('click');
    await flushPromises();

    // No generate call.
    expect(mockGenerateGap).not.toHaveBeenCalled();
    // Output area now shows the gap text (mode switched).
    expect(wrapper.find('.summary-view__markdown').html()).toContain('Gap content');
    // Dialog closed.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
  });

  it('switch dialog "Regenerate" calls generate and switches after completion', async () => {
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    // Click gap button -> dialog opens.
    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    // Click "Regenerate".
    const regenBtn = wrapper
      .findAll('.dialog__actions .btn')
      .find((b) => b.text().includes('Regenerate'));
    expect(regenBtn).toBeTruthy();
    await regenBtn!.trigger('click');
    await flushPromises();

    // Generate called.
    expect(mockGenerateGap).toHaveBeenCalledWith({ style: 'APA' });
    // Dialog closed.
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
  });

  it('switch dialog "Cancel" closes without generating or switching', async () => {
    mockGenerateGap.mockResolvedValue(undefined);
    const wrapper = mountView();
    await flushPromises();

    // Default mode is review. Click gap -> dialog opens.
    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    await gapBtn!.trigger('click');
    await flushPromises();

    // Click "Cancel".
    const cancelBtn = wrapper
      .findAll('.dialog__actions .btn')
      .find((b) => b.text().includes('Cancel'));
    expect(cancelBtn).toBeTruthy();
    await cancelBtn!.trigger('click');
    await flushPromises();

    // No generate, dialog closed.
    expect(mockGenerateGap).not.toHaveBeenCalled();
    expect(wrapper.find('.dialog-overlay').exists()).toBe(false);
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

    expect(wrapper.find('.summary-requirements').exists()).toBe(true);
  });

  it('enables both buttons for a local provider with no API key (isConfigured gate)', async () => {
    mockHasLlmConfig.value = true;
    const wrapper = mountView();
    await flushPromises();

    const gapBtn = findHeaderButton(wrapper, 'Research Gap Report');
    const summaryBtn = findHeaderButton(wrapper, 'Summarize Findings');
    expect(gapBtn!.attributes('disabled')).toBeUndefined();
    expect(summaryBtn!.attributes('disabled')).toBeUndefined();
    expect(wrapper.find('.summary-requirements').exists()).toBe(false);
  });

  it('shows the active report error banner', async () => {
    mockSummaryError.value = 'review failed';
    const wrapper = mountView();
    await flushPromises();
    expect(wrapper.find('.summary-view__error').text()).toContain('review failed');
  });

  // ── Premium guidance cards (per-report LLM instructions + target words) ──

  it('guidance cards are hidden when not premium', async () => {
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.findAll('.summary-guidance')).toHaveLength(0);
  });

  it('renders both guidance cards collapsed when premium', async () => {
    mockIsPremium.value = true;
    const wrapper = mountView();
    await flushPromises();

    const cards = wrapper.findAll('.summary-guidance');
    expect(cards).toHaveLength(2);
    expect(cards[0]!.text()).toContain('Literature Review Instructions');
    expect(cards[1]!.text()).toContain('Research Gap Report Instructions');
    // Collapsed by default: no inputs rendered.
    expect(wrapper.find('.summary-guidance__textarea').exists()).toBe(false);
    expect(wrapper.find('.summary-guidance__input').exists()).toBe(false);
  });

  it('expanding a card reveals its textarea and word input (premium)', async () => {
    mockIsPremium.value = true;
    const wrapper = mountView();
    await flushPromises();

    await wrapper.find('.summary-guidance__toggle').trigger('click');
    const textarea = wrapper.find('#review-instructions');
    const words = wrapper.find('#review-target-words');
    expect(textarea.exists()).toBe(true);
    expect(words.exists()).toBe(true);

    await textarea.setValue('Focus on UK policy studies.');
    await words.setValue('1200');
    expect(mockReviewInstructions.value).toBe('Focus on UK policy studies.');
    // v-model on type="number" auto-casts valid entries to numbers.
    expect(mockReviewTargetWords.value).toBe(1200);
  });

  it('review generate forwards its card values when premium', async () => {
    mockIsPremium.value = true;
    mockSummaryText.value = null; // first generation: no switch dialog
    mockGenerateSummary.mockResolvedValue(undefined);
    mockReviewInstructions.value = 'Focus on UK policy studies.';
    mockReviewTargetWords.value = '1200';
    const wrapper = mountView();
    await flushPromises();

    await findHeaderButton(wrapper, 'Summarize Findings')!.trigger('click');
    await flushPromises();

    expect(mockGenerateSummary).toHaveBeenCalledWith({
      style: 'APA',
      additionalInstructions: 'Focus on UK policy studies.',
      targetWordCount: 1200,
    });
  });

  it('gap generate forwards its own card values when premium', async () => {
    mockIsPremium.value = true;
    mockGapText.value = null; // first generation: no switch dialog
    mockGenerateGap.mockResolvedValue(undefined);
    mockGapInstructions.value = 'Prioritize UK studies.';
    mockGapTargetWords.value = '800';
    const wrapper = mountView();
    await flushPromises();

    await findHeaderButton(wrapper, 'Research Gap Report')!.trigger('click');
    await flushPromises();

    expect(mockGenerateGap).toHaveBeenCalledWith({
      style: 'APA',
      additionalInstructions: 'Prioritize UK studies.',
      targetWordCount: 800,
    });
  });

  it('invalid word counts parse to null (no length constraint)', async () => {
    mockIsPremium.value = true;
    mockSummaryText.value = null;
    mockGenerateSummary.mockResolvedValue(undefined);
    mockReviewInstructions.value = 'Keep it terse';
    mockReviewTargetWords.value = 'abc';
    const wrapper = mountView();
    await flushPromises();

    await findHeaderButton(wrapper, 'Summarize Findings')!.trigger('click');
    await flushPromises();

    expect(mockGenerateSummary).toHaveBeenCalledWith({
      style: 'APA',
      additionalInstructions: 'Keep it terse',
      targetWordCount: null,
    });
  });

  it('stale guidance values are not forwarded when not premium', async () => {
    mockIsPremium.value = false;
    mockSummaryText.value = null;
    mockGenerateSummary.mockResolvedValue(undefined);
    // Values left over from a premium session must never reach the command.
    mockReviewInstructions.value = 'stale';
    mockReviewTargetWords.value = '999';
    const wrapper = mountView();
    await flushPromises();

    await findHeaderButton(wrapper, 'Summarize Findings')!.trigger('click');
    await flushPromises();

    expect(mockGenerateSummary).toHaveBeenCalledWith({ style: 'APA' });
  });
});
