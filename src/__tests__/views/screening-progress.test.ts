import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref, computed } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';
import type { ScreeningProgress, ScreeningReadiness } from '@/types';

const mockStartScreening = vi.fn();

const mockReadiness = ref<ScreeningReadiness>({
  totalWorking: 10,
  totalUnscreened: 5,
  hasAims: true,
  hasInclusion: true,
  hasExclusion: true,
  hasLlmConfig: true,
  tokenWarning: null,
  progress: null,
});
const mockProgress = ref<ScreeningProgress | null>(null);
const mockLoading = ref(false);
const mockReadinessLoading = ref(false);
const mockError = ref<string | null>(null);
const mockTokenWarning = ref<string | null>(null);

function resetMockState(): void {
  mockReadiness.value = {
    totalWorking: 10,
    totalUnscreened: 5,
    hasAims: true,
    hasInclusion: true,
    hasExclusion: true,
    hasLlmConfig: true,
    tokenWarning: null,
    progress: null,
  };
  mockProgress.value = null;
  mockLoading.value = false;
  mockReadinessLoading.value = false;
  mockError.value = null;
  mockTokenWarning.value = null;
}

vi.mock('@/composables/use-screening', () => ({
  useScreening: () => ({
    progress: mockProgress,
    loading: mockLoading,
    readinessLoading: mockReadinessLoading,
    error: mockError,
    tokenWarning: mockTokenWarning,
    readiness: mockReadiness,
    percentage: computed(() => {
      if (!mockProgress.value || mockProgress.value.total === 0) return 0;
      return Math.round((mockProgress.value.completed / mockProgress.value.total) * 100);
    }),
    estimatedTimeRemaining: ref('-'),
    fetchReadiness: vi.fn().mockResolvedValue(undefined),
    startScreening: mockStartScreening,
    pauseScreening: vi.fn(),
    resumeScreening: vi.fn(),
    stopScreening: vi.fn(),
    startListening: vi.fn().mockResolvedValue(undefined),
    stopListening: vi.fn(),
    resetScreeningErrors: vi.fn().mockResolvedValue(0),
    resetWorkingList: vi.fn().mockResolvedValue(0),
  }),
}));

import ScreeningProgressView from '@/views/screening-progress.vue';

function mountView() {
  setActivePinia(createPinia());
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  });
  return mount(ScreeningProgressView, {
    global: { plugins: [createPinia(), router] },
  });
}

function getStartButton(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('button').find((b) => b.text().includes('Start Screening'));
}

describe('screening-progress.vue - start controls and subtitle', () => {
  beforeEach(() => {
    mockStartScreening.mockReset();
    resetMockState();
  });

  it('shows idle subtitle as Available: N article(s)', async () => {
    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.text()).toContain('Available: 5 article(s)');
  });

  it('shows running subtitle as Processing: X of Y article(s)', async () => {
    mockProgress.value = {
      total: 2,
      completed: 1,
      included: 1,
      rejected: 0,
      errors: 0,
      isRunning: true,
      currentArticleTitles: ['Article A'],
      elapsedMs: 500,
      estimatedRemainingMs: 500,
      stage: null,
      stageTotal: null,
    };

    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.text()).toContain('Processing: 1 of 2 article(s)');
    expect(wrapper.find('.screening-view__start-config').exists()).toBe(false);
  });

  it('uses Available subtitle when progress exists but is not running', async () => {
    mockProgress.value = {
      total: 2,
      completed: 2,
      included: 2,
      rejected: 0,
      errors: 0,
      isRunning: false,
      currentArticleTitles: [],
      elapsedMs: 1000,
      estimatedRemainingMs: null,
      stage: null,
      stageTotal: null,
    };
    mockReadiness.value.totalUnscreened = 3;

    const wrapper = mountView();
    await flushPromises();

    expect(wrapper.text()).toContain('Available: 3 article(s)');
  });

  it('defaults number-to-process to total unscreened and disables [+] at max', async () => {
    const wrapper = mountView();
    await flushPromises();

    const numInput = wrapper.find('#num-to-process-input');
    expect(numInput.attributes('value')).toBe('5');

    const incNumBtn = wrapper.find('button[aria-label="Increase number to process"]');
    expect(incNumBtn.attributes('disabled')).toBeDefined();
  });

  it('decrements number-to-process and passes both args to startScreening', async () => {
    const wrapper = mountView();
    await flushPromises();

    const decNumBtn = wrapper.find('button[aria-label="Decrease number to process"]');
    await decNumBtn.trigger('click');
    await flushPromises();

    const batchIncBtn = wrapper.find('button[aria-label="Increase batch size"]');
    await batchIncBtn.trigger('click');
    await flushPromises();

    const startBtn = getStartButton(wrapper);
    expect(startBtn).toBeTruthy();
    await startBtn!.trigger('click');
    await flushPromises();

    expect(mockStartScreening).toHaveBeenCalledWith(2, 4);
  });

  it('disables all start controls when no unscreened articles are available', async () => {
    mockReadiness.value.totalUnscreened = 0;

    const wrapper = mountView();
    await flushPromises();

    const numInput = wrapper.find('#num-to-process-input');
    const batchInput = wrapper.find('#batch-input');
    expect(numInput.attributes('disabled')).toBeDefined();
    expect(batchInput.attributes('disabled')).toBeDefined();

    expect(
      wrapper.find('button[aria-label="Decrease number to process"]').attributes('disabled')
    ).toBeDefined();
    expect(
      wrapper.find('button[aria-label="Increase number to process"]').attributes('disabled')
    ).toBeDefined();
    expect(
      wrapper.find('button[aria-label="Decrease batch size"]').attributes('disabled')
    ).toBeDefined();
    expect(
      wrapper.find('button[aria-label="Increase batch size"]').attributes('disabled')
    ).toBeDefined();

    const startBtn = getStartButton(wrapper);
    expect(startBtn).toBeTruthy();
    expect(startBtn!.attributes('disabled')).toBeDefined();
  });
});
