import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLlmConfig } from '@/composables/use-llm-config';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

describe('useLlmConfig', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('exposes reactive state refs', () => {
    const c = useLlmConfig();
    expect(c.loading.value).toBe(false);
    expect(c.saving.value).toBe(false);
    expect(c.testing.value).toBe(false);
    expect(c.showApiKey.value).toBe(false);
    expect(c.fetchingModels.value).toBe(false);
    expect(c.fetchedModels.value).toBeNull();
    expect(c.config.value.provider).toBe('openai');
    expect(c.testResult.value).toBeNull();
  });

  it('loadConfig delegates to store.fetch', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(null);
    const c = useLlmConfig();
    await c.loadConfig();
    expect(tauriCommand).toHaveBeenCalledWith('get_llm_config');
  });

  it('save calls save_llm_config and toggles saving', async () => {
    let resolveSave: () => void;
    const p = new Promise<void>((r) => {
      resolveSave = r;
    });
    vi.mocked(tauriCommand).mockReturnValue(p);

    const c = useLlmConfig();
    const savePromise = c.save();
    expect(c.saving.value).toBe(true);
    resolveSave!();
    await savePromise;
    expect(c.saving.value).toBe(false);
    expect(tauriCommand).toHaveBeenCalledWith('save_llm_config', expect.any(Object));
  });

  it('testConnection success sets success testResult', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'save_llm_config') return Promise.resolve();
      if (cmd === 'get_llm_config') return Promise.resolve(null);
      if (cmd === 'test_llm_connection')
        return Promise.resolve({ success: true, message: 'Connected' });
      return Promise.resolve(undefined);
    });

    const c = useLlmConfig();
    await c.testConnection();
    expect(c.testing.value).toBe(false);
    expect(c.testResult.value).toEqual({ success: true, message: 'Connected' });
  });

  it('testConnection failure sets error testResult', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'save_llm_config') return Promise.resolve();
      if (cmd === 'test_llm_connection')
        return Promise.resolve({ success: false, message: 'Bad key' });
      return Promise.resolve(undefined);
    });

    const c = useLlmConfig();
    await c.testConnection();
    expect(c.testResult.value).toEqual({ success: false, message: 'Bad key' });
  });

  it('testConnection catches thrown exceptions', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'save_llm_config') return Promise.resolve();
      if (cmd === 'test_llm_connection') return Promise.reject(new Error('network'));
      return Promise.resolve(undefined);
    });

    const c = useLlmConfig();
    await c.testConnection();
    expect(c.testResult.value!.success).toBe(false);
    expect(c.testResult.value!.message).toBe('network');
  });

  it('isLocalProvider detects local providers', () => {
    const c = useLlmConfig();
    c.config.value.provider = 'ollama';
    expect(c.isLocalProvider()).toBe(true);
    c.config.value.provider = 'lmStudio';
    expect(c.isLocalProvider()).toBe(true);
    c.config.value.provider = 'llamaCpp';
    expect(c.isLocalProvider()).toBe(true);
    c.config.value.provider = 'openai';
    expect(c.isLocalProvider()).toBe(false);
  });

  it('fetchModels populates fetchedModels on success', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(['gpt-4', 'gpt-3.5']);
    const c = useLlmConfig();
    c.config.value.provider = 'openai';
    await c.fetchModels();
    expect(c.fetchedModels.value).toEqual(['gpt-4', 'gpt-3.5']);
    expect(c.testResult.value!.success).toBe(true);
    expect(c.testResult.value!.message).toContain('OpenAI');
  });

  it('fetchModels handles error', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('unreachable'));
    const c = useLlmConfig();
    await c.fetchModels();
    expect(c.fetchedModels.value).toBeNull();
    expect(c.testResult.value!.success).toBe(false);
    expect(c.testResult.value!.message).toContain('Failed to fetch models');
  });

  it('resetFetchedModels clears the list', () => {
    const c = useLlmConfig();
    c.fetchedModels.value = ['a', 'b'];
    c.resetFetchedModels();
    expect(c.fetchedModels.value).toBeNull();
  });

  it('revert invalidates store and clears models', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(null);
    const c = useLlmConfig();
    c.fetchedModels.value = ['x'];
    c.revert();
    expect(c.fetchedModels.value).toBeNull();
  });
});
