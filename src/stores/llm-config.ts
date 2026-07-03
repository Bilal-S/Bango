import { defineStore } from 'pinia';
import { ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import type { LlmConfig } from '@/types';

export interface TestResult {
  success: boolean;
  message: string;
}

/**
 * The minimum selectable context window, in tokens. The Settings UI slider
 * floor and the load-time clamp both use this so a legacy config below the
 * floor is transparently bumped up on load (keeping the badge and slider
 * consistent with the UI floor).
 */
export const MIN_CONTEXT_WINDOW_TOKENS = 16_000;

const DEFAULT_CONFIG: LlmConfig = {
  provider: 'openai',
  endpointUrl: 'https://api.openai.com/v1',
  apiKeyEncrypted: null,
  modelName: 'gpt-5-mini',
  temperature: 0.2,
  skipTemperature: false,
  maxConcurrentRequests: 3,
  requestDelayMs: 500,
  contextWindowTokens: 50000,
};

export const useLlmConfigStore = defineStore('llm-config', () => {
  const config = ref<LlmConfig>({ ...DEFAULT_CONFIG });
  const loading = ref(false);
  const initialized = ref(false);
  const testResult = ref<TestResult | null>(null);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const saved = await tauriCommand<LlmConfig | null>('get_llm_config');
      if (saved) {
        // Clamp a legacy sub-floor context window up to the minimum so the
        // badge and slider (whose min is now MIN_CONTEXT_WINDOW_TOKENS) stay
        // consistent with the persisted value. A config at/above the floor
        // is left untouched.
        if (saved.contextWindowTokens < MIN_CONTEXT_WINDOW_TOKENS) {
          saved.contextWindowTokens = MIN_CONTEXT_WINDOW_TOKENS;
        }
        config.value = saved;
      }
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    config.value = { ...DEFAULT_CONFIG };
    initialized.value = false;
    testResult.value = null;
  }

  function clearTestResult(): void {
    testResult.value = null;
  }

  return {
    config,
    loading,
    initialized,
    testResult,
    fetchIfNeeded,
    fetch,
    invalidate,
    clearTestResult,
  };
});
