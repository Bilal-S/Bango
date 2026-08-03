import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useLlmConfigured } from '@/composables/use-llm-configured';
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

describe('useLlmConfigured', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('returns a computed ref that is false before the store initializes', () => {
    const isLlmConfigured = useLlmConfigured();
    expect(isLlmConfigured.value).toBe(false);
  });

  it('returns true after the store loads a configured provider', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(savedConfig);
    const store = useLlmConfigStore();
    const isLlmConfigured = useLlmConfigured();

    await store.fetch();

    expect(isLlmConfigured.value).toBe(true);
  });

  it('reactively tracks store.config mutations (the original bug fix)', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(savedConfig);
    const store = useLlmConfigStore();
    const isLlmConfigured = useLlmConfigured();

    await store.fetch();
    expect(isLlmConfigured.value).toBe(true);

    // Clearing the API key (as the Settings v-model does) must flip the gate.
    store.config.apiKeyEncrypted = null;
    expect(isLlmConfigured.value).toBe(false);

    // Re-typing a key must flip it back.
    store.config.apiKeyEncrypted = 'new-key';
    expect(isLlmConfigured.value).toBe(true);
  });

  it('returns false when a cloud provider has no API key', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ ...savedConfig, apiKeyEncrypted: null });
    const store = useLlmConfigStore();
    const isLlmConfigured = useLlmConfigured();

    await store.fetch();

    expect(isLlmConfigured.value).toBe(false);
  });

  it('returns true for a local provider (Ollama) with no API key', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({
      ...savedConfig,
      provider: 'ollama',
      apiKeyEncrypted: null,
      endpointUrl: 'http://localhost:11434/v1',
      modelName: 'llama3',
    });
    const store = useLlmConfigStore();
    const isLlmConfigured = useLlmConfigured();

    await store.fetch();

    expect(isLlmConfigured.value).toBe(true);
  });
});
