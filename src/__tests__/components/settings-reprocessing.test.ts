import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { shimLocalStorage } from '../helpers/fixtures';

// Mock @tauri-apps/api/core so the component's `invoke` calls are captured.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock the event API. Handlers are captured per event name so tests can
// simulate live backend events (`chunk-rebuild:progress`, `embedding:progress`).
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    eventHandlers.set(name, handler);
    return () => undefined;
  }),
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

/** An idle chunk-rebuild progress snapshot (fresh `RebuildChunksState`). */
const REBUILD_IDLE = {
  phase: 'idle',
  isRunning: false,
  isCancelled: false,
  completed: 0,
  total: 0,
  percent: 0,
  chunked: 0,
  failed: 0,
  skipped: 0,
  skippedTranslated: 0,
  message: '',
  errors: [] as string[],
  embeddingSummary: null,
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
    eventHandlers.clear();
    // Default: no full-text articles (hides the rebuild section), idle progress.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(0);
      if (cmd === 'get_batch_import_progress') return Promise.resolve(IDLE_PROGRESS);
      if (cmd === 'get_rebuild_chunks_progress') return Promise.resolve(REBUILD_IDLE);
      if (cmd === 'start_batch_import') {
        return Promise.resolve({ ...IDLE_PROGRESS, isRunning: true });
      }
      if (cmd === 'start_rebuild_chunks') {
        return Promise.resolve({ ...REBUILD_IDLE, isRunning: true, phase: 'chunks', total: 3 });
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

  it('renders the phase label and percent from the snapshot (Batch Import @ 100%)', async () => {
    // The terminal snapshot emitted after all phases finish carries
    // phaseName "Batch Import" + overallPercent 100 so the user sees an
    // unambiguous end state instead of the just-finished "Embeddings".
    // The bar is data-driven: it renders whatever phaseName/overallPercent
    // the backend sends. We mount with isRunning=true so batchStarted flips
    // on (the reveal guard), then assert the header renders the values.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(0);
      if (cmd === 'get_batch_import_progress') {
        return Promise.resolve({
          ...IDLE_PROGRESS,
          isRunning: true,
          phaseName: 'Batch Import',
          overallPercent: 100,
          message: 'Batch import complete',
        });
      }
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.batch-progress').exists()).toBe(true);
    expect(wrapper.find('.batch-progress__phase').text()).toBe('Batch Import');
    expect(wrapper.find('.batch-progress__percent').text()).toBe('100%');
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

  // ── Chunk rebuild widget ────────────────────────────────────────────────

  function mockFullTextCount(n: number) {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(n);
      if (cmd === 'get_batch_import_progress') return Promise.resolve(IDLE_PROGRESS);
      if (cmd === 'get_rebuild_chunks_progress') return Promise.resolve(REBUILD_IDLE);
      if (cmd === 'start_rebuild_chunks') {
        return Promise.resolve({ ...REBUILD_IDLE, isRunning: true, phase: 'chunks', total: 3 });
      }
      return Promise.resolve(undefined);
    });
  }

  it('rebuildChunks_starts_async_task_and_reveals_bar', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();

    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    expect(btn).toBeTruthy();
    await btn!.trigger('click');
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('start_rebuild_chunks');
    expect(wrapper.find('.rebuild-chunks__bar').exists()).toBe(true);
    expect(wrapper.find('.batch-progress__phase').text()).toBe('Rebuilding text chunks');
  });

  it('rebuildBar_hidden_on_fresh_mount', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.rebuild-chunks__bar').exists()).toBe(false);
  });

  it('rebuildBar_restored_on_mount_when_run_is_live', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'count_articles_with_full_text') return Promise.resolve(2);
      if (cmd === 'get_batch_import_progress') return Promise.resolve(IDLE_PROGRESS);
      if (cmd === 'get_rebuild_chunks_progress') {
        return Promise.resolve({
          ...REBUILD_IDLE,
          isRunning: true,
          phase: 'chunks',
          completed: 1,
          total: 4,
          percent: 25,
          message: 'Rebuilding text chunks...',
        });
      }
      return Promise.resolve(undefined);
    });

    const wrapper = mountCard();
    await flushPromises();

    expect(wrapper.find('.rebuild-chunks__bar').exists()).toBe(true);
    // Chunk-phase percent maps into the 0-90 display band: 25% -> 22%.
    expect(wrapper.find('.batch-progress__percent').text()).toBe('22%');
    expect(wrapper.find('.batch-progress__detail').text()).toContain('1 / 4 articles');
  });

  it('rebuildBar_renders_counts_skip_note_and_errors_from_event', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();
    // Reveal the widget the way the user does: click Start.
    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    await btn!.trigger('click');
    await flushPromises();

    const handler = eventHandlers.get('chunk-rebuild:progress');
    expect(handler).toBeTruthy();
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'chunks',
        completed: 5,
        total: 6,
        percent: 83,
        chunked: 4,
        failed: 1,
        skippedTranslated: 1,
        message: 'Rebuilding text chunks...',
        errors: ['Article art-9: File not found for article art-9: /x/gone.pdf'],
      },
    });
    await flushPromises();

    const summary = wrapper.find('.rebuild-chunks__bar .batch-progress__summary');
    expect(summary.text()).toContain('4 chunked, 1 failed');
    expect(summary.text()).toContain('1 skipped (translated)');
    const errors = wrapper.findAll('.rebuild-chunks__errors li');
    expect(errors).toHaveLength(1);
    expect(errors[0]?.text() ?? '').toContain('art-9');
  });

  it('rebuildBar_embedding_phase_subline_and_final_summary', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();
    // Reveal the widget the way the user does: click Start.
    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    await btn!.trigger('click');
    await flushPromises();

    const handler = eventHandlers.get('chunk-rebuild:progress');
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'embeddings',
        completed: 6,
        total: 6,
        percent: 100,
        message: 'Updating embeddings...',
      },
    });
    await flushPromises();
    expect(wrapper.find('.batch-progress__phase').text()).toBe('Updating embeddings');

    // Live per-article embedding progress drives the sub-line.
    const embedHandler = eventHandlers.get('embedding:progress');
    expect(embedHandler).toBeTruthy();
    embedHandler!({ payload: { processed: 2, total: 5 } });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar').text()).toContain('Embedding articles... 2/5');

    // Final summary line replaces the sub-line context on completion.
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: false,
        phase: 'done',
        completed: 6,
        total: 6,
        percent: 100,
        message: '6 chunked, 0 failed',
        embeddingSummary: 'Embeddings skipped: LLM not configured',
      },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Embeddings skipped: LLM not configured');
    expect(wrapper.find('.rebuild-chunks__bar').text()).not.toContain('Embedding articles...');
  });

  it('cancel_button_invokes_cancel_rebuild_chunks', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();
    // Reveal the widget the way the user does: click Start.
    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    await btn!.trigger('click');
    await flushPromises();

    eventHandlers.get('chunk-rebuild:progress')!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'chunks',
        completed: 1,
        total: 6,
        percent: 16,
        message: 'Rebuilding text chunks...',
      },
    });
    await flushPromises();

    const cancel = wrapper
      .find('.rebuild-chunks__bar')
      .findAll('button')
      .find((b) => b.text().includes('Cancel'));
    expect(cancel).toBeTruthy();
    await cancel!.trigger('click');
    await flushPromises();

    expect(mockInvoke).toHaveBeenCalledWith('cancel_rebuild_chunks');
  });

  it('chunk_percent_maps_to_0_90_band_and_hides_counter_during_embeddings', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();
    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    await btn!.trigger('click');
    await flushPromises();

    const handler = eventHandlers.get('chunk-rebuild:progress')!;
    // Chunk phase at 50% raw -> displayed in the 0-90 band (45%).
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'chunks',
        completed: 3,
        total: 6,
        percent: 50,
        message: 'Rebuilding text chunks...',
      },
    });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('45%');
    expect(wrapper.find('.batch-progress__detail').text()).toContain('3 / 6 articles');

    // Cascade phase: the "N / M articles" counter is hidden even though the
    // raw snapshot still carries chunk totals.
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'embeddings',
        completed: 6,
        total: 6,
        percent: 100,
        message: 'Updating embeddings...',
      },
    });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('90%');
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__detail').exists()).toBe(false);

    // Done: raw percent (100) again.
    handler!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: false,
        phase: 'done',
        completed: 6,
        total: 6,
        percent: 100,
        message: '6 chunked, 0 failed',
      },
    });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('100%');
  });

  it('cascade_band_90_100_driven_by_embedding_progress', async () => {
    mockFullTextCount(3);
    const wrapper = mountCard();
    await flushPromises();
    const btn = wrapper.findAll('button').find((b) => b.text().includes('Rebuild text chunks'));
    await btn!.trigger('click');
    await flushPromises();

    eventHandlers.get('chunk-rebuild:progress')!({
      payload: {
        ...REBUILD_IDLE,
        isRunning: true,
        phase: 'embeddings',
        completed: 6,
        total: 6,
        percent: 100,
        message: 'Updating embeddings...',
      },
    });
    await flushPromises();

    // No embedding events yet: fallback floor of the cascade band.
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('90%');

    // Halfway through embedding 4 articles: 90 + 10*2/4 = 95%.
    eventHandlers.get('embedding:progress')!({ payload: { processed: 2, total: 4 } });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('95%');
    expect(wrapper.find('.rebuild-chunks__bar').text()).toContain('Embedding articles... 2/4');

    // Complete: top of the band.
    eventHandlers.get('embedding:progress')!({ payload: { processed: 4, total: 4 } });
    await flushPromises();
    expect(wrapper.find('.rebuild-chunks__bar .batch-progress__percent').text()).toBe('100%');
  });
});
