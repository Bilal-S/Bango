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

  describe('isConfigured getter (mirrors backend has_config)', () => {
    it('is false before fetch (not initialized)', () => {
      const store = useLlmConfigStore();
      expect(store.isConfigured).toBe(false);
    });

    it('is true for a local provider (LM Studio) with no API key', async () => {
      // Local providers (lmStudio, ollama, llamaCpp) do not require an API
      // key. This is the LM Studio regression case: previously the gate
      // checked only apiKeyEncrypted and incorrectly returned false.
      const lmStudio: LlmConfig = {
        ...savedConfig,
        provider: 'lmStudio',
        apiKeyEncrypted: null,
        endpointUrl: 'http://localhost:1234/v1',
        modelName: 'local-model',
      };
      vi.mocked(tauriCommand).mockResolvedValue(lmStudio);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(true);
    });

    it('is true for Ollama with no API key', async () => {
      const ollama: LlmConfig = {
        ...savedConfig,
        provider: 'ollama',
        apiKeyEncrypted: null,
        endpointUrl: 'http://localhost:11434/v1',
        modelName: 'llama3',
      };
      vi.mocked(tauriCommand).mockResolvedValue(ollama);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(true);
    });

    it('is false for a cloud provider without an API key', async () => {
      const noKey: LlmConfig = {
        ...savedConfig,
        provider: 'openai',
        apiKeyEncrypted: null,
      };
      vi.mocked(tauriCommand).mockResolvedValue(noKey);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(false);
    });

    it('is true for a cloud provider with an API key', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(savedConfig);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(true);
    });

    it('is false when endpointUrl is empty even for a local provider', async () => {
      const noEndpoint: LlmConfig = {
        ...savedConfig,
        provider: 'lmStudio',
        apiKeyEncrypted: null,
        endpointUrl: '   ',
        modelName: 'local-model',
      };
      vi.mocked(tauriCommand).mockResolvedValue(noEndpoint);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(false);
    });

    it('is false when modelName is empty even with an API key', async () => {
      const noModel: LlmConfig = {
        ...savedConfig,
        provider: 'openai',
        apiKeyEncrypted: 'secret',
        modelName: '',
      };
      vi.mocked(tauriCommand).mockResolvedValue(noModel);

      const store = useLlmConfigStore();
      await store.fetch();

      expect(store.isConfigured).toBe(false);
    });

    it('is false after invalidate()', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(savedConfig);

      const store = useLlmConfigStore();
      await store.fetch();
      expect(store.isConfigured).toBe(true);

      store.invalidate();
      expect(store.isConfigured).toBe(false);
    });
  });
});
