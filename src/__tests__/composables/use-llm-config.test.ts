import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { nextTick } from 'vue';
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

  describe('clearTestResult reactivity (regression: infinite save loop)', () => {
    /* Regression for the infinite "Saving..." <-> "Not Tested" flicker.
       The clearTestResult watcher must use array-of-getters (not a getter
       returning a fresh object/array), otherwise it fires on every reactive
       touch - including store.fetch() reassigning config.value to an
       identical-shape object after every save - causing an endless
       save -> fetch -> clearTestResult -> save loop. */
    it('does NOT clear testResult when store.fetch reassigns identical config', async () => {
      const saved = {
        provider: 'openai',
        endpointUrl: 'https://api.openai.com/v1',
        apiKeyEncrypted: 'sk-test',
        modelName: 'gpt-5-mini',
        temperature: 0.2,
        skipTemperature: false,
        maxConcurrentRequests: 3,
        requestDelayMs: 500,
        contextWindowTokens: 50000,
      };
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_llm_config') return Promise.resolve({ ...saved });
        return Promise.resolve(undefined);
      });

      const c = useLlmConfig();
      await c.loadConfig();
      // Simulate a successful test result (e.g. set by Test Connection).
      c.testResult.value = { success: true, message: 'Connected' };
      expect(c.testResult.value).not.toBeNull();

      // Simulate `save()` -> `store.fetch()` reassigning config.value to a
      // NEW object with identical field values (the post-save encrypted-key
      // refresh). The watcher must NOT treat this as a change.
      await c.loadConfig();
      await nextTick();
      expect(c.testResult.value).not.toBeNull();
      expect(c.testResult.value?.success).toBe(true);
    });

    it('clears testResult on a real field change', async () => {
      const saved = {
        provider: 'openai',
        endpointUrl: 'https://api.openai.com/v1',
        apiKeyEncrypted: 'sk-test',
        modelName: 'gpt-5-mini',
        temperature: 0.2,
        skipTemperature: false,
        maxConcurrentRequests: 3,
        requestDelayMs: 500,
        contextWindowTokens: 50000,
      };
      vi.mocked(tauriCommand).mockResolvedValue({ ...saved });

      const c = useLlmConfig();
      await c.loadConfig();
      c.testResult.value = { success: true, message: 'Connected' };
      expect(c.testResult.value).not.toBeNull();

      // A genuine user edit must still clear the stale test result.
      c.config.value.modelName = 'gpt-5.4';
      await nextTick();
      expect(c.testResult.value).toBeNull();
    });
  });

  describe('debounced parameter auto-save', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('scheduleParamSave debounces save by default delay', async () => {
      // `save()` now re-fetches the config after persisting (so the in-memory
      // store reflects the post-save encrypted-blob state), so mock both
      // commands.
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'save_llm_config') return Promise.resolve(undefined);
        if (cmd === 'get_llm_config') return Promise.resolve(null);
        return Promise.resolve(undefined);
      });
      const c = useLlmConfig();
      c.scheduleParamSave();
      // Not called immediately.
      expect(tauriCommand).not.toHaveBeenCalled();
      // Fast-forward past the debounce window (600ms).
      await vi.advanceTimersByTimeAsync(600);
      expect(tauriCommand).toHaveBeenCalledWith('save_llm_config', expect.any(Object));
      // The post-save re-fetch also fires.
      expect(tauriCommand).toHaveBeenCalledWith('get_llm_config');
      expect(c.lastSavedAt.value).toBeGreaterThan(0);
    });

    it('cancelScheduledParamSave drops a pending save', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);
      const c = useLlmConfig();
      c.scheduleParamSave();
      c.cancelScheduledParamSave();
      await vi.advanceTimersByTimeAsync(1000);
      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('scheduling again resets the timer (only the last save fires)', async () => {
      // `save()` re-fetches after persisting, so mock both commands.
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'save_llm_config') return Promise.resolve(undefined);
        if (cmd === 'get_llm_config') return Promise.resolve(null);
        return Promise.resolve(undefined);
      });
      const c = useLlmConfig();
      c.scheduleParamSave();
      await vi.advanceTimersByTimeAsync(400); // before the 600ms window
      c.scheduleParamSave();
      await vi.advanceTimersByTimeAsync(400); // would have fired if not reset
      expect(tauriCommand).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(200); // 600ms since the second schedule
      // The save fires exactly once (debounce reset worked). The re-fetch
      // also fires once, so the total IPC call count is 2. Assert on the
      // save command specifically to make the intent clear.
      expect(tauriCommand).toHaveBeenCalledTimes(2);
      expect(
        vi.mocked(tauriCommand).mock.calls.filter(([cmd]) => cmd === 'save_llm_config')
      ).toHaveLength(1);
    });
  });
});
