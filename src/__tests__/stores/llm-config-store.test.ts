import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLlmConfigStore, MIN_CONTEXT_WINDOW_TOKENS } from '@/stores/llm-config';
import type { LlmConfig } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const savedConfig: LlmConfig = {
  provider: 'anthropic',
  endpointUrl: 'https://api.anthropic.com',
  apiKeyEncrypted: 'secret',
  modelName: 'claude-3',
  temperature: 0.7,
  skipTemperature: true,
  maxConcurrentRequests: 5,
  requestDelayMs: 200,
  contextWindowTokens: 100_000,
};

describe('useLlmConfigStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts with default config and null testResult', () => {
    const store = useLlmConfigStore();
    expect(store.config.provider).toBe('openai');
    expect(store.config.modelName).toBe('gpt-5-mini');
    expect(store.config.temperature).toBe(0.2);
    expect(store.config.maxConcurrentRequests).toBe(3);
    expect(store.testResult).toBeNull();
    expect(store.initialized).toBe(false);
    expect(store.loading).toBe(false);
  });

  it('fetch loads saved config when present', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(savedConfig);

    const store = useLlmConfigStore();
    await store.fetch();

    expect(tauriCommand).toHaveBeenCalledWith('get_llm_config');
    expect(store.config.provider).toBe('anthropic');
    expect(store.config.modelName).toBe('claude-3');
    expect(store.config.temperature).toBe(0.7);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
  });

  it('fetch keeps default when saved config is null', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(null);

    const store = useLlmConfigStore();
    await store.fetch();

    expect(store.config.provider).toBe('openai');
    expect(store.initialized).toBe(true);
  });

  it('fetchIfNeeded skips when initialized', async () => {
    const store = useLlmConfigStore();
    store.initialized = true;
    await store.fetchIfNeeded();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('invalidate resets to defaults and clears testResult', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(savedConfig);
    const store = useLlmConfigStore();
    await store.fetch();
    store.testResult = { success: true, message: 'ok' };

    store.invalidate();

    expect(store.config.provider).toBe('openai');
    expect(store.initialized).toBe(false);
    expect(store.testResult).toBeNull();
  });

  it('clearTestResult sets testResult to null', () => {
    const store = useLlmConfigStore();
    store.testResult = { success: false, message: 'err' };
    store.clearTestResult();
    expect(store.testResult).toBeNull();
  });

  describe('context window floor clamp on fetch', () => {
    it('clamps a legacy sub-floor contextWindowTokens up to the minimum', async () => {
      const legacy: LlmConfig = {
        ...savedConfig,
        contextWindowTokens: 4_000,
      };
      vi.mocked(tauriCommand).mockResolvedValue(legacy);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.config.contextWindowTokens).toBe(MIN_CONTEXT_WINDOW_TOKENS);
    });

    it('leaves an at-floor contextWindowTokens untouched', async () => {
      const atFloor: LlmConfig = {
        ...savedConfig,
        contextWindowTokens: MIN_CONTEXT_WINDOW_TOKENS,
      };
      vi.mocked(tauriCommand).mockResolvedValue(atFloor);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.config.contextWindowTokens).toBe(MIN_CONTEXT_WINDOW_TOKENS);
    });

    it('leaves an above-floor contextWindowTokens untouched', async () => {
      const above: LlmConfig = {
        ...savedConfig,
        contextWindowTokens: 50_000,
      };
      vi.mocked(tauriCommand).mockResolvedValue(above);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.config.contextWindowTokens).toBe(50_000);
    });
  });
});
