import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';

// Mock @tauri-apps/api/core so the component's `invoke` calls are captured.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock the event listener so onMounted does not blow up. The listen callback
// is not needed for these tests; we just need the registration to resolve.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

import SettingsReprocessing from '@/components/settings/settings-reprocessing.vue';

/** An idle/empty progress snapshot as returned by a fresh backend state. */
const IDLE_PROGRESS = {
  phase: 0,
  phaseName: '',
  completed: 0,
  total: 0,
  overallPercent: 0,
  message: '',
  isRunning: false,
  isCancelled: false,
  fullText: null,
  citations: null,
  summaries: null,
};

function mountCard() {
  setActivePinia(createPinia());
  return mount(SettingsReprocessing, {
    global: { plugins: [createPinia()] },
  });
}

describe('settings-reprocessing.vue', () => {
  beforeEach(() => {
    // happy-dom's localStorage lacks removeItem/clear; install a full shim so
    // the readAutoSummarize/readSectionSummaries helpers in the dialog render
    // path work without throwing.
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    mockInvoke.mockReset();
    // Default: no full-text articles (hides the rebuild section), idle progress.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(0);
      if (cmd === 'get_batch_import_progress') return Promise.resolve(IDLE_PROGRESS);
      if (cmd === 'start_batch_import') {
        return Promise.resolve({ ...IDLE_PROGRESS, isRunning: true });
      }
      return Promise.resolve(undefined);
    });
  });

  it('hides the batch progress bar on mount when no import is running', async () => {
    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.batch-progress').exists()).toBe(false);
  });

  it('reveals the progress bar when a run is already in progress on mount', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(0);
      if (cmd === 'get_batch_import_progress') {
        return Promise.resolve({ ...IDLE_PROGRESS, isRunning: true, phaseName: 'Full Text' });
      }
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.batch-progress').exists()).toBe(true);
  });

  it('hides the progress bar, then reveals it after clicking Start in the dialog', async () => {
    const wrapper = mountCard();
    await flushPromises();

    // Bar hidden initially.
    expect(wrapper.find('.batch-progress').exists()).toBe(false);

    // Open the dialog and click Start.
    const openBtn = wrapper.findAll('button').find((b) => b.text().includes('Import full text'));
    expect(openBtn).toBeTruthy();
    await openBtn!.trigger('click');
    await flushPromises();

    const startBtn = wrapper.findAll('button').find((b) => b.text().includes('Start'));
    expect(startBtn).toBeTruthy();
    await startBtn!.trigger('click');
    await flushPromises();

    // Bar is now revealed.
    expect(wrapper.find('.batch-progress').exists()).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('start_batch_import', expect.objectContaining({}));
  });
});
