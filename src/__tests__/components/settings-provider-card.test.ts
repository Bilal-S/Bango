import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { createRouter, createMemoryHistory } from 'vue-router';

const mockTauriCommand = vi.fn();

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: vi.fn(() => true),
  tauriCommand: (...args: unknown[]) => mockTauriCommand(...args),
}));

import SettingsProviderCard from '@/components/settings/settings-provider-card.vue';
import { useFeatureFlags } from '@/composables/use-feature-flags';

/**
 * Regression suite for the premium Embedding Model override input. The bug:
 * the auto-save watcher used a "skip the first change" flag, so when the
 * stored value was empty (load produced no ref change) the flag swallowed the
 * user's first - and often only - edit event (paste/fill), the save never
 * fired, and the field came back blank after navigating away and back.
 */
describe('settings-provider-card embedding model override', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    // Default IPC: no stored override; any other command resolves null.
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_embedding_status') {
        return Promise.resolve({
          status: 'unknown',
          model: '',
          dimensions: 0,
          modelOverride: null,
        });
      }
      return Promise.resolve(null);
    });
    useFeatureFlags().isPremium.value = true;
  });

  afterEach(() => {
    useFeatureFlags().isPremium.value = false;
    vi.useRealTimers();
  });

  function mountCard(): VueWrapper {
    const pinia = createPinia();
    setActivePinia(pinia);
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/', component: { template: '<div />' } }],
    });
    return mount(SettingsProviderCard, {
      global: { plugins: [pinia, router] },
    });
  }

  function embeddingInput(wrapper: VueWrapper) {
    return wrapper.find('.provider-card__embedding-override input');
  }

  function saveCalls(): unknown[][] {
    return mockTauriCommand.mock.calls.filter(([cmd]) => cmd === 'set_embedding_model_override');
  }

  it('persists a single-event edit even when no value was stored (regression)', async () => {
    const wrapper = mountCard();
    await vi.advanceTimersByTimeAsync(0); // mount-time load resolves

    await embeddingInput(wrapper).setValue('text-embedding-3-large');
    await vi.advanceTimersByTimeAsync(600); // debounce elapses

    expect(mockTauriCommand).toHaveBeenCalledWith('set_embedding_model_override', {
      value: 'text-embedding-3-large',
    });
    wrapper.unmount();
  });

  it('redisplays the stored override after unmount + remount without re-saving', async () => {
    const first = mountCard();
    await vi.advanceTimersByTimeAsync(0);
    await embeddingInput(first).setValue('text-embedding-3-large');
    await vi.advanceTimersByTimeAsync(600);
    expect(saveCalls()).toHaveLength(1);
    first.unmount();

    // Navigate back: fresh mount, backend now returns the stored override.
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_embedding_status') {
        return Promise.resolve({
          status: 'unknown',
          model: '',
          dimensions: 0,
          modelOverride: 'text-embedding-3-large',
        });
      }
      return Promise.resolve(null);
    });
    const second = mountCard();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(600);

    expect((embeddingInput(second).element as HTMLInputElement).value).toBe(
      'text-embedding-3-large'
    );
    // The load is propagation: it must not trigger a second save.
    expect(saveCalls()).toHaveLength(1);
    second.unmount();
  });

  it('does not save when the load populates the field (propagation skip)', async () => {
    mockTauriCommand.mockImplementation((cmd: string) => {
      if (cmd === 'get_embedding_status') {
        return Promise.resolve({
          status: 'unknown',
          model: '',
          dimensions: 0,
          modelOverride: 'nomic-embed-text',
        });
      }
      return Promise.resolve(null);
    });
    const wrapper = mountCard();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(600);

    expect(
      mockTauriCommand.mock.calls.some(([cmd]) => cmd === 'set_embedding_model_override')
    ).toBe(false);
    wrapper.unmount();
  });

  it('hides the embedding override input for non-premium users', async () => {
    useFeatureFlags().isPremium.value = false;
    const wrapper = mountCard();
    await vi.advanceTimersByTimeAsync(0);

    expect(wrapper.find('.provider-card__embedding-override').exists()).toBe(false);
    wrapper.unmount();
  });

  it('loads the stored override when the premium flag flips true after mount', async () => {
    useFeatureFlags().isPremium.value = false;
    const wrapper = mountCard();
    await vi.advanceTimersByTimeAsync(0);
    expect(mockTauriCommand.mock.calls.some(([cmd]) => cmd === 'get_embedding_status')).toBe(false);

    useFeatureFlags().isPremium.value = true;
    await vi.advanceTimersByTimeAsync(0);
    expect(mockTauriCommand.mock.calls.some(([cmd]) => cmd === 'get_embedding_status')).toBe(true);
    wrapper.unmount();
  });

  it('flushes a pending debounced save on unmount', async () => {
    const wrapper = mountCard();
    await vi.advanceTimersByTimeAsync(0);
    await embeddingInput(wrapper).setValue('mistral-embed');

    // Unmount BEFORE the 600ms debounce elapses: the edit must still land.
    wrapper.unmount();
    expect(mockTauriCommand).toHaveBeenCalledWith('set_embedding_model_override', {
      value: 'mistral-embed',
    });
  });
});
